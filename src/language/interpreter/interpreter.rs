
use std::{ collections::{ HashMap, HashSet, VecDeque },
           env::{ consts::OS,
                  current_dir,
                  current_exe,
                  home_dir,
                  join_paths,
                  set_current_dir,
                  split_paths },
           fmt::{ self, Debug, Display, Formatter },
           fs::File,
           io::{ BufReader, Error, ErrorKind, Read, Write, copy, pipe, stdout, stderr },
           path::{ Path, PathBuf, Component, MAIN_SEPARATOR_STR, is_separator },
           process::{ Command, Stdio },
           rc::Rc,
           mem::replace,
           thread::Builder };

use glob::{ MatchOptions, Pattern, glob };

use hostname::get as get_hostname;

use crate::{ language::{ bytecode::{ Code, Instruction, FunctionRef },
                         compiler::CompileError,
                         native::{ NativeFunction, NativeFunctions, NativeVisibility },
                         data::{ value::{ ExecResult, Executable, Value },
                                 map_key::MapKey,
                                 methods::{ BoundMethod, MethodDefinition, method_key },
                                 range::Range,
                                 scoped_variables::{ ScopedValue, ValueReference,
                                                     ValueVisibility },
                                 types::{ StructValue, TypeId, TypeKind, TypeRegistry } },
                         parser::{ ParserError, parse_text },
                         tokenizer::Tokenizer,
                         text::{ buffer::Buffer, location::Location, read_buffer::ReadBuffer },
                         interpreter::{ Alias, iteration::Iteration,
                                        scope::Scope, modules::Prelude,
                                        redirection::{ Redirection, Output, configure } } },
             runtime::{ color::TtyColorMode, process::{ COMMANDS, invoke } } };

const MAIN_SCOPE: &str = "main";

const BANNER_TRUECOLOR: &str = include_str!("../../../banner_truecolor.txt");
const BANNER_256: &str = include_str!("../../../banner_256.txt");
const BANNER_MONO: &str = include_str!("../../../banner_mono.txt");


struct LoopFrame
{
    continue_target: usize,
    break_target: usize,
    scope: usize,
    stack_depth: usize,
    iteration_depth: usize,
    reference_depth: usize,
    redirection_depth: usize,
}


pub enum ErrorWhat
{
    ParserError(ParserError),
    CompileError(CompileError),
    InvalidOperand(String),
    ArithmeticError(String),
    ArrayError(String),
    HashMapError(String),
    RangeError(String),
    MatchError,
    ModuleError(String),
    IterationError(String),
    LoopControlError(String),
    CommandNotFound(String, Location),
    ArgumentMismatch(String),
    FileGlobError(String),
    StackUnderflow,
    NoResult,
    ReturnOutsideFunction,
    ExecutableNotFound(String),
    ExecutableIoError(String),
    RedirectionError(String),
    ExecutableBadReturn(u8),
    ExecutableSignaled,
    InitialScopePopAttempt
}


impl Display for ErrorWhat
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
    {
        match self
        {
            ErrorWhat::ParserError(error) => write!(f, "Parser error: {}.", error),
            ErrorWhat::CompileError(error) => write!(f, "Compile error: {}.", error),
            ErrorWhat::InvalidOperand(message) => write!(f, "Invalid operand: {}.", message),
            ErrorWhat::ArithmeticError(message) => write!(f, "Arithmetic error: {}.", message),
            ErrorWhat::ArrayError(message) => write!(f, "Array error: {}.", message),
            ErrorWhat::HashMapError(message) => write!(f, "Hash map error: {}.", message),
            ErrorWhat::RangeError(message) => write!(f, "Range error: {}.", message),
            ErrorWhat::IterationError(message) => write!(f, "Iteration error: {}.", message),
            ErrorWhat::LoopControlError(message) => write!(f, "Loop control error: {}.", message),
            ErrorWhat::CommandNotFound(command, location) =>
                write!(f, "Command not found: {} at {}.", command, location),
            ErrorWhat::FileGlobError(message) => write!(f, "File glob error: {}.", message),
            ErrorWhat::StackUnderflow => write!(f, "Stack underflow"),
            ErrorWhat::NoResult => write!(
                f,
                "Attempted to access the last result, but there was none."
            ),
            ErrorWhat::ReturnOutsideFunction => write!(f, "Cannot return outside a function."),
            ErrorWhat::ExecutableNotFound(name) => write!(f, "Executable not found: {}.", name),
            ErrorWhat::ExecutableIoError(message) =>
                write!(f, "Executable I/O error: {}.", message),
            ErrorWhat::RedirectionError(message) => write!(f, "Redirection error: {}.", message),
            ErrorWhat::ModuleError(message) => write!(f, "Module error: {}.", message),
            ErrorWhat::MatchError => write!(f, "Match error: No arm matched the value."),
            ErrorWhat::ExecutableBadReturn(code) =>
                {
                    write!(f, "Executable returned error code: {}.", code)
                },

            ErrorWhat::ExecutableSignaled =>
                {
                    write!(f, "Executable was terminated by a signal.")
                },

            ErrorWhat::ArgumentMismatch(message) => write!(f, "{}", message),
            ErrorWhat::InitialScopePopAttempt => write!(f, "Attempted to pop the initial scope.")
        }
    }
}


pub struct InterpreterError
{
    pub(super) location: Location,
    pub(super) what: ErrorWhat
}


impl From<ParserError> for InterpreterError
{
    fn from(error: ParserError) -> Self
    {
        InterpreterError
            {
                location: Location::default(),
                what: ErrorWhat::ParserError(error)
            }
    }
}


impl From<CompileError> for InterpreterError
{
    fn from(error: CompileError) -> Self
    {
        InterpreterError
            {
                location: Location::default(),
                what: ErrorWhat::CompileError(error)
            }
    }
}


impl Display for InterpreterError
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
    {
        write!(f, "Interpreter error at {}: {}", self.location, self.what)
    }
}


impl Debug for InterpreterError
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
    {
        write!(f, "{}", self)
    }
}


pub type BuiltIn<'a> = Rc<dyn Fn(&mut Interpreter,
                                 &Location,
                                 &[Value]) -> InterpreterResult<()> + 'a>;

pub type InterpreterResult<T> = Result<T, InterpreterError>;



type ReadFunction = Rc<dyn Fn(&Interpreter) -> InterpreterResult<Value>>;


type SpecialVars = HashMap<&'static str, ReadFunction>;


pub enum Startup
{
    Login,
    NonLogin
}


pub enum Interactive
{
    Yes,
    YesWithoutBanner,
    No
}


pub struct Interpreter
{
    pub(super) scopes: HashMap<String, Scope>,
    pub(super) current_scope: String,
    pub(super) loading_modules: HashSet<String>,
    pub(super) prelude: Option<Prelude>,
    pub(super) prelude_loading: bool,
    pub(super) prelude_generation: usize,
    // A source file can define several generations of nominal types after reloads.
    pub(super) type_scopes: HashMap<TypeId, String>,
    special_vars: SpecialVars,
    pub(super) native_functions: NativeFunctions,
    captured_stdout: Option<Vec<u8>>,
    redirections: Vec<Redirection>,
    pub last_result: Option<Value>,
    pub exit_code: u8,
    pub halted: bool
}


#[derive(Clone, PartialEq, Eq)]
pub enum RcFile
{
    None,
    Default,
    Custom(PathBuf)
}


impl Interpreter
{
    pub(super) fn scope(&self) -> &Scope
    {
        self.scopes.get(&self.current_scope)
            .expect("The current scope must be registered.")
    }

    pub(super) fn scope_mut(&mut self) -> &mut Scope
    {
        self.scopes.get_mut(&self.current_scope)
            .expect("The current scope must be registered.")
    }

    fn exported_environment(&self, location: &Location) -> InterpreterResult<Vec<(String, String)>>
    {
        self.scope().variables.get_all_flattened()
            .iter()
            .filter(|(_, value)| value.exported == ValueVisibility::Exported)
            .map(|(key, _)|
                {
                    let text = self.read_raw_variable(key, location)?.as_text();
                    let text = if key == "$PATH"
                        {
                            self.eval_path_list_from(&text)
                        }
                        else
                        {
                            self.eval_path_from(&text)
                        };

                    Ok((key.strip_prefix('$').unwrap_or(key.as_str()).to_string(), text))
                })
            .collect()
    }

    pub fn new(startup: Startup,
               interactive: Interactive,
               color_mode: TtyColorMode,
               tab_width: usize,
               rc_file: RcFile,
               script_args: Vec<String>) -> Self
    {
        let mut bodies: HashMap<&'static str, BuiltIn<'static>> = HashMap::from([
                (
                    "cd",
                    Rc::new(Interpreter::handle_cd) as BuiltIn<'static>
                ),

                (
                    "visible",
                    Rc::new(Interpreter::handle_visible) as BuiltIn<'static>
                ),

                (
                    "prelude_reload",
                    Rc::new(Interpreter::handle_prelude_reload) as BuiltIn<'static>
                ),

                (
                    "exit",
                    Rc::new(Interpreter::handle_exit) as BuiltIn<'static>
                )
            ]);

        for &name in COMMANDS
        {
            bodies.insert(name, Rc::new(move |interpreter, location, args|
                {
                    let environment = interpreter.exported_environment(location)?;
                    let mut capture = None;
                    let result = invoke(name, args, &environment,
                        |path| interpreter.eval_path_from(path),
                        |command|
                            {
                                let redirected = configure(command, &interpreter.redirections)?;
                                if interpreter.captured_stdout.is_some() && !redirected
                                {
                                    let (mut reader, writer) = pipe()?;
                                    capture = Some(Builder::new().name("shelly-process".into())
                                        .spawn(move ||
                                            {
                                                let mut bytes = Vec::new();
                                                reader.read_to_end(&mut bytes)?;
                                                Ok::<_, Error>(bytes)
                                            })?);
                                    command.stdout(writer);
                                }
                                Ok(())
                            });
                    if let Some(worker) = capture
                    {
                        let bytes = worker.join()
                            .map_err(|_| Error::other("Process capture worker panicked"))
                            .and_then(|result| result)
                            .map_err(|error| Self::redirection_error(location, error))?;
                        if let Some(output) = interpreter.captured_stdout.as_mut()
                        { output.extend_from_slice(&bytes); }
                    }
                    let result = result.map_err(|error| InterpreterError
                            {
                                location: location.clone(),
                                what: ErrorWhat::InvalidOperand(error),
                            })?;
                    interpreter.last_result = Some(result);
                    Ok(())
                }));
        }

        let native_functions: NativeFunctions = bodies.into_iter().map(|(name, body)|
            (name, NativeFunction::new(name, NativeVisibility::Visible, body))).collect();

        let special_vars: SpecialVars = HashMap::from([
                (
                    "$pwd",
                    Rc::new(|_interpreter: &Interpreter|
                        {
                            let cwd = current_dir().unwrap_or_else(|_| ".".into());
                            Ok(Value::from_string(cwd.display().to_string()))
                        }) as ReadFunction
                ),
                (
                    "$HOSTNAME",
                    Rc::new(|_interpreter: &Interpreter|
                        {
                            let name = get_hostname()
                                .map(|name| name.to_string_lossy().into_owned())
                                .unwrap_or_else(|_| "unknown".to_string());

                            Ok(Value::from_string(name))
                        }) as ReadFunction
                )
            ]);

        let mut new_self = Self
            {
                scopes: HashMap::from([
                        (MAIN_SCOPE.to_string(), Scope::new(native_functions.values().cloned(),
                            MAIN_SCOPE.to_string(), TypeRegistry::new()))
                    ]),
                current_scope: MAIN_SCOPE.to_string(),
                loading_modules: HashSet::new(),
                prelude: None,
                prelude_loading: false,
                prelude_generation: 0,
                type_scopes: HashMap::new(),
                special_vars,
                native_functions,
                captured_stdout: None,
                redirections: Vec::new(),
                last_result: None,
                exit_code: 0,
                halted: false
            };

        new_self.initialize_startup(startup,
                                    interactive,
                                    color_mode,
                                    tab_width,
                                    rc_file,
                                    script_args);

        new_self
    }

    /**
     * Load login profiles, then the prelude, then interactive initialization and the banner.
     */
    fn initialize_startup(&mut self,
                          startup: Startup,
                          interactive: Interactive,
                          color_mode: TtyColorMode,
                          tab_width: usize,
                          rc_file: RcFile,
                          script_args: Vec<String>)
    {
        let banner = match color_mode
            {
                TtyColorMode::TtyTrueColor => BANNER_TRUECOLOR,
                TtyColorMode::Tty256 => BANNER_256,
                TtyColorMode::TtyBasic | TtyColorMode::TtyMonochrome => BANNER_MONO
            };

        let is_interactive = matches!(
            interactive,
            Interactive::Yes | Interactive::YesWithoutBanner
        );

        self.set_variable("$build_date", Value::from_string(env!("SHELLY_BUILD_DATE").to_string()));
        self.set_variable("$build_time", Value::from_string(env!("SHELLY_BUILD_TIME").to_string()));
        self.set_variable("$version", Value::from_string(env!("CARGO_PKG_VERSION").to_string()));
        self.set_variable("$shelly", current_exe()
            .map(|path| Value::from_executable_string(path.display().to_string()))
            .unwrap_or_else(|_| Value::from_string(".".to_string())));
        self.set_variable("$OS",  Value::from_string(OS.to_string()));
        self.set_variable("$os",  Value::from_string(OS.to_string()));

        self.set_variable("$args",
            Value::from_array(script_args.iter().map(|arg|
                {
                    Value::from_string(arg.clone())
                })
                .collect()));

        if is_interactive
        {
            self.set_variable("$banner", Value::from_string(banner.to_string()));
            self.set_variable("$interactive", Value::Boolean(true));
        }
        else
        {
            self.set_variable("$interactive", Value::Boolean(false));
        }

        self.set_variable("$login", Value::Boolean(matches!(startup, Startup::Login)));
        if matches!(startup, Startup::Login)
        {
            self.load_startup_script(Path::new("/etc/shelly/profile.shy"), tab_width);

            if let Some(home) = home_dir()
            {
                self.load_startup_script(&home.join(".shelly_profile.shy"), tab_width);
            }
        }

        // Login profiles configure the standard-library path before its one-time load.
        if self.halted { return; }
        if let Err(error) = self.initialize_prelude()
        {
            eprintln!("Error loading standard prelude: {}", error);
            self.exit_code = 1;
            self.halted = true;
            return;
        }
        if self.halted { return; }

        if    is_interactive
           && rc_file != RcFile::None
        {
            let file = if let RcFile::Custom(file_path)= rc_file
                {
                    Some(file_path)
                }
                else
                {
                    if let Some(home) = home_dir()
                    {
                        Some(home.join(".shelly_init.shy"))
                    }
                    else
                    {
                        None
                    }
                };

            self.set_variable("$rc_path", Value::from_string(file.as_ref().map_or("".to_string(),
                |f| f.display().to_string())));

            if    let Some(file) = file
               && file.exists()
            {
                self.load_startup_script(&file, tab_width);
            }
            else
            {
                self.set_variable("$rc_path", Value::from_string("<not found>".to_string()));
            }
        }
        else
        {
            self.set_variable("$rc_path", Value::from_string("<unloaded>".to_string()));
        }

        if matches!(interactive, Interactive::Yes) && !self.halted
        {
            match self.evaluate_variable("$banner")
            {
                Ok(filtered_banner) => println!("{}", filtered_banner),

                Err(error) =>
                    {
                        self.set_variable("$banner", Value::from_string(banner.to_string()));
                        let banner = self.evaluate_variable("$banner").unwrap();

                        println!("{}", banner);
                        eprintln!("Error evaluating banner: {}", error)
                    }
            }
        }
    }

    /**
     * Execute an optional startup script, preserving its path in diagnostics.
     */
    fn load_startup_script(&mut self, path: &Path, tab_width: usize)
    {
        if self.halted
        {
            return;
        }

        let origin = self.eval_path_to(&path.to_string_lossy());
        let file = match File::open(path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::NotFound => return,
            Err(error) =>
                {
                    eprintln!("Error opening startup file {}: {}", origin, error);
                    return;
                }
        };

        let mut file_buffer = BufReader::new(file);
        let mut buffer = ReadBuffer::new(&origin, &mut file_buffer, Some(tab_width));

        if let Err(error) = self.execute_from_buffer(&mut buffer)
        {
            eprintln!("Error processing startup file {}: {}", origin, error);
        }
    }

    pub fn capture_stdout<T>(&mut self,
                             run: impl FnOnce(&mut Self) -> InterpreterResult<T>)
                             -> (InterpreterResult<T>, Vec<u8>)
    {
        let previous = self.captured_stdout.replace(Vec::new());
        let result = run(self);

        let captured = replace(&mut self.captured_stdout,
                               previous).expect("Capture buffer must be installed.");

        (result, captured)
    }

    pub fn has_command(&self, command: &str) -> bool
    {
        if self.scope().base_function(command).is_some()
        {
            return true;
        }

        self.module_native_function(command).is_some()
    }

    pub fn execute_command(&mut self,
                           location: Location,
                           command: &str,
                           args: Vec<String>) -> InterpreterResult<()>
    {
        let function = self.scope().base_function(command);

        if let Some(function) = function
        {
            self.execute_function(&location, command, &function,
                &args.iter().cloned().map(Value::from_string).collect::<Vec<_>>())?;
        }
        else if let Some(built_in) = self.module_native_function(command)
        {
            (built_in.body)(self, &location,
                &args.iter().cloned().map(Value::from_string).collect::<Vec<_>>())?;
        }
        else
        {
            return Err(InterpreterError
                {
                    location: location.clone(),
                    what: ErrorWhat::CommandNotFound(command.to_string(), location)
                });
        }

        Ok(())
    }

    pub fn execute_from_buffer(&mut self, buffer: &mut dyn Buffer) -> InterpreterResult<()>
    {
        if self.halted { return Ok(()); }
        let origin = buffer.location().origin.to_string();
        let mut tokenizer = Tokenizer::new(buffer);
        let statements = parse_text(&mut tokenizer)?;
        self.execute_submission(statements, &origin)
    }

    pub fn execute_instructions(&mut self, instructions: &Vec<Instruction>) -> InterpreterResult<()>
    {
        self.execute_instructions_with_arguments(instructions, &[], None)
    }

    fn execute_instructions_with_arguments(&mut self, instructions: &Vec<Instruction>,
                                           arguments: &[Value],
                                           receiver: Option<&ValueReference>)
                                           -> InterpreterResult<()>
    {
        let initial_scope = self.scope().variables.current_scope();
        let initial_redirections = self.redirections.len();
        let result =
            self.execute_instructions_scoped(instructions, initial_scope, arguments, receiver);

        let cleanup = self.finish_redirections(initial_redirections);
        let result = result.and(cleanup);

        self.scope_mut().variables.reset_to_scope(initial_scope);
        if result.is_err() { self.last_result = None; }

        result
    }

    fn execute_instructions_scoped(&mut self,
                                   instructions: &Vec<Instruction>,
                                   initial_scope: usize,
                                   function_arguments: &[Value],
                                   receiver: Option<&ValueReference>) -> InterpreterResult<()>
    {
        let mut stack: VecDeque<Value> = VecDeque::new();
        let mut references: Vec<ValueReference> = Vec::new();
        let mut iterations: Vec<Iteration> = Vec::new();
        let mut loops: Vec<LoopFrame> = Vec::new();
        let mut instruction_pointer: usize = 0;
        let mut location: Location = Location::default();

        while    instruction_pointer < instructions.len()
              && !self.halted
        {
            let instruction = &instructions[instruction_pointer];

            if let Some(new_location) = instruction.location.clone()
            {
                location = new_location;
            }

            match instruction.code
            {
                Code::BeginRedirect =>
                    {
                        let Some(Value::Array(parts)) = &instruction.operand else
                        { unreachable!("Invalid redirection operand"); };
                        let [Value::Integer(stream), Value::Boolean(variable),
                             Value::Integer(pending)] = parts.as_slice()
                        else { unreachable!("Invalid redirection operand"); };
                        let index = stack.len().checked_sub(*pending as usize)
                            .ok_or_else(|| InterpreterError
                                { location: location.clone(), what: ErrorWhat::StackUnderflow })?;
                        let target = stack.remove(index).ok_or_else(|| InterpreterError
                            { location: location.clone(), what: ErrorWhat::StackUnderflow })?;
                        self.begin_redirection(&location, *stream, *variable, target)?;
                    },

                Code::EndRedirect =>
                    {
                        self.finish_redirections(self.redirections.len() - 1)?;
                    },

                Code::RedirectSource =>
                    {
                        let Some(Value::Boolean(command_word)) = &instruction.operand
                        else { unreachable!("Invalid redirection source operand"); };
                        let value = self.last_result.take().ok_or_else(|| InterpreterError
                            { location: location.clone(), what: ErrorWhat::NoResult })?;
                        self.redirect_source(&location, value, *command_word)?;
                    },

                Code::Push =>
                    {
                        if let Some(operand) = &instruction.operand
                        {
                            Self::push(&mut stack, self.bind_executable(operand.clone()));
                        }
                        else
                        {
                            let message = "Missing operand for Push instruction.".to_string();

                            return Err(InterpreterError
                                {
                                    location: location.clone(),
                                    what: ErrorWhat::InvalidOperand(message)
                                });
                        }
                    },

                Code::ExitFunction =>
                    {
                        if !self.scope().in_function()
                        {
                            return Err(InterpreterError
                                {
                                    location: location.clone(),
                                    what: ErrorWhat::ReturnOutsideFunction
                                });
                        }

                        return Ok(());
                    },

                Code::TryExecute =>
                    {
                        let value = Self::pop(&location, &mut stack)?;
                        if matches!(value,
                            Value::String(_, Executable::Function(_) | Executable::Native(_)
                                | Executable::Method(_)))
                        {
                            self.execute_value(&location, value, Vec::new())?;
                            instruction_pointer += 1;
                            continue;
                        }
                        let executable = value.as_text();

                        if self.can_execute(&executable)
                        {
                            let value =
                                self.bind_executable(Value::from_executable_string(executable));
                            self.execute_value(&location, value, Vec::new())?;
                        }
                        else
                        {
                            self.last_result = Some(Value::from_string(executable));
                        }
                    },

                Code::MakeExecutable =>
                    {
                        let value = self.last_result.take().ok_or_else(|| InterpreterError
                            {
                                location: location.clone(),
                                what: ErrorWhat::NoResult
                            })?;
                        if matches!(value, Value::Enum(_) | Value::Struct(_) | Value::Terminal(_))
                        {
                            return Err(InterpreterError
                                {
                                    location: location.clone(),
                                    what: ErrorWhat::InvalidOperand(format!(
                                        "Cannot execute {} as a command",
                                        if matches!(value, Value::Enum(_))
                                        {
                                            "an enum"
                                        }
                                        else if matches!(value, Value::Terminal(_))
                                        {
                                            "a terminal"
                                        }
                                        else
                                        {
                                            "a struct"
                                        }
                                    )),
                                });
                        }
                        self.last_result = Some(match value
                            {
                                value @ Value::String(_,
                                    Executable::Function(_) | Executable::Native(_)
                                        | Executable::Method(_)) => value,
                                value =>
                                    self.bind_executable(
                                        Value::from_executable_string(value.as_text()))
                            });
                    },

                Code::ExecuteIfExecutable =>
                    {
                        match self.last_result.take()
                        {
                            Some(value @ Value::String(_,
                                                       Executable::Yes
                                                           | Executable::Native(_)
                                                           | Executable::Function(_)
                                                           | Executable::Method(_))) =>
                                {
                                    self.execute_value(&location, value, Vec::new())?;
                                },

                            Some(
                                value @ (Value::Array(_)
                                | Value::ArgumentExpansion(_)
                                | Value::HashMap(_)
                                | Value::Range(_)
                                | Value::Enum(_)
                                | Value::Struct(_)
                                | Value::Terminal(_)),
                            )
                                if matches!(instruction.operand, Some(Value::Boolean(true))) =>
                                {
                                    Self::command_name(&location, value)?;
                                },

                            Some(value) => self.last_result = Some(value),

                            None => return Err(InterpreterError
                                {
                                    location: location.clone(),
                                    what: ErrorWhat::NoResult
                                })
                        }
                    },

                Code::Execute =>
                    {
                        let mut args = Vec::new();

                        if let Some(operand) = &instruction.operand
                        {
                            let value = operand.as_integer();

                            if value < 0
                            {
                                let message = "Negative argument count for Execute instruction.";

                                return Err(InterpreterError
                                    {
                                        location: location.clone(),
                                        what: ErrorWhat::InvalidOperand(message.to_string())
                                    });
                            }

                            for _ in 0..value
                            {
                                let arg = Self::pop(&location, &mut stack)?;

                                if let Value::ArgumentExpansion(expanded_args) = arg
                                {
                                    for expanded_arg in expanded_args.iter().rev()
                                    {
                                        args.push(expanded_arg.clone());
                                    }
                                }
                                else
                                {
                                    args.push(arg);
                                }
                            }
                        }
                        else
                        {
                            return Err(InterpreterError
                                {
                                    location: location.clone(),
                                    what: ErrorWhat::InvalidOperand(
                                        "Missing operand for Execute instruction.".to_string())
                                });
                        }

                        args.reverse();

                        let mut executable = Self::pop(&location, &mut stack)?;
                        let name = Self::command_name(&location, executable.clone())?;

                        // If the executable name starts with a $ eval as a variable first.
                        if name.starts_with('$')
                        {
                            if name.contains('/')
                            {
                                executable = Value::from_executable_string(
                                    self.interpolate_string(&location, &name)?,
                                );
                            }
                            else
                            {
                                executable = self.read_raw_variable(&name, &location)?;
                            }
                        }

                        self.execute_value(&location, executable, args)?;
                    }

                Code::NewAlias =>
                    {
                        let definition = match &instruction.operand
                            {
                                Some(Value::Array(values)) => match values.as_slice()
                                    {
                                        [Value::String(alias, _), Value::Integer(count)]
                                            if !alias.is_empty() && *count >= 0 =>
                                                Some((alias, *count)),
                                        _ => None
                                    },
                                _ => None
                            };

                        let Some((alias, count)) = definition else
                        {
                            return Err(InterpreterError
                                {
                                    location: location.clone(),
                                    what: ErrorWhat::InvalidOperand(
                                        "NewAlias requires a nonempty alias name and a \
                                            nonnegative argument count."
                                            .to_string())
                                });
                        };

                        let mut arguments = Vec::new();
                        for _ in 0..count
                        {
                            arguments.push(Self::pop_as_text(&location, &mut stack)?);
                        }
                        arguments.reverse();
                        let target = Self::pop_as_text(&location, &mut stack)?;

                        self.scope_mut().aliases
                            .insert(alias.clone(), Alias { name: target, arguments });
                    },

                Code::BindParameter | Code::BindRestParameter =>
                    {
                        let invalid = |message| InterpreterError { location: location.clone(),
                            what: ErrorWhat::ArgumentMismatch(message) };
                        let Some(Value::Array(parts)) = &instruction.operand else
                        { return Err(invalid("Invalid parameter binding operand".to_string())); };
                        let [
                            Value::String(name, _),
                            constraint,
                            Value::Integer(index),
                            defaults @ ..,
                        ] = parts.as_slice()
                        else
                        {
                            return Err(invalid("Invalid parameter binding operand".to_string()));
                        };
                        let mut value = if matches!(instruction.code, Code::BindRestParameter)
                            {
                                Value::from_array(
                                    function_arguments.get(*index as usize..).unwrap_or(&[])
                                        .to_vec())
                            }
                            else
                            {
                                function_arguments.get(*index as usize).or_else(|| defaults.first())
                                    .cloned()
                                    .ok_or_else(
                                        || invalid(format!("Missing argument for parameter '{}'",
                                                           name)))?
                            };
                        let type_id = match constraint
                            {
                                Value::Integer(id) => Some(TypeId(*id as usize)),
                                Value::None => None,
                                _ => return Err(invalid("Invalid parameter constraint".to_string()))
                            };
                        if let Some(id) = type_id
                        {
                            value = self.scope().types.coerce(id, value).map_err(|message|
                                invalid(format!("Type error for parameter '{}': {}", name,
                                                message)))?;
                        }
                        let reference = if *index == 0 && name == "$self"
                            {
                                receiver.cloned().map(|mut reference|
                                    {
                                        if let Some(id) = type_id
                                        {
                                            reference.constraints
                                                .push((reference.indexes.len(), id));
                                        }
                                        reference
                                    })
                            }
                            else { None };
                        self.scope_mut().variables
                            .create(
                                name.clone(),
                                ScopedValue
                                    {
                                        value,
                                        type_id,
                                        exported: ValueVisibility::Private,
                                        reference,
                                    },
                            )
                            .map_err(invalid)?;
                    },

                Code::NewVariable | Code::BindIteration =>
                    {
                        let (variable_name, type_id) = match &instruction.operand
                            {
                                Some(Value::String(name, _)) => (name.clone(), None),
                                Some(Value::Array(parts)) if matches!(instruction.code,
                                    Code::NewVariable) =>
                                    {
                                        let [Value::String(name, _),
                                             Value::Integer(id)] = parts.as_slice() else
                                        { return Err(InterpreterError { location: location.clone(),
                                            what: ErrorWhat::InvalidOperand(
                                                "Invalid typed variable operand".to_string()) }); };
                                        (name.clone(), Some(TypeId(*id as usize)))
                                    },
                                _ => return Err(InterpreterError
                                    {
                                        location: location.clone(),
                                        what: ErrorWhat::InvalidOperand(
                                            "Missing or invalid operand for NewVariable \
                                                instruction."
                                                .to_string(),
                                        )
                                    })
                            };

                        let mut value = if matches!(instruction.code, Code::BindIteration)
                            { Self::pop(&location, &mut stack)? }
                            else if variable_name == "$HOME"
                            {
                                // Keep the old anchor until SetVariable resolves a shortened
                                // initializer, including `let $HOME = $HOME` in a nested scope.
                                self.read_raw_variable("$HOME", &location).unwrap_or(Value::None)
                            }
                            else { Value::None };
                        if variable_name == "$HOME" && let Value::String(path, _) = &mut value
                        {
                            *path = self.eval_path_from(path);
                        }
                        if let Err(error) = self.scope_mut().variables.create(variable_name,
                            ScopedValue
                                {
                                    value,
                                    type_id,
                                    exported: ValueVisibility::Private,
                                    reference: None,
                                })
                        {
                            return Err(InterpreterError
                                {
                                    location: location.clone(),
                                    what: ErrorWhat::InvalidOperand(
                                        format!("Failed to create variable: {}", error))
                                });
                        }
                    },

                Code::ValidateType =>
                    {
                        let Some(Value::Integer(id)) = instruction.operand
                        else
                        {
                            return Err(InterpreterError
                                {
                                    location: location.clone(),
                                    what: ErrorWhat::InvalidOperand(
                                        "Invalid ValidateType operand".to_string(),
                                    ),
                                });
                        };
                        let value = stack.back_mut().ok_or_else(|| InterpreterError
                            {
                                location: location.clone(),
                                what: ErrorWhat::InvalidOperand(
                                    "Missing value for type validation".to_string(),
                                ),
                            })?;
                        *value = self.scope().types
                            .coerce(TypeId(id as usize), value.clone())
                            .map_err(|message| InterpreterError
                                {
                                    location: location.clone(),
                                    what: ErrorWhat::InvalidOperand(
                                        format!("Type error: {}", message)),
                                })?;
                    },

                Code::SetVariable =>
                    {
                        let variable_name = match &instruction.operand
                            {
                                Some(Value::String(name, _)) => name.clone(),
                                _ => return Err(InterpreterError
                                    {
                                        location: location.clone(),
                                        what: ErrorWhat::InvalidOperand(
                                            "Missing or invalid operand for SetVariable \
                                                instruction."
                                                .to_string(),
                                        )
                                    })
                            };

                        let mut value = Self::pop(&location, &mut stack)?;

                        let type_id = self.variable_binding(&variable_name)
                            .and_then(|variable| variable.borrow().type_id);
                        match value
                        {
                            Value::ArgumentExpansion(array) if type_id.is_none() =>
                            {
                                value = Value::Array(array);
                            }
                            _ => {}
                        }

                        if variable_name == "$HOME" && let Value::String(path, _) = &mut value
                        {
                            *path = self.eval_path_from(path);
                        }

                        self.write_variable(&location, &variable_name, value)?;
                    },

                Code::StartIteration =>
                    {
                        let value = Self::pop(&location, &mut stack)?;
                        let snapshot = instruction.operand.as_ref().unwrap_or(&Value::None);
                        let iteration = Iteration::new(self, &location, value, snapshot)?;
                        iterations.push(iteration);
                    },

                Code::NextIteration =>
                    {
                        let iteration = iterations.last_mut().ok_or_else(|| InterpreterError
                            {
                                location: location.clone(),
                                what: ErrorWhat::InvalidOperand("No active iteration".to_string())
                            })?;
                        if let Some(value) = iteration.next(self, &location)?
                        {
                            Self::push(&mut stack, value);
                            self.last_result = Some(Value::Boolean(true));
                        }
                        else
                        {
                            self.last_result = Some(Value::Boolean(false));
                        }
                    },

                Code::EndIteration =>
                    {
                        iterations.pop().ok_or_else(|| InterpreterError
                            {
                                location: location.clone(),
                                what: ErrorWhat::InvalidOperand("No active iteration".to_string())
                            })?;
                        self.last_result = None;
                    },

                Code::MakeRange =>
                    {
                        let flags = match instruction.operand
                            {
                                Some(Value::Integer(flags)) if (0..=7).contains(&flags)
                                    && (flags & 4 == 0 || flags & 2 != 0) => flags,
                                _ => return Err(InterpreterError
                                    {
                                        location: location.clone(),
                                        what: ErrorWhat::InvalidOperand(
                                            "Invalid MakeRange flags.".to_string(),
                                        ),
                                    })
                            };
                        let end = if flags & 2 != 0
                        {
                            Some(Self::range_bound(
                                &location,
                                Self::pop(&location, &mut stack)?,
                            )?)
                        }
                        else
                        {
                            None
                        };
                        let start = if flags & 1 != 0
                        {
                            Some(Self::range_bound(
                                &location,
                                Self::pop(&location, &mut stack)?,
                            )?)
                        }
                        else
                        {
                            None
                        };
                        Self::push(
                            &mut stack,
                            Value::Range(Range
                                {
                                    start,
                                    end,
                                    inclusive: flags & 4 != 0,
                                }),
                        );
                    },

                Code::MakeHashMap =>
                    {
                        let count = match instruction.operand
                            {
                                Some(Value::Integer(count)) if count >= 0 => count as usize,
                                _ => return Err(InterpreterError
                                    {
                                        location: location.clone(),
                                        what: ErrorWhat::InvalidOperand(
                                            "Invalid MakeHashMap count.".to_string(),
                                        ),
                                    })
                            };
                        let mut pairs = Vec::new();
                        for _ in 0..count
                        {
                            let value = Self::pop(&location, &mut stack)?;
                            let key = Self::pop(&location, &mut stack)?;
                            pairs.push((MapKey::from_value(&key), value));
                        }
                        // Restore source order so the last duplicate key wins.
                        Self::push(
                            &mut stack,
                            Value::from_hash_map(pairs.into_iter().rev().collect()),
                        );
                    },

                Code::UnpackArray =>
                    {
                        let Some(Value::Integer(count)) = instruction.operand else
                        {
                            return Err(InterpreterError
                                {
                                    location: location.clone(),
                                    what: ErrorWhat::InvalidOperand(
                                        "Missing array destructuring length".to_string()),
                                });
                        };
                        let value = Self::pop(&location, &mut stack)?;
                        let values = match value
                            {
                                Value::Array(values) | Value::ArgumentExpansion(values) => values,
                                _ => return Err(InterpreterError
                                    {
                                        location: location.clone(),
                                        what: ErrorWhat::ArrayError(
                                            "Array destructuring requires an array".to_string()),
                                    })
                            };
                        if usize::try_from(count).ok() != Some(values.len())
                        {
                            return Err(InterpreterError
                                {
                                    location: location.clone(),
                                    what: ErrorWhat::ArrayError(format!(
                                        "Array destructuring expected {} elements, got {}",
                                        count, values.len())),
                                });
                        }
                        for value in values.iter() { Self::push(&mut stack, value.clone()); }
                    },

                Code::MakeArray =>
                    {
                        let count = match instruction.operand
                            {
                                Some(Value::Integer(count)) if count >= 0 => count as usize,
                                _ => return Err(InterpreterError
                                    {
                                        location: location.clone(),
                                        what: ErrorWhat::InvalidOperand(
                                            "Invalid MakeArray count.".to_string(),
                                        ),
                                    })
                            };
                        let mut elements = Vec::new();
                        for _ in 0..count { elements.push(Self::pop(&location, &mut stack)?); }
                        elements.reverse();
                        let mut array = Vec::new();
                        for element in elements
                        {
                            match element
                            {
                                Value::ArgumentExpansion(values) =>
                                    array.extend(Rc::unwrap_or_clone(values)),
                                value => array.push(value)
                            }
                        }
                        Self::push(&mut stack, Value::from_array(array));
                    },

                Code::MakeStruct =>
                    {
                        let invalid = |message| InterpreterError
                            {
                                location: location.clone(),
                                what: ErrorWhat::InvalidOperand(message),
                            };
                        let Some(Value::Array(parts)) = &instruction.operand else
                        { return Err(invalid("Invalid MakeStruct operand".to_string())); };
                        let [Value::Integer(id), Value::Array(indexes)] = parts.as_slice() else
                        { return Err(invalid("Invalid MakeStruct operand".to_string())); };
                        let definition = self.scope().types.get(TypeId(*id as usize));
                        let TypeKind::Struct(fields) = &definition.kind else
                        { return Err(invalid("Expected a struct definition".to_string())); };
                        let mut values = vec![Value::None; fields.len()];
                        for index in indexes.iter().rev()
                        {
                            let Value::Integer(index) = index
                            else
                            {
                                return Err(invalid("Invalid field index".to_string()));
                            };
                            values[*index as usize] = Self::pop(&location, &mut stack)?;
                        }
                        let value = Value::Struct(Rc::new(StructValue
                            {
                                definition,
                                fields: values,
                            }));
                        let value = self.scope().types
                            .coerce_value(value)
                            .map_err(|message| invalid(format!("Type error: {}", message)))?;
                        Self::push(&mut stack, value);
                    },

                Code::ReferenceVariable =>
                    {
                        let Some(Value::String(name, _)) = &instruction.operand
                        else { unreachable!("ReferenceVariable requires a variable name"); };
                        let reference = if let Some(reference) =
                            self.variable_reference(name)
                            { reference }
                            else
                            { ValueReference::temporary(self.read_variable(name, &location)?) };
                        self.read_reference(&location, &reference)?;
                        references.push(reference);
                    },

                Code::ReferenceValue =>
                    {
                        let value = Self::pop(&location, &mut stack)?;
                        references.push(ValueReference::temporary(value));
                    },

                Code::ReferenceIndex =>
                    {
                        let index = Self::pop(&location, &mut stack)?;
                        let mut reference = references.pop().expect("Missing receiver reference");
                        Self::get_element(&location,
                            &self.read_reference(&location, &reference)?, &index)?;
                        reference.indexes.push(index);
                        reference.fields.push(Value::Boolean(false));
                        references.push(reference);
                    },

                Code::ReferenceGroup =>
                    {
                        let mut reference = references.pop().expect("Missing receiver reference");
                        let value = self.read_reference(&location, &reference)?;
                        // Field mode executes a callable data member. An intermediate method
                        // already ran and produced a temporary reference with an empty path.
                        let execute = matches!(instruction.operand, Some(Value::Boolean(true)))
                            || matches!(instruction.operand, Some(Value::Integer(1)))
                                && !reference.indexes.is_empty();
                        if    execute
                           && matches!(value, Value::String(_, Executable::Yes
                                | Executable::Function(_) | Executable::Native(_)
                                | Executable::Method(_)))
                        {
                            let previous = self.last_result.take();
                            let result = self.execute_value(&location, value, Vec::new());
                            let value = self.last_result.take().unwrap_or(Value::None);
                            self.last_result = previous;
                            result?;
                            reference = ValueReference::temporary(value);
                        }
                        references.push(reference);
                    },

                Code::GetField | Code::BindField | Code::ReferenceField =>
                    {
                        let mut reference = references.pop().expect("Missing receiver reference");
                        let value = self.eval_value_paths_to(
                            self.read_reference(&location, &reference)?);
                        let Some(Value::Array(operands)) = &instruction.operand else
                        {
                            return Err(InterpreterError { location: location.clone(),
                                what: ErrorWhat::InvalidOperand(
                                    "Missing field operand".to_string()) });
                        };
                        let field = &operands[0];
                        let enum_index =
                            matches!(field, Value::String(name, _) if name == "index");
                        let result = if let Value::Enum(item) = &value && enum_index
                            {
                                reference.indexes.push(field.clone());
                                reference.fields.push(Value::Boolean(true));
                                Value::Integer(item.variant as i64)
                            }
                            else if let Value::Struct(item) = &value
                                && let Ok(index) = Self::struct_field_index(&location, item, field)
                            {
                                reference.indexes.push(field.clone());
                                reference.fields.push(Value::Boolean(true));
                                item.fields[index].clone()
                            }
                            else if let Value::String(name, _) = field
                                && let Some(method) = self.resolve_method(
                                    &value, reference.clone(), name, &operands[1])
                            {
                                if matches!(instruction.code, Code::BindField)
                                {
                                    Value::String(format!("{}.{}", value.type_name(), name),
                                        Executable::Method(Rc::new(method)))
                                }
                                else
                                {
                                    // Receiver and field operations preserve last_result.
                                    let previous = self.last_result.take();
                                    let result = self.execute_method(&location, &method, &[]);
                                    let value = self.last_result.take().unwrap_or(Value::None);
                                    self.last_result = previous;
                                    result?;
                                    reference = ValueReference::temporary(value.clone());
                                    value
                                }
                            }
                            else
                            {
                                if let Value::Struct(item) = &value
                                { Self::struct_field_index(&location, item, field)?; }
                                return Err(InterpreterError { location: location.clone(),
                                    what: ErrorWhat::InvalidOperand(format!(
                                        "Cannot access a field on {}", value.type_name())) });
                            };
                        if matches!(instruction.code, Code::ReferenceField)
                        { references.push(reference); }
                        else { Self::push(&mut stack, result); }
                    },

                Code::GetElement =>
                    {
                        let index = Self::pop(&location, &mut stack)?;
                        let collection = Self::pop(&location, &mut stack)?;
                        let value = Self::get_element(&location, &collection, &index)?;
                        Self::push(&mut stack, value);
                    },

                Code::SetElement =>
                    {
                        let (name, fields) = match &instruction.operand
                            {
                                Some(Value::Array(parts)) => match parts.as_slice()
                                    {
                                        [Value::String(name, _),
                                         Value::Array(fields)] if !fields.is_empty() =>
                                            (name, fields),
                                        _ => return Err(InterpreterError
                                            {
                                                location: location.clone(),
                                                what: ErrorWhat::InvalidOperand(
                                                    "Invalid SetElement operand.".to_string(),
                                                ),
                                            })
                                    },
                                _ => return Err(InterpreterError
                                    {
                                        location: location.clone(),
                                        what: ErrorWhat::InvalidOperand(
                                            "Missing SetElement operand.".to_string(),
                                        ),
                                    })
                            };
                        let value = Self::pop(&location, &mut stack)?;
                        let mut indexes = Vec::new();
                        for _ in 0..fields.len()
                        {
                            indexes.push(Self::pop(&location, &mut stack)?);
                        }
                        let value = match value
                            {
                                Value::ArgumentExpansion(values) if !matches!(fields.last(),
                                    Some(Value::Boolean(true))) => Value::Array(values),
                                value => value
                            };
                        indexes.reverse();
                        let mut updated = self.read_raw_variable(name, &location)?;
                        Self::set_element(&location, &mut updated, &indexes, fields, value)?;
                        let updated = self.scope().types
                            .coerce_value(updated)
                            .map_err(|message| InterpreterError
                                {
                                    location: location.clone(),
                                    what: ErrorWhat::InvalidOperand(
                                        format!("Type error: {}", message)),
                                })?;
                        self.write_variable(&location, name, updated)?;
                    },

                Code::GetVariable =>
                    {
                        let variable_name = match &instruction.operand
                            {
                                Some(Value::String(name, _)) => name.clone(),
                                _ => return Err(InterpreterError
                                    {
                                        location: location.clone(),
                                        what: ErrorWhat::InvalidOperand(
                                            "Missing or invalid operand for GetVariable \
                                                instruction."
                                                .to_string(),
                                        )
                                    })
                            };

                        let value = self.read_variable(&variable_name, &location)?;
                        Self::push(&mut stack, value);
                    },

                Code::PushResult =>
                    {
                        let result = self.last_result.take();

                        if let Some(result) = result
                        {
                            Self::push(&mut stack, result);
                        }
                        else
                        {
                            // Something was really wrong in the bytecode; we expected a result but
                            // found none. Error out hard.
                            self.halted = true;

                            return Err(InterpreterError
                                {
                                    location: location.clone(),
                                    what: ErrorWhat::NoResult
                                });
                        }
                    },

                Code::PopResult =>
                    {
                        // Attempt to pop a value from the stack and then store it as the last
                        // result.
                        let result = Self::pop(&location, &mut stack)?;
                        self.last_result = Some(result);
                    },

                Code::CheckResult =>
                    {
                        // Extract and clear the last result.
                        let result = self.last_result.take();

                        // If there was an actual value stored in the last result, check it. If the
                        // value was an execution result, check its return code. If it's bad, halt
                        // script execution.
                        if    let Some(result) = result
                           && let Value::ExecResult(result) = result
                        {
                            match result
                            {
                                ExecResult::Value(0) => {},

                                ExecResult::Value(code) =>
                                    {
                                        return Err(InterpreterError
                                            {
                                                location: location.clone(),
                                                what: ErrorWhat::ExecutableBadReturn(code)
                                            });
                                    },

                                ExecResult::Signaled =>
                                    {
                                        return Err(InterpreterError
                                            {
                                                location: location.clone(),
                                                what: ErrorWhat::ExecutableSignaled
                                            });
                                    }
                            }
                        }
                    },

                Code::ExportVariable =>
                    {
                        let variable_name = match &instruction.operand
                            {
                                Some(Value::String(name, _)) => name.clone(),
                                _ => return Err(InterpreterError
                                    {
                                        location: location.clone(),
                                        what: ErrorWhat::InvalidOperand(
                                            "Missing or invalid operand for ExportVariable \
                                                instruction."
                                                .to_string(),
                                        )
                                    })
                            };

                        if let Some(mut value) = self.scope_mut().variables.get_mut(&variable_name)
                        {
                            value.exported = ValueVisibility::Exported;
                        }
                        else
                        {
                            return Err(InterpreterError
                                {
                                    location: location.clone(),
                                    what: ErrorWhat::InvalidOperand(
                                        "Variable not found for ExportVariable \
                                            instruction.".to_string(),
                                    )
                                });
                        }
                    },

                Code::GlobFiles =>
                    {
                        let pattern = Self::pop_as_text(&location, &mut stack)?;
                        let expand_tilde = matches!(instruction.operand,
                                                    Some(Value::Boolean(true)));
                        stack.push_back(self.handle_file_glob(&pattern, expand_tilde)
                            .map_err(|mut error|
                                {
                                    error.location = location.clone();
                                    error
                                })?);
                    },

                Code::ExpandPath =>
                    {
                        let value = Self::pop(&location, &mut stack)?;
                        let path = self.eval_path_from(&value.as_text());
                        let expanded = match value
                            {
                                Value::String(_, executable) => Value::String(path, executable),
                                _ => Value::from_string(path)
                            };
                        Self::push(&mut stack, expanded);
                    },

                Code::ExpandArray =>
                    {
                        let mut value = Self::pop(&location, &mut stack)?;

                        match value
                        {
                            Value::Array(array) => value = Value::ArgumentExpansion(array),

                            Value::Range(range) =>
                                {
                                    let error = |message: &str| InterpreterError
                                        {
                                            location: location.clone(),
                                            what: ErrorWhat::RangeError(message.to_string())
                                        };
                                    let length = range.len()
                                        .ok_or_else(
                                            || error("Cannot expand a range with omitted bounds"))?;
                                    let length = usize::try_from(length)
                                        .map_err(|_| error("Range is too large to expand"))?;
                                    let mut values = Vec::new();
                                    values
                                        .try_reserve_exact(length)
                                        .map_err(|_| error("Range is too large to expand"))?;
                                    values.extend(range.iter().unwrap().map(Value::Integer));
                                    value = Value::from_argument_expansion(values);
                                },

                            Value::ArgumentExpansion(_) => {},

                            _ =>
                                {
                                    value = Value::from_argument_expansion(vec![value]);
                                }
                        }

                        Self::push(&mut stack, value);
                    },

                Code::InterpolateString | Code::InterpolateGlob =>
                    {
                        let escaped_dollars = match &instruction.operand
                            {
                                Some(Value::Array(offsets)) => offsets.iter()
                                    .map(|offset| offset.as_integer() as usize).collect(),
                                _ => Vec::new()
                            };
                        self.handle_string_interpolation(&location, &mut stack, &escaped_dollars,
                            matches!(instruction.code, Code::InterpolateGlob))?;
                    },

                Code::EnterScope =>
                    {
                        self.scope_mut().variables.push_scope();
                    },

                Code::ExitScope =>
                    {
                        if self.scope().variables.current_scope() == initial_scope
                        {
                            return Err(InterpreterError
                                {
                                    location: location.clone(),
                                    what: ErrorWhat::InitialScopePopAttempt
                                });
                        }

                        self.scope_mut().variables.pop_scope();
                    },

                Code::EnterLoop =>
                    {
                        let Some(Value::Array(targets)) = &instruction.operand else
                        {
                            return Err(InterpreterError
                                {
                                    location: location.clone(),
                                    what: ErrorWhat::InvalidOperand(
                                        "Expected two linked loop targets".to_string(),
                                    ),
                                });
                        };
                        if targets.len() != 2
                        {
                            return Err(InterpreterError
                                {
                                    location: location.clone(),
                                    what: ErrorWhat::InvalidOperand(
                                        "Expected two linked loop targets".to_string(),
                                    ),
                                });
                        }
                        loops.push(LoopFrame
                            {
                                continue_target: Self::jump_target(
                                    instructions,
                                    targets.first(),
                                    &location,
                                )?,
                                break_target: Self::jump_target(instructions, targets.get(1),
                                                                &location)?,
                                scope: self.scope().variables.current_scope(),
                                stack_depth: stack.len(),
                                iteration_depth: iterations.len(),
                                reference_depth: references.len(),
                                redirection_depth: self.redirections.len(),
                            });
                    },

                Code::ExitLoop =>
                    {
                        loops.pop().ok_or_else(|| InterpreterError
                            {
                                location: location.clone(),
                                what: ErrorWhat::InvalidOperand(
                                    "No active loop to exit".to_string()),
                            })?;
                    },

                Code::Break | Code::Continue =>
                    {
                        let frame = loops.last().ok_or_else(|| InterpreterError
                            {
                                location: location.clone(),
                                what: ErrorWhat::LoopControlError(format!(
                                    "Cannot {} outside a loop",
                                    if matches!(instruction.code, Code::Break)
                                    {
                                        "break"
                                    }
                                    else
                                    {
                                        "continue"
                                    }
                                )),
                            })?;
                        // A transfer may abandon nested blocks and partially evaluated
                        // expressions. Restore the state saved before the iteration body.
                        self.finish_redirections(frame.redirection_depth)?;
                        self.scope_mut().variables.reset_to_scope(frame.scope);
                        stack.truncate(frame.stack_depth);
                        iterations.truncate(frame.iteration_depth);
                        references.truncate(frame.reference_depth);
                        self.last_result = None;
                        instruction_pointer = if matches!(instruction.code, Code::Break)
                            { frame.break_target } else { frame.continue_target };
                        continue;
                    },

                Code::JumpTarget => {},

                Code::Jump | Code::JumpIfFalse | Code::JumpIfTrue =>
                    {
                        let target =
                            Self::jump_target(instructions, instruction.operand.as_ref(),
                                              &location)?;
                        let jump = if matches!(instruction.code, Code::Jump)
                            {
                                true
                            }
                            else
                            {
                                let Some(Value::Boolean(condition)) = &self.last_result else
                                {
                                    return Err(InterpreterError
                                        {
                                            location: location.clone(),
                                            what: ErrorWhat::InvalidOperand(
                                                "Expected a boolean jump condition".to_string(),
                                            ),
                                        });
                                };
                                *condition == matches!(instruction.code, Code::JumpIfTrue)
                            };
                        if jump
                        {
                            instruction_pointer = target;
                            continue;
                        }
                    },

                Code::ConvertType =>
                    {
                        let Some(Value::Integer(id)) = instruction.operand else
                        {
                            return Err(InterpreterError
                                {
                                    location: location.clone(),
                                    what: ErrorWhat::InvalidOperand(
                                        "Missing conversion target".to_string()),
                                });
                        };
                        let value = self.last_result.take().ok_or_else(|| InterpreterError
                            { location: location.clone(), what: ErrorWhat::NoResult })?;
                        self.last_result = Some(self.scope().types
                            .convert(TypeId(id as usize), &value)
                            .map_err(|message| InterpreterError
                                {
                                    location: location.clone(),
                                    what: ErrorWhat::InvalidOperand(
                                        format!("Type conversion error: {}", message)),
                                })?);
                    },

                Code::ToBoolean | Code::BooleanNot =>
                    {
                        let value = self.last_result.take().ok_or_else(|| InterpreterError
                            {
                                location: location.clone(),
                                what: ErrorWhat::NoResult
                            })?;
                        let value = value.as_bool();
                        self.last_result = Some(Value::Boolean(
                            if matches!(instruction.code, Code::BooleanNot)
                            {
                                !value
                            }
                            else
                            {
                                value
                            },
                        ));
                    },

                Code::Discard => { Self::pop(&location, &mut stack)?; },

                Code::MatchFail => return Err(InterpreterError
                    { location: location.clone(), what: ErrorWhat::MatchError }),

                Code::MatchPattern =>
                    {
                        let pattern = Self::pop(&location, &mut stack)?;
                        let subject = stack.back().ok_or_else(|| InterpreterError
                            { location: location.clone(), what: ErrorWhat::StackUnderflow })?;
                        let matched = match (&pattern, subject)
                            {
                                (Value::Range(range), Value::Integer(value)) =>
                                    range.contains(*value),
                                (Value::Range(_), _) => false,
                                _ => subject.equals(&pattern),
                            };
                        if matched { stack.pop_back(); }
                        self.last_result = Some(Value::Boolean(matched));
                    },

                Code::CompareEqual | Code::CompareNotEqual =>
                    {
                        let rhs = Self::pop(&location, &mut stack)?;
                        let lhs = Self::pop(&location, &mut stack)?;
                        let equal = lhs.equals(&rhs);
                        Self::push(
                            &mut stack,
                            Value::Boolean(
                                if matches!(instruction.code, Code::CompareEqual)
                                {
                                    equal
                                }
                                else
                                {
                                    !equal
                                },
                            ),
                        );
                    },

                Code::MathAdd | Code::MathSubtract | Code::MathMultiply
                | Code::MathDivide | Code::MathModulo =>
                    {
                        let error = |message: &str| InterpreterError
                            {
                                location: location.clone(),
                                what: ErrorWhat::ArithmeticError(message.to_string())
                            };
                        let rhs = Self::pop(&location, &mut stack)?;
                        let lhs = Self::pop(&location, &mut stack)?;
                        if    matches!(instruction.code, Code::MathAdd)
                           && let (Value::String(left, _), Value::String(right, _)) = (&lhs, &rhs)
                        {
                            let text = format!("{}{}", left, right);
                            Self::push(&mut stack, Value::from_string(text));
                            instruction_pointer += 1;
                            continue;
                        }
                        if let Some(message) = lhs
                            .arithmetic_error()
                            .or_else(|| rhs.arithmetic_error())
                        {
                            return Err(error(message));
                        }
                        if matches!(lhs, Value::Float(_, _)) || matches!(rhs, Value::Float(_, _))
                        {
                            let lhs = lhs.arithmetic_float()
                                .ok_or_else(|| error("Non-finite floating-point operand"))?;
                            let rhs = rhs.arithmetic_float()
                                .ok_or_else(|| error("Non-finite floating-point operand"))?;
                            if    rhs == 0.0
                               && matches!(instruction.code, Code::MathDivide | Code::MathModulo)
                            { return Err(error("Division or remainder by zero")); }
                            let result = match instruction.code
                                {
                                    Code::MathAdd => lhs + rhs,
                                    Code::MathSubtract => lhs - rhs,
                                    Code::MathMultiply => lhs * rhs,
                                    Code::MathDivide => lhs / rhs,
                                    Code::MathModulo => lhs % rhs,
                                    _ => unreachable!(),
                                };
                            if !result.is_finite()
                            { return Err(error("Floating-point overflow")); }
                            Self::push(&mut stack, Value::Float(result, None));
                            instruction_pointer += 1;
                            continue;
                        }
                        let (Value::Integer(lhs), Value::Integer(rhs)) = (lhs, rhs)
                        else { unreachable!("Arithmetic operands have been checked") };
                        if    rhs == 0
                           && matches!(instruction.code, Code::MathDivide | Code::MathModulo)
                        {
                            return Err(error("Division or remainder by zero"));
                        }
                        let result = match instruction.code
                            {
                                Code::MathAdd => lhs.checked_add(rhs),
                                Code::MathSubtract => lhs.checked_sub(rhs),
                                Code::MathMultiply => lhs.checked_mul(rhs),
                                Code::MathDivide => lhs.checked_div(rhs),
                                Code::MathModulo => lhs.checked_rem(rhs),
                                _ => unreachable!()
                            }.ok_or_else(|| error("Integer overflow"))?;
                        Self::push(&mut stack, Value::Integer(result));
                    }
            }

            instruction_pointer += 1;
        }

        Ok(())
    }

    pub fn set_variable(&mut self, name: &str, value: Value)
    {
        let _ = self.scope_mut().variables.create(name.to_string(), ScopedValue
            {
                value,
                type_id: None,
                exported: ValueVisibility::Private,
                reference: None,
            });
    }

    pub fn variable_names(&self) -> Vec<String>
    {
        let mut names: Vec<String> = self.scope().variables.names()
            .chain(self.special_vars.keys().copied()).map(str::to_string).collect();
        names.sort();
        names.dedup();
        names
    }

    pub fn evaluate_variable(&self, name: &str) -> InterpreterResult<String>
    {
        if self.variable_binding(name).is_none() && !self.special_vars.contains_key(name)
        {
            return Ok(String::new());
        }

        let location = Location::new(name, 1, 1);
        let value = self.read_variable(name, &location)?;

        self.interpolate_string_at(&location, &value.as_text(), &[], true, false)
    }

    /**
     * Read a filesystem setting, expanding its home-shortened form.
     */
    pub fn evaluate_path_variable(&self, name: &str) -> InterpreterResult<String>
    {
        if self.variable_binding(name).is_none() && !self.special_vars.contains_key(name)
        {
            return Ok(String::new());
        }
        let value = self.read_raw_variable(name, &Location::new(name, 1, 1))?.as_text();

        Ok(if name == "$PATH"
            {
                self.eval_path_list_from(&value)
            }
            else
            {
                self.eval_path_from(&value)
            })
    }

    /**
     * Preserve value types while shortening path strings, including array entries.
     */
    fn eval_value_paths_to(&self, value: Value) -> Value
    {
        match value
        {
            Value::Struct(mut item) =>
                {
                    let fields = item
                        .fields
                        .iter()
                        .cloned()
                        .map(|value| self.eval_value_paths_to(value))
                        .collect();
                    Rc::make_mut(&mut item).fields = fields;
                    Value::Struct(item)
                },
            Value::String(path, executable) => Value::String(self.eval_path_to(&path), executable),
            Value::Array(values) =>
                Value::from_array(Rc::unwrap_or_clone(values).into_iter()
                    .map(|value| self.eval_value_paths_to(value)).collect()),
            Value::ArgumentExpansion(values) =>
                Value::from_argument_expansion(Rc::unwrap_or_clone(values).into_iter()
                    .map(|value| self.eval_value_paths_to(value)).collect()),
            Value::HashMap(values) => Value::from_hash_map(Rc::unwrap_or_clone(values).into_iter()
                .map(|(key, value)| (key, self.eval_value_paths_to(value))).collect()),
            value => value
        }
    }

    fn read_variable(&self, name: &str, location: &Location) -> InterpreterResult<Value>
    {
        Ok(self.eval_value_paths_to(self.read_raw_variable(name, location)?))
    }

    fn read_projection(location: &Location, mut value: Value, indexes: &[Value],
                       fields: &[Value]) -> InterpreterResult<Value>
    {
        for (index, field) in indexes.iter().zip(fields)
        {
            value = if matches!(field, Value::Boolean(true))
                {
                    match &value
                    {
                        Value::Enum(item) if index.as_text() == "index" =>
                            Value::Integer(item.variant as i64),
                        Value::Struct(item) =>
                            item.fields[Self::struct_field_index(location, item, index)?].clone(),
                        _ => return Err(InterpreterError { location: location.clone(),
                            what: ErrorWhat::InvalidOperand(format!(
                                "Cannot access a field on {}", value.type_name())) }),
                    }
                }
                else { Self::get_element(location, &value, index)? };
        }
        Ok(value)
    }

    fn read_reference(&self, location: &Location,
                      reference: &ValueReference) -> InterpreterResult<Value>
    {
        let root = reference.root.borrow().value.clone();
        for (depth, id) in &reference.constraints
        {
            let value = Self::read_projection(location, root.clone(),
                &reference.indexes[..*depth], &reference.fields[..*depth])?;
            self.scope().types.validate(*id, &value).map_err(|message| InterpreterError
                {
                    location: location.clone(),
                    what: ErrorWhat::InvalidOperand(
                        format!("Type error for receiver: {}", message)),
                })?;
        }
        Self::read_projection(location, root, &reference.indexes, &reference.fields)
    }

    fn write_variable(&mut self, location: &Location, name: &str,
                      mut value: Value) -> InterpreterResult<()>
    {
        let invalid = |message| InterpreterError { location: location.clone(),
            what: ErrorWhat::InvalidOperand(message) };
        let variable = self.variable_binding(name)
            .ok_or_else(|| invalid("Variable not found for SetVariable instruction.".to_string()))?;
        if let Some(id) = variable.borrow().type_id
        {
            value = self.scope().types.coerce(id, value).map_err(|message|
                invalid(format!("Type error for '{}': {}", name, message)))?;
        }
        drop(variable);
        let reference = self.variable_reference(name).unwrap();
        self.write_receiver(location, name, &reference, value)
    }

    fn write_receiver(&mut self, location: &Location, name: &str,
                      reference: &ValueReference, value: Value) -> InterpreterResult<()>
    {
        let invalid = |message| InterpreterError { location: location.clone(),
            what: ErrorWhat::InvalidOperand(message) };
        let mut root = reference.root.borrow_mut();
        let mut updated = root.value.clone();
        Self::set_element(location, &mut updated, &reference.indexes, &reference.fields, value)?;
        updated = self.scope().types.coerce_value(updated)
            .map_err(|message| invalid(format!("Type error: {}", message)))?;
        if let Some(id) = root.type_id
        {
            updated = self.scope().types.coerce(id, updated)
                .map_err(|message| invalid(format!("Type error for '{}': {}", name, message)))?;
        }
        for (depth, id) in &reference.constraints
        {
            let value = Self::read_projection(location, updated.clone(),
                &reference.indexes[..*depth], &reference.fields[..*depth])?;
            let value = self.scope().types.coerce(*id, value)
                .map_err(|message| invalid(format!("Type error for receiver: {}", message)))?;
            Self::set_element(location, &mut updated, &reference.indexes[..*depth],
                &reference.fields[..*depth], value)?;
        }
        // Receiver promotion must also satisfy the original owner's constraints.
        self.scope().types.validate_value(&updated)
            .map_err(|message| invalid(format!("Type error: {}", message)))?;
        if let Some(id) = root.type_id
        {
            self.scope().types.validate(id, &updated)
                .map_err(|message| invalid(format!("Type error for '{}': {}", name, message)))?;
        }
        for (depth, id) in &reference.constraints
        {
            let value = Self::read_projection(location, updated.clone(),
                &reference.indexes[..*depth], &reference.fields[..*depth])?;
            self.scope().types.validate(*id, &value)
                .map_err(|message| invalid(format!("Type error for receiver: {}", message)))?;
        }
        root.value = updated;
        Ok(())
    }

    pub(super) fn read_raw_variable(&self, name: &str, location: &Location)
        -> InterpreterResult<Value>
    {
        if let Some(reference) = self.variable_reference(name)
        {
            self.read_reference(location, &reference)
        }
        else if let Some(value) = self.special_vars.get(name).cloned()
        {
            Ok(value(self)?)
        }
        else
        {
            Err(InterpreterError
                {
                    location: location.clone(),
                    what: ErrorWhat::InvalidOperand(
                        format!("Variable '{}' not found.", name))
                })
        }
    }

    fn bind_executable(&self, value: Value) -> Value
    {
        if let Value::String(name, Executable::Yes) = &value
        {
            if let Some(function) = self.module_native_function(name)
            { return Value::String(name.clone(), Executable::Native(function)); }
            let function = self.module_function(name);
            if let Some(function) = function
            { return Value::String(name.clone(), Executable::Function(function)); }
        }
        value
    }

    fn execute_value(
        &mut self, location: &Location, value: Value, args: Vec<Value>,
    ) -> InterpreterResult<()>
    {
        if let Value::String(name, Executable::Function(function)) = value
        { return self.execute_function(location, &name, &function, &args); }
        if let Value::String(_, Executable::Native(function)) = value
        { return (function.body)(self, location, &args); }
        if let Value::String(_, Executable::Method(method)) = value
        { return self.execute_method(location, &method, &args); }
        self.execute(location, Self::command_name(location, value)?, args)
    }

    pub(super) fn resolve_method(&self, value: &Value, receiver: ValueReference,
                      name: &str, snapshot: &Value) -> Option<BoundMethod>
    {
        for id in self.scope().types.method_types(self.scope().types.value_type(value))
        {
            let key = method_key(id, name);
            let bound = match snapshot
                {
                    Value::HashMap(methods) => methods.get(&MapKey::String(key.clone())),
                    _ => None,
                };
            let function = if let Some(Value::String(_, Executable::Function(function))) = bound
                { Some(function.clone()) }
                else
                { self.scope().lexical_function(&key).or_else(|| self.module_method(id, &key)) };
            let definition = if let Some(function) = function
                { MethodDefinition::User(function) }
                else if let Some(method) = self.scope().types.method(id, name)
                { MethodDefinition::Builtin(method) }
                else { continue; };
            return Some(BoundMethod
                { receiver, name: name.to_string(), definition });
        }
        None
    }

    pub(super) fn execute_method(&mut self, location: &Location, method: &BoundMethod,
                      arguments: &[Value]) -> InterpreterResult<()>
    {
        let mut receiver =
            self.eval_value_paths_to(self.read_reference(location, &method.receiver)?);
        let (minimum, maximum) = match &method.definition
            {
                MethodDefinition::Builtin(definition) =>
                    (definition.argument_count, Some(definition.argument_count)),
                MethodDefinition::User(function) =>
                    (function.minimum_arguments.saturating_sub(1),
                     (!function.variadic).then_some(function.arguments.len() - 1)),
            };
        if arguments.len() < minimum || maximum.is_some_and(|max| arguments.len() > max)
        {
            let expected = match maximum
                {
                    None => format!("at least {}", minimum),
                    Some(max) if max != minimum => format!("{} to {}", minimum, max),
                    Some(max) => max.to_string(),
                };
            return Err(InterpreterError { location: location.clone(),
                what: ErrorWhat::ArgumentMismatch(format!(
                    "Method '{}.{}' expected {} arguments, but got {}", receiver.type_name(),
                    method.name, expected, arguments.len())) });
        }
        match &method.definition
        {
            MethodDefinition::Builtin(definition) =>
                {
                    let value = (definition.body)(&mut receiver, arguments)
                        .map_err(|message| InterpreterError { location: location.clone(),
                            what: ErrorWhat::InvalidOperand(message) })?;
                    if definition.mutates_receiver
                    { self.write_receiver(location, "$self", &method.receiver, receiver)?; }
                    self.last_result = Some(value);
                    Ok(())
                },
            MethodDefinition::User(function) =>
                {
                    let mut values = Vec::with_capacity(arguments.len() + 1);
                    values.push(receiver.clone());
                    values.extend_from_slice(arguments);
                    self.execute_function_with_receiver(location,
                        &format!("{}::{}", receiver.type_name(), method.name),
                        function, &values, Some(&method.receiver))
                },
        }
    }

    fn can_execute(&self, executable: &str) -> bool
    {
        if    (executable.contains("::") && self.scope().types.module_names
                .contains(executable.split("::").next().unwrap()))
           || self.module_native_function(executable).is_some()
           || self.scope().aliases.contains_key(executable)
           || self.module_function(executable).is_some()
        {
            return true;
        }

        let is_executable = |path: &Path|
            {
                let Ok(metadata) = path.metadata() else { return false; };
                if !metadata.is_file()
                {
                    return false;
                }

                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    metadata.permissions().mode() & 0o111 != 0
                }

                #[cfg(not(unix))]
                {
                    true
                }
            };

        if executable.contains('/')
        {
            return is_executable(Path::new(&self.eval_path_from(executable)));
        }

        // Command uses the child's exported PATH, or the system default when it is absent.
        let path = self.scope().variables.get("$PATH")
            .filter(|value| value.exported == ValueVisibility::Exported)
            .map(|value| self.eval_path_list_from(&value.value.as_text()))
            .unwrap_or_else(|| "/bin:/usr/bin".to_string());

        split_paths(&path).any(|directory| is_executable(&directory.join(executable)))
    }

    fn execute_function(&mut self,
                        location: &Location,
                        name: &str,
                        function: &FunctionRef,
                        args: &[Value]) -> InterpreterResult<()>
    {
        self.execute_function_with_receiver(location, name, function, args, None)
    }

    fn execute_function_with_receiver(&mut self, location: &Location, name: &str,
                                      function: &FunctionRef, args: &[Value],
                                      receiver: Option<&ValueReference>) -> InterpreterResult<()>
    {
        if    args.len() < function.minimum_arguments
           || !function.variadic
           && args.len() > function.arguments.len()
        {
            let expected = if function.variadic
            {
                format!("at least {}", function.minimum_arguments)
            }
            else if function.minimum_arguments == function.arguments.len()
            {
                function.arguments.len().to_string()
            }
            else
            {
                format!(
                    "{} to {}",
                    function.minimum_arguments,
                    function.arguments.len()
                )
            };
            return Err(InterpreterError
                {
                    location: location.clone(),
                    what: ErrorWhat::ArgumentMismatch(format!(
                        "Function {} expected {} arguments, but got {}.",
                        name,
                        expected,
                        args.len()
                    )),
                });
        }

        let home = function.functions.borrow().scope.clone();
        if !self.scopes.contains_key(&home)
        {
            return Err(InterpreterError { location: location.clone(),
                what: ErrorWhat::ModuleError("The defining module failed to load".into()) });
        }
        let caller_scope = replace(&mut self.current_scope, home);
        let caller = self.scope_mut().enter_function(function.functions.clone());

        let call_result = self.execute_instructions_with_arguments(&function.code, args, receiver);

        self.scope_mut().exit_function(caller);
        self.current_scope = caller_scope;

        call_result?;
        if !self.halted && let Some(id) = function.return_type
        {
            let value = self.last_result.take().unwrap_or(Value::None);
            self.last_result = Some(self.scope().types.coerce(id, value).map_err(|message|
                InterpreterError
                    {
                        location: location.clone(),
                        what: ErrorWhat::ArgumentMismatch(format!(
                            "Type error for return value of '{}': {}", name, message)),
                    })?);
        }
        Ok(())
    }

    fn find_and_execute_function(&mut self,
                        location: &Location,
                        executable: &str,
                        args: &[Value]) -> InterpreterResult<bool>
    {
        if let Some(function) = self.module_function(executable)
        {
            self.execute_function(location, executable, &function, args)?;
            return Ok(true);
        }

        if executable.contains("::") && self.scope().types.module_names
            .contains(executable.split("::").next().unwrap())
        {
            return Err(InterpreterError
                {
                    location: location.clone(),
                    what: ErrorWhat::ModuleError(
                        format!("Module function '{}' is unavailable", executable)),
                });
        }
        Ok(false)
    }

    fn execute(&mut self,
               location: &Location,
               executable: String,
               args: Vec<Value>) -> InterpreterResult<()>
    {
        let (executable, resolved_args) = self.resolve_alias(location, &executable)?;
        let executable = self.eval_path_from(&executable);
        let args: Vec<Value> = resolved_args
            .into_iter()
            .map(Value::from_string)
            .chain(args)
            .collect();

        if let Some(built_in) = self.module_native_function(executable.as_str())
        {
            return (built_in.body)(self, location, &args);
        }

        if self.find_and_execute_function(location, &executable, &args)?
        {
            // Make sure `last_result` as been updated by the executed function.
            if self.last_result.is_none()
            {
                return Err(InterpreterError
                    {
                        location: location.clone(),
                        what: ErrorWhat::NoResult
                    });
            }

            return Ok(());
        }

        let env_vars = self.exported_environment(location)?;


        let mut command = Command::new(&executable);

        command.args(args.iter().map(|argument| self.eval_path_from(&argument.as_text())))
            .env_clear().envs(env_vars);

        command.stdin(Stdio::inherit()).stdout(Stdio::inherit()).stderr(Stdio::inherit());
        let redirected_output = configure(&mut command, &self.redirections)
            .map_err(|error| Self::redirection_error(location, error))?;
        let status_result = if self.captured_stdout.is_some() && !redirected_output
            {
                command
                    .stdout(Stdio::piped())
                    .output()
                    .map(|output|
                        {
                            self.captured_stdout
                                .as_mut()
                                .expect("Capture buffer must be installed.")
                                .extend_from_slice(&output.stdout);

                            output.status
                        })
            }
            else
            {
                command.status()
            };

        let status = status_result
            .map_err(|error| InterpreterError
                {
                    location: location.clone(),
                    what: match error.kind()
                    {
                        ErrorKind::NotFound =>
                            ErrorWhat::ExecutableNotFound(self.eval_path_to(&executable)),

                        _ => ErrorWhat::ExecutableIoError(
                            format!("'{}': {}", self.eval_path_to(&executable), error))
                    }
                })?;

        // Set the `last_result` to indicate the result of the executed command.
        let value = Value::from_status_code(status.code());
        self.last_result = Some(value);

        Ok(())
    }

    fn redirection_error(location: &Location, error: impl Display) -> InterpreterError
    {
        InterpreterError
            {
                location: location.clone(),
                what: ErrorWhat::RedirectionError(error.to_string()),
            }
    }

    fn begin_redirection(&mut self, location: &Location, stream: i64,
                          variable: bool, target: Value) -> InterpreterResult<()>
    {
        let Value::String(text, _) = target else
        {
            return Err(Self::redirection_error(location,
                "A redirection requires a string path or string variable"));
        };
        let mut redirect = Redirection::new(location.clone());
        let target_name = text.clone();
        let output = if variable
            {
                // Validate before running commands or replacing the binding.
                let binding = self.variable_binding(&text).ok_or_else(||
                    Self::redirection_error(location,
                        format!("Variable '{}' not found; declare it with let", text)))?;
                if let Some(id) = binding.borrow().type_id
                {
                    self.scope().types.validate(id, &Value::from_string(String::new()))
                        .map_err(|error| Self::redirection_error(location, error))?;
                }
                drop(binding);
                redirect.capture(text)
            }
            else
            {
                File::create(self.eval_path_from(&text))
                    .map(|file| Rc::new(Output::File(file)))
            };
        let result = output.map(|output|
            {
                if stream == 1 || stream == 3 { redirect.output = Some(output.clone()); }
                if stream == 2 || stream == 3 { redirect.error = Some(output); }
            });
        result.map_err(|error| Self::redirection_error(location,
            format!("'{}': {}", target_name, error)))?;
        self.redirections.push(redirect);
        Ok(())
    }

    fn finish_redirections(&mut self, depth: usize) -> InterpreterResult<()>
    {
        let mut result = Ok(());
        while self.redirections.len() > depth
        {
            let redirect = self.redirections.pop().unwrap();
            let location = redirect.location.clone();
            let finished = redirect.finish()
                .map_err(|error| Self::redirection_error(&location, error))
                .and_then(|capture|
                    {
                        if let Some((name, text)) = capture
                        { self.write_variable(&location, &name, Value::from_string(text))?; }
                        Ok(())
                    });
            result = result.and(finished);
        }
        result
    }

    fn redirect_source(&mut self, location: &Location, value: Value,
                        command_word: bool) -> InterpreterResult<()>
    {
        if    matches!(&value, Value::String(_, executable) if *executable != Executable::No)
           || (command_word && self.can_execute(&value.as_text()))
        {
            return self.execute_value(location, value, Vec::new());
        }
        let Value::String(path, _) = value else
        {
            return Err(Self::redirection_error(location, "A file path must be a string"));
        };
        let path = self.eval_path_from(&path);
        let result = File::open(&path).and_then(|mut file|
            {
                if let Some(output) = self.redirections.iter().rev()
                    .find_map(|redirect| redirect.output.as_ref())
                {
                    output.copy_from(&mut file)
                }
                else if let Some(capture) = self.captured_stdout.as_mut()
                {
                    copy(&mut file, capture)
                }
                else { copy(&mut file, &mut stdout().lock()) }
            });
        result.map_err(|error| Self::redirection_error(location,
            format!("'{}': {}", path, error)))?;
        self.last_result = Some(Value::from_status_code(Some(0)));
        Ok(())
    }

    fn write_stderr(&self, location: &Location, text: &str) -> InterpreterResult<()>
    {
        let result = if let Some(output) = self.redirections.iter().rev()
            .find_map(|redirect| redirect.error.as_ref())
            { output.copy_from(&mut text.as_bytes()).map(|_| ()) }
            else { stderr().lock().write_all(text.as_bytes()) };
        result.map_err(|error| Self::redirection_error(location, error))
    }

    fn command_name(location: &Location, value: Value) -> InterpreterResult<String>
    {
        match value
        {
            Value::Terminal(_) => Err(InterpreterError
                {
                    location: location.clone(),
                    what: ErrorWhat::InvalidOperand(
                        "Cannot execute a terminal as a command".to_string()),
                }),
            Value::Struct(_) => Err(InterpreterError
                {
                    location: location.clone(),
                    what: ErrorWhat::InvalidOperand(
                        "Cannot execute a struct as a command".to_string()),
                }),
            Value::Enum(_) => Err(InterpreterError
                {
                    location: location.clone(),
                    what: ErrorWhat::InvalidOperand(
                        "Cannot execute an enum as a command".to_string()),
                }),
            Value::Range(_) => Err(InterpreterError
                {
                    location: location.clone(),
                    what: ErrorWhat::RangeError("Cannot execute a range as a command".to_string())
                }),
            Value::HashMap(_) => Err(InterpreterError
                {
                    location: location.clone(),
                    what: ErrorWhat::HashMapError(
                        "Cannot execute a hash map as a command".to_string()),
                }),
            Value::Array(_) | Value::ArgumentExpansion(_) => Err(InterpreterError
                {
                    location: location.clone(),
                    what: ErrorWhat::ArrayError("Cannot execute an array as a command".to_string())
                }),
            value => Ok(value.as_text())
        }
    }

    fn push(stack: &mut VecDeque<Value>, value: Value)
    {
        stack.push_back(value);
    }

    fn jump_target(instructions: &[Instruction], operand: Option<&Value>,
                    location: &Location) -> InterpreterResult<usize>
    {
        let invalid = || InterpreterError
            {
                location: location.clone(),
                what: ErrorWhat::InvalidOperand("Expected a linked jump target".to_string())
            };
        let Some(Value::Integer(target)) = operand else { return Err(invalid()); };
        let target = usize::try_from(*target).map_err(|_| invalid())?;
        if !matches!(instructions.get(target), Some(Instruction { code: Code::JumpTarget, .. }))
        {
            return Err(invalid());
        }
        Ok(target)
    }

    fn range_bound(location: &Location, value: Value) -> InterpreterResult<i64>
    {
        match value
        {
            Value::Integer(value) => Ok(value),
            _ => Err(InterpreterError
                {
                    location: location.clone(),
                    what: ErrorWhat::RangeError("Range bounds must be integers".to_string())
                })
        }
    }

    // Validate without detaching shared storage. Reads only clone the selected
    // value; writes use Rc::make_mut on each array along the indexed path.
    fn array_index(location: &Location, elements: &[Value],
                    index: &Value) -> InterpreterResult<usize>
    {
        let error = |message| InterpreterError
            {
                location: location.clone(),
                what: ErrorWhat::ArrayError(message)
            };
        let Value::Integer(index) = index else
        {
            return Err(error("Array index must be an integer".to_string()));
        };
        let length = elements.len();
        let index = usize::try_from(*index)
            .map_err(
                |_| error(format!("Array index {} is out of bounds for length {}", index,
                                  length)))?;
        if index >= length
        {
            return Err(error(format!(
                "Array index {} is out of bounds for length {}",
                index, length
            )));
        }
        Ok(index)
    }

    fn struct_field_index(location: &Location, item: &StructValue,
                           field: &Value) -> InterpreterResult<usize>
    {
        let TypeKind::Struct(fields) = &item.definition.kind else { unreachable!(); };
        let index = match field
            {
                Value::Integer(index) => usize::try_from(*index)
                    .ok()
                    .filter(|index| *index < fields.len()),
                Value::String(name, _) => fields.iter().position(|field| field.name == *name),
                _ => None
            };
        index.ok_or_else(|| InterpreterError
            {
                location: location.clone(),
                what: ErrorWhat::InvalidOperand(format!(
                    "Unknown field '{}.{}'",
                    item.definition.name,
                    field.as_text()
                )),
            })
    }

    fn get_element(
        location: &Location, collection: &Value, index: &Value,
    ) -> InterpreterResult<Value>
    {
        match collection
        {
            Value::HashMap(values) => Ok(values
                .get(&MapKey::from_value(index))
                .cloned()
                .unwrap_or(Value::None)),
            Value::Array(values) => Ok(values[Self::array_index(location, values, index)?].clone()),
            _ => Err(InterpreterError
                {
                    location: location.clone(),
                    what: ErrorWhat::ArrayError(
                        "Cannot index a value that is not an array or hash map".to_string(),
                    ),
                })
        }
    }

    fn set_element(location: &Location, collection: &mut Value,
                    indexes: &[Value], fields: &[Value], value: Value) -> InterpreterResult<()>
    {
        let Some((index, rest)) = indexes.split_first() else
        {
            *collection = value;
            return Ok(());
        };
        if matches!(fields.first(), Some(Value::Boolean(true)))
        {
            if    matches!(collection, Value::Enum(_))
               && matches!(index, Value::String(name, _) if name == "index")
            {
                return Err(InterpreterError
                    {
                        location: location.clone(),
                        what: ErrorWhat::InvalidOperand("Enum index is read-only".to_string()),
                    });
            }
            let Value::Struct(item) = collection else
            {
                return Err(InterpreterError
                    {
                        location: location.clone(),
                        what: ErrorWhat::InvalidOperand(format!(
                            "Cannot assign a field on {}",
                            collection.type_name()
                        )),
                    });
            };
            let index = Self::struct_field_index(location, item, index)?;
            return Self::set_element(
                location,
                &mut Rc::make_mut(item).fields[index],
                rest,
                &fields[1..],
                value,
            );
        }
        match collection
        {
            Value::HashMap(values) =>
                {
                    let key = MapKey::from_value(index);
                    if rest.is_empty()
                    {
                        Rc::make_mut(values).insert(key, value);
                        return Ok(());
                    }
                    // Missing intermediate keys do not implicitly create containers.
                    if !values.contains_key(&key)
                    {
                        return Err(InterpreterError
                            {
                                location: location.clone(),
                                what: ErrorWhat::HashMapError(
                                    "Missing intermediate key in indexed assignment".to_string(),
                                ),
                            });
                    }
                    let child = Rc::make_mut(values).get_mut(&key).unwrap();
                    Self::set_element(location, child, rest, &fields[1..], value)
                },
            Value::Array(values) =>
                {
                    let index = Self::array_index(location, values, index)?;
                    Self::set_element(
                        location,
                        &mut Rc::make_mut(values)[index],
                        rest,
                        &fields[1..],
                        value,
                    )
                },
            _ => Err(InterpreterError
                {
                    location: location.clone(),
                    what: ErrorWhat::ArrayError(
                        "Cannot index a value that is not an array or hash map".to_string(),
                    ),
                })
        }
    }

    fn pop(location: &Location, stack: &mut VecDeque<Value>) -> InterpreterResult<Value>
    {
        stack.pop_back().ok_or(InterpreterError
            {
                location: location.clone(),
                what: ErrorWhat::StackUnderflow
            })
    }

    fn pop_as_text(location: &Location,
               stack: &mut VecDeque<Value>) -> InterpreterResult<String>
    {
        Ok(Self::pop(location, stack)?.as_text())
    }

    fn handle_string_interpolation(
        &self, location: &Location, stack: &mut VecDeque<Value>, escaped_dollars: &[usize],
        escape_glob: bool,
    ) -> InterpreterResult<()>
    {
        let invalid_operand = |message| InterpreterError
            {
                location: location.clone(),
                what: ErrorWhat::InvalidOperand(message)
            };

        let Value::String(text, executable) = Self::pop(location, stack)? else
        {
            return Err(invalid_operand(
                "Expected a string for InterpolateString instruction.".to_string()));
        };

        let interpolated = self.interpolate_string_at(
            location,
            &text,
            escaped_dollars,
            !escape_glob,
            escape_glob,
        )?;
        Self::push(stack, Value::String(interpolated, executable));
        Ok(())
    }

    fn interpolate_string(&self, location: &Location, text: &str) -> InterpreterResult<String>
    {
        self.interpolate_string_at(location, text, &[], true, false)
    }

    fn interpolate_string_at(&self, location: &Location, text: &str,
                             escaped_dollars: &[usize], display_paths: bool,
                             escape_glob: bool) -> InterpreterResult<String>
    {
        let invalid_operand = |message| InterpreterError
            {
                location: location.clone(),
                what: ErrorWhat::InvalidOperand(message)
            };

        let mut interpolated = String::with_capacity(text.len());
        let mut characters = text.char_indices().peekable();

        while let Some((offset, character)) = characters.next()
        {
            if character != '$' || escaped_dollars.binary_search(&offset).is_ok()
            {
                interpolated.push(character);
                continue;
            }

            let mut variable_name = String::from("$");

            if characters.peek().is_some_and(|(_, character)| *character == '{')
            {
                characters.next();
                let mut closed = false;

                for (_, character) in characters.by_ref()
                {
                    if character == '}'
                    {
                        closed = true;
                        break;
                    }

                    variable_name.push(character);
                }

                if !closed || variable_name.len() == 1
                {
                    return Err(invalid_operand(
                        "Expected a variable name and closing '}' in string interpolation."
                            .to_string()));
                }
            }
            else
            {
                // Braces delimit names explicitly; bare names end at punctuation.
                while let Some(&(_, character)) = characters.peek()
                {
                    if character == ':'
                    {
                        let mut probe = characters.clone();
                        probe.next();
                        if probe.next().is_some_and(|(_, c)| c == ':')
                            && probe.peek().is_some_and(|(_, c)| c.is_alphanumeric() || *c == '_')
                        {
                            characters.next();
                            characters.next();
                            variable_name.push_str("::");
                            continue;
                        }
                    }
                    if !character.is_alphanumeric() && character != '_'
                    {
                        break;
                    }

                    variable_name.push(character);
                    characters.next();
                }

                if variable_name.len() == 1
                {
                    interpolated.push('$');
                    continue;
                }
            }

            if variable_name.contains('=')
            {
                return Err(invalid_operand(
                    "Invalid variable name: '=' is not allowed.".to_string()));
            }

            let value = self.read_raw_variable(&variable_name, &location)?;
            let value = if display_paths { self.eval_value_paths_to(value) } else { value };

            // Append values directly so their contents are not interpolated again.
            let text = value.as_text();
            let text = if escape_glob { self.eval_path_from(&text) } else { text };
            interpolated.push_str(&if escape_glob { Pattern::escape(&text) } else { text });
        }

        Ok(interpolated)
    }

    /**
     * Resolve the current shell's home directory without interpolating its contents.
     */
    fn home_path(&self) -> Option<String>
    {
        self.scope().variables.get("$HOME")
            .map(|home| home.value.as_text())
            .filter(|home| !home.is_empty())
            .or_else(|| home_dir().map(|home| home.to_string_lossy().into_owned()))
            .filter(|home| Path::new(home).is_absolute())
    }

    /**
     * Shorten a leading home directory for display, matching complete path segments.
     */
    fn eval_path_to(&self, path: &str) -> String
    {
        let Some(home) = self.home_path() else { return path.to_string(); };
        let home = home.trim_end_matches(is_separator);

        // A root home must not become an empty prefix that matches relative paths.
        if home.is_empty()
        {
            return if path == MAIN_SEPARATOR_STR
                {
                    "~".to_string()
                }
                else if path.starts_with(is_separator)
                {
                    format!("~{}", path)
                }
                else
                {
                    path.to_string()
                };
        }

        if path == home
        {
            return "~".to_string();
        }

        if    let Some(suffix) = path.strip_prefix(home)
           && suffix.starts_with(is_separator)
        {
            return format!("~{}", suffix);
        }

        path.to_string()
    }

    /**
     * Expand only the current user's leading ~ or ~/ prefix. Callers decide
     * whether the source word is eligible; quoted words and variables are literal.
     */
    pub(super) fn eval_path_from(&self, path: &str) -> String
    {
        let Some(suffix) = path.strip_prefix('~') else { return path.to_string(); };
        if !suffix.is_empty() && !suffix.starts_with(is_separator)
        {
            return path.to_string();
        }

        let Some(home) = self.home_path() else { return path.to_string(); };
        if suffix.is_empty()
        {
            return home;
        }

        format!("{}{}", home.trim_end_matches(is_separator), suffix)
    }

    /**
     * PATH entries must be real paths when searching for or launching programs.
     */
    fn eval_path_list_from(&self, paths: &str) -> String
    {
        let expanded = split_paths(paths)
            .map(|path| PathBuf::from(self.eval_path_from(&path.to_string_lossy())));

        join_paths(expanded)
            .map(|paths| paths.to_string_lossy().into_owned())
            .unwrap_or_else(|_| paths.to_string())
    }

    fn handle_file_glob(&self, pattern: &str, expand_tilde: bool) -> InterpreterResult<Value>
    {
        // Glob results may omit an explicit "./" prefix or normalize separators.
        // Normalize both sides for matching without resolving parent directories.
        fn normalized_path(path: &Path) -> PathBuf
        {
            path.components()
                .filter(|component| !matches!(component, Component::CurDir))
                .collect()
        }

        let display_pattern = self.eval_path_to(pattern);
        let expanded = if expand_tilde
        {
            self.eval_path_from(pattern)
        }
        else
        {
            pattern.to_string()
        };
        let pattern = if expanded != pattern
            {
                let suffix = pattern.strip_prefix('~').unwrap();
                let home = expanded.strip_suffix(suffix).unwrap();
                format!("{}{}", Pattern::escape(home), suffix)
            }
            else
            {
                expanded
            };

        let options = MatchOptions
            {
                require_literal_separator: true,
                require_literal_leading_dot: true,
                ..MatchOptions::new()
            };

        let invalid_pattern = |error| InterpreterError
            {
                location: Location::default(),
                what: ErrorWhat::InvalidOperand(
                    format!("Invalid glob pattern '{}': {}", display_pattern, error))
            };

        let matcher = Pattern::new(
            &normalized_path(Path::new(&pattern)).to_string_lossy())
            .map_err(&invalid_pattern)?;

        // glob_with's leading-dot option prunes even explicitly requested hidden
        // entries. Enumerate normally, then enforce that rule with Pattern instead.
        let paths = glob(&pattern).map_err(invalid_pattern)?;

        let mut arguments = Vec::new();

        for entry in paths
        {
            let path = entry.map_err(|error| InterpreterError
                {
                    location: Location::default(),
                    what: ErrorWhat::FileGlobError(
                        format!("Failed to expand '{}': {}", display_pattern, error))
                })?;

            let path_text = path.to_string_lossy();

            // Inspect the final entry as written: Path::file_name normalizes
            // away a trailing "/.", which would hide that special entry.
            let entry_name = path_text.trim_end_matches(is_separator)
                .rsplit(is_separator).next();

            if matches!(entry_name, Some(".") | Some(".."))
            {
                continue;
            }

            if !matcher.matches_path_with(&normalized_path(&path), options)
            {
                continue;
            }

            arguments.push(Value::from_string(path_text.into_owned()));
        }

        if arguments.is_empty()
        {
            return Err(InterpreterError
                {
                    location: Location::default(),
                    what: ErrorWhat::FileGlobError(
                        format!("No paths matched glob pattern '{}'.", display_pattern))
                });
        }

        Ok(Value::from_argument_expansion(arguments))
    }

    fn resolve_alias(&self,
                     location: &Location,
                     command: &str) -> InterpreterResult<(String, Vec<String>)>
    {
        let mut name = command;
        let mut arguments = VecDeque::new();
        let mut visited = HashSet::new();

        while let Some(alias) = self.scope().aliases.get(name)
        {
            if !visited.insert(name)
            {
                return Err(InterpreterError
                    {
                        location: location.clone(),
                        what: ErrorWhat::InvalidOperand(
                            format!("Alias cycle detected at '{}' while resolving '{}'.",
                                    name, command))
                    });
            }

            // Target arguments precede the arguments of aliases that refer to it.
            for argument in alias.arguments.iter().rev()
            {
                arguments.push_front(argument.clone());
            }

            // An alias such as `ls = ls -h` adds defaults to the real command.
            // Apply it once, including when reached through another alias.
            if alias.name == name
            {
                break;
            }

            name = &alias.name;
        }

        Ok((name.to_string(), arguments.into_iter().collect()))
    }

    fn handle_cd(&mut self, location: &Location, args: &[Value]) -> InterpreterResult<()>
    {
        let args = args.iter().map(Value::as_text).collect::<Vec<_>>();
        let args = args.as_slice();
        if args.len() != 1
        {
            self.write_stderr(location, "Usage: cd <directory>\n")?;
            self.last_result = Some(Value::ExecResult(ExecResult::Value(1)));
            return Ok(());
        }

        if let Err(error) = set_current_dir(self.eval_path_from(&args[0]))
        {
            self.write_stderr(location, &format!("Failed to change directory to {}: {}\n",
                self.eval_path_to(&args[0]), error))?;
            self.last_result = Some(Value::ExecResult(ExecResult::Value(1)));
            return Ok(());
        }

        self.last_result = Some(Value::ExecResult(ExecResult::Value(0)));
        Ok(())
    }

    fn handle_exit(&mut self, location: &Location, args: &[Value]) -> InterpreterResult<()>
    {
        let args = args.iter().map(Value::as_text).collect::<Vec<_>>();
        let args = args.as_slice();
        self.exit_code = match args
            {
                [] => 0,
                [code] => code.parse::<u8>().map_err(|_| InterpreterError
                    {
                        location: location.clone(),
                        what: ErrorWhat::ArgumentMismatch(
                            "exit expects a status from 0 to 255.".to_string(),
                        ),
                    })?,
                _ => return Err(InterpreterError
                    {
                        location: location.clone(),
                        what: ErrorWhat::ArgumentMismatch(
                            "exit expects at most one argument.".to_string(),
                        ),
                    })
            };
        self.halted = true;
        self.last_result = Some(Value::None);

        Ok(())
    }
}
