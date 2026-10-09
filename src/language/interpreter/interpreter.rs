
use std::{ cell::RefCell,
           collections::{ HashMap, HashSet, VecDeque },
           env::consts::OS,
           fmt::{ self, Debug, Display, Formatter },
           fs::File,
           io::{ self, BufReader },
           path::{ Path, PathBuf },
           process::{ Command, Stdio },
           rc::Rc };

use crate::{ language::{ bytecode::{ Code,
                                     Instruction,
                                     FunctionBlockRef,
                                     FunctionRef,
                                     FunctionBlock },
                         compiler::{ compile_ast, CompileError, CompileTarget },
                         data::{ value::{ ExecResult, Executable, Value },
                                 map_key::MapKey,
                                 range::Range,
                                 scoped_variables::{ ScopedValue,
                                                     ScopedVariables,
                                                     ValueVisibility } },
                         parser::{ ParserError, parse_text },
                         tokenizer::Tokenizer,
                         text::{ buffer::Buffer,
                                 location::Location,
                                 read_buffer::ReadBuffer } },
             runtime::{ color::TtyColorMode } };



const BANNER_TRUECOLOR: &str = include_str!("../../../banner_truecolor.txt");
const BANNER_256: &str = include_str!("../../../banner_256.txt");
const BANNER_MONO: &str = include_str!("../../../banner_mono.txt");


struct LoopFrame
{
    continue_target: usize,
    break_target: usize,
    scope: usize,
    stack_depth: usize,
    iteration_depth: usize
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
            ErrorWhat::CommandNotFound(command, location) => write!(f, "Command not found: {} at {}.", command, location),
            ErrorWhat::FileGlobError(message) => write!(f, "File glob error: {}.", message),
            ErrorWhat::StackUnderflow => write!(f, "Stack underflow"),
            ErrorWhat::NoResult => write!(f, "Attempted to access the last result, but there was none."),
            ErrorWhat::ReturnOutsideFunction => write!(f, "Cannot return outside a function."),
            ErrorWhat::ExecutableNotFound(name) => write!(f, "Executable not found: {}.", name),
            ErrorWhat::ExecutableIoError(message) => write!(f, "Executable I/O error: {}.", message),
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
    location: Location,
    what: ErrorWhat
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
                                 &[String]) -> InterpreterResult<()> + 'a>;

pub type BuiltIns<'a> = HashMap<&'static str, BuiltIn<'a>>;


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


pub struct Alias
{
    pub name: String,
    pub arguments: Vec<String>
}


pub struct Interpreter
{
    variables: ScopedVariables,
    special_vars: SpecialVars,
    aliases: HashMap<String, Alias>,
    base_function_block: FunctionBlockRef,
    current_function_block: Option<FunctionBlockRef>,
    built_ins: BuiltIns<'static>,
    types: crate::language::data::types::TypeRegistry,
    captured_stdout: Option<Vec<u8>>,
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
    pub fn new(startup: Startup,
               interactive: Interactive,
               color_mode: TtyColorMode,
               tab_width: usize,
               rc_file: RcFile,
               script_args: Vec<String>) -> Self
    {
        let variables = ScopedVariables::new_from_environment();

        let built_ins: BuiltIns<'static> = HashMap::from([
                (
                    "cd",
                    Rc::new(Interpreter::handle_cd) as BuiltIn<'static>
                ),

                (
                    "exit",
                    Rc::new(Interpreter::handle_exit) as BuiltIn<'static>
                )
            ]);

        let special_vars: SpecialVars = HashMap::from([
                (
                    "$pwd",
                    Rc::new(|_interpreter: &Interpreter|
                        {
                            let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
                            Ok(Value::from_string(cwd.display().to_string()))
                        }) as ReadFunction
                ),
                (
                    "$HOSTNAME",
                    Rc::new(|_interpreter: &Interpreter|
                        {
                            let name = hostname::get()
                                .map(|name| name.to_string_lossy().into_owned())
                                .unwrap_or_else(|_| "unknown".to_string());

                            Ok(Value::from_string(name))
                        }) as ReadFunction
                )
            ]);

        let mut new_self = Self
            {
                variables,
                special_vars,
                aliases: HashMap::new(),
                base_function_block: Rc::new(RefCell::new(FunctionBlock
                    {
                        parent: None,
                        functions: HashMap::new()
                    })),
                current_function_block: None,
                built_ins,
                types: crate::language::data::types::TypeRegistry::new(),
                captured_stdout: None,
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
     * Load login profiles before interactive initialization and banner display.
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

        let is_interactive = matches!(interactive, Interactive::Yes | Interactive::YesWithoutBanner);

        self.set_variable("$build_date", Value::from_string(env!("SHELLY_BUILD_DATE").to_string()));
        self.set_variable("$build_time", Value::from_string(env!("SHELLY_BUILD_TIME").to_string()));
        self.set_variable("$version", Value::from_string(env!("CARGO_PKG_VERSION").to_string()));
        self.set_variable("$shelly", std::env::current_exe()
            .map(|path| Value::from_executable_string(path.display().to_string()))
            .unwrap_or_else(|_| Value::from_string(".".to_string())));
        self.set_variable("$OS",  Value::from_string(OS.to_string()));

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

        if matches!(startup, Startup::Login)
        {
            self.set_variable("$login", Value::Boolean(true));
            self.load_startup_script(Path::new("/etc/shelly/profile.shy"), tab_width);

            if let Some(home) = std::env::home_dir()
            {
                self.load_startup_script(&home.join(".shelly_profile.shy"), tab_width);
            }
        }
        else
        {
            self.set_variable("$login", Value::Boolean(false));
        }

        if    is_interactive
           && rc_file != RcFile::None
        {
            let file = if let RcFile::Custom(file_path)= rc_file
                {
                    Some(file_path)
                }
                else
                {
                    if let Some(home) = std::env::home_dir()
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
            Err(error) if error.kind() == io::ErrorKind::NotFound => return,
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

        let captured = std::mem::replace(&mut self.captured_stdout,
                                         previous).expect("Capture buffer must be installed.");

        (result, captured)
    }

    pub fn has_command(&self, command: &str) -> bool
    {
        if self.base_function_block.borrow().functions.contains_key(command)
        {
            return true;
        }

        self.built_ins.contains_key(command)
    }

    pub fn execute_command(&mut self,
                           location: Location,
                           command: &str,
                           args: Vec<String>) -> InterpreterResult<()>
    {
        let function = self.base_function_block.borrow().functions.get(command).cloned();

        if let Some(function) = function
        {
            self.execute_function(&location, command, &function,
                &args.iter().cloned().map(Value::from_string).collect::<Vec<_>>())?;
        }
        else if let Some(built_in) = self.built_ins.get(command).cloned()
        {
            built_in(self, &location, &args)?;
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
        let mut tokenizer = Tokenizer::new(buffer);
        let mut statements = parse_text(&mut tokenizer)?;
        let instructions = compile_ast(&mut self.types,
                                       &self.base_function_block,
                                       &mut statements,
                                       CompileTarget::Toplevel)?;

        self.execute_instructions(&instructions)
    }

    pub fn execute_instructions(&mut self, instructions: &Vec<Instruction>) -> InterpreterResult<()>
    {
        let initial_scope = self.variables.current_scope();
        let result = self.execute_instructions_scoped(instructions, initial_scope);

        self.variables.reset_to_scope(initial_scope);
        if result.is_err() { self.last_result = None; }

        result
    }

    fn execute_instructions_scoped(&mut self,
                                   instructions: &Vec<Instruction>,
                                   initial_scope: usize) -> InterpreterResult<()>
    {
        let mut stack: VecDeque<Value> = VecDeque::new();
        let mut iterations: Vec<super::iteration::Iteration> = Vec::new();
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
                Code::Push =>
                    {
                        if let Some(operand) = &instruction.operand
                        {
                            Self::push(&mut stack, operand.clone());
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
                        if self.current_function_block.is_none()
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
                        let executable = Self::pop_as_text(&location, &mut stack)?;

                        if self.can_execute(&executable)
                        {
                            self.execute(&location, executable, Vec::new())?;
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
                        if matches!(value, Value::Enum(_))
                        {
                            return Err(InterpreterError { location: location.clone(),
                                what: ErrorWhat::InvalidOperand("Cannot execute an enum as a command".to_string()) });
                        }
                        self.last_result = Some(Value::from_executable_string(value.as_text()));
                    },

                Code::ExecuteIfExecutable =>
                    {
                        match self.last_result.take()
                        {
                            Some(Value::String(executable, Executable::Yes)) =>
                                {
                                    let executable = self.eval_path_from(&executable);
                                    self.execute(&location, executable, Vec::new())?;
                                },

                            Some(value @ (Value::Array(_) | Value::ArgumentExpansion(_) | Value::HashMap(_) | Value::Range(_) | Value::Enum(_)))
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

                        let mut executable = Self::command_name(&location, Self::pop(&location, &mut stack)?)?;

                        // If the executable name starts with a $ eval as a variable first.
                        if executable.starts_with('$')
                        {
                            if executable.contains('/')
                            {
                                executable = self.interpolate_string(&location, &executable)?;
                            }
                            else
                            {
                                executable = Self::command_name(&location,
                                    self.read_raw_variable(&executable, &location)?)?;
                            }
                        }

                        self.execute(&location, executable, args)?;
                    }

                Code::NewAlias =>
                    {
                        let definition = match &instruction.operand
                            {
                                Some(Value::Array(values)) => match values.as_slice()
                                    {
                                        [Value::String(alias, _), Value::Integer(count)]
                                            if !alias.is_empty() && *count >= 0 => Some((alias, *count)),
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
                                        "NewAlias requires a nonempty alias name and a nonnegative argument count."
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

                        self.aliases.insert(alias.clone(), Alias { name: target, arguments });
                    },

                Code::NewVariable | Code::BindIteration =>
                    {
                        let variable_name = match &instruction.operand
                            {
                                Some(Value::String(name, _)) => name.clone(),
                                _ => return Err(InterpreterError
                                    {
                                        location: location.clone(),
                                        what: ErrorWhat::InvalidOperand(
                                            "Missing or invalid operand for NewVariable instruction.".to_string())
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
                        if let Err(error) = self.variables.create(variable_name,
                            ScopedValue
                                {
                                    value,
                                    exported: ValueVisibility::Private
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

                Code::SetVariable =>
                    {
                        let variable_name = match &instruction.operand
                            {
                                Some(Value::String(name, _)) => name.clone(),
                                _ => return Err(InterpreterError
                                    {
                                        location: location.clone(),
                                        what: ErrorWhat::InvalidOperand(
                                            "Missing or invalid operand for SetVariable instruction.".to_string())
                                    })
                            };

                        let mut value = Self::pop(&location, &mut stack)?;

                        match value
                        {
                            Value::ArgumentExpansion(array) => { value = Value::Array(array); }
                            _ => {}
                        }

                        if variable_name == "$HOME" && let Value::String(path, _) = &mut value
                        {
                            *path = self.eval_path_from(path);
                        }

                        if let Some(variable) = self.variables.get_mut(&variable_name)
                        {
                            variable.value = value;
                        }
                        else
                        {
                            return Err(InterpreterError
                                {
                                    location: location.clone(),
                                    what: ErrorWhat::InvalidOperand(
                                        "Variable not found for SetVariable instruction.".to_string())
                                });
                        }
                    },

                Code::StartIteration =>
                    {
                        let Some(Value::Integer(bindings @ (1 | 2))) = instruction.operand else
                        {
                            return Err(InterpreterError
                                {
                                    location: location.clone(),
                                    what: ErrorWhat::InvalidOperand("Expected one or two loop bindings".to_string())
                                });
                        };
                        let value = Self::pop(&location, &mut stack)?;
                        let iteration = super::iteration::Iteration::new(value, bindings)
                            .map_err(|message| InterpreterError
                                {
                                    location: location.clone(),
                                    what: ErrorWhat::IterationError(message.to_string())
                                })?;
                        iterations.push(iteration);
                    },

                Code::NextIteration =>
                    {
                        let iteration = iterations.last_mut().ok_or_else(|| InterpreterError
                            {
                                location: location.clone(),
                                what: ErrorWhat::InvalidOperand("No active iteration".to_string())
                            })?;
                        if let Some((first, second)) = iteration.next()
                        {
                            Self::push(&mut stack, first);
                            if let Some(second) = second { Self::push(&mut stack, second); }
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
                                        what: ErrorWhat::InvalidOperand("Invalid MakeRange flags.".to_string())
                                    })
                            };
                        let end = if flags & 2 != 0 { Some(Self::range_bound(&location, Self::pop(&location, &mut stack)?)?) } else { None };
                        let start = if flags & 1 != 0 { Some(Self::range_bound(&location, Self::pop(&location, &mut stack)?)?) } else { None };
                        Self::push(&mut stack, Value::Range(Range { start, end, inclusive: flags & 4 != 0 }));
                    },

                Code::MakeHashMap =>
                    {
                        let count = match instruction.operand
                            {
                                Some(Value::Integer(count)) if count >= 0 => count as usize,
                                _ => return Err(InterpreterError
                                    {
                                        location: location.clone(),
                                        what: ErrorWhat::InvalidOperand("Invalid MakeHashMap count.".to_string())
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
                        Self::push(&mut stack, Value::from_hash_map(pairs.into_iter().rev().collect()));
                    },

                Code::MakeArray =>
                    {
                        let count = match instruction.operand
                            {
                                Some(Value::Integer(count)) if count >= 0 => count as usize,
                                _ => return Err(InterpreterError
                                    {
                                        location: location.clone(),
                                        what: ErrorWhat::InvalidOperand("Invalid MakeArray count.".to_string())
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
                                Value::ArgumentExpansion(values) => array.extend(Rc::unwrap_or_clone(values)),
                                value => array.push(value)
                            }
                        }
                        Self::push(&mut stack, Value::from_array(array));
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
                        let (name, count) = match &instruction.operand
                            {
                                Some(Value::Array(parts)) => match parts.as_slice()
                                    {
                                        [Value::String(name, _), Value::Integer(count)] if *count > 0 =>
                                            (name, *count as usize),
                                        _ => return Err(InterpreterError
                                            {
                                                location: location.clone(),
                                                what: ErrorWhat::InvalidOperand("Invalid SetElement operand.".to_string())
                                            })
                                    },
                                _ => return Err(InterpreterError
                                    {
                                        location: location.clone(),
                                        what: ErrorWhat::InvalidOperand("Missing SetElement operand.".to_string())
                                    })
                            };
                        let value = Self::pop(&location, &mut stack)?;
                        let mut indexes = Vec::new();
                        for _ in 0..count { indexes.push(Self::pop(&location, &mut stack)?); }
                        let variable = self.variables.get_mut(name).ok_or_else(|| InterpreterError
                            {
                                location: location.clone(),
                                what: ErrorWhat::InvalidOperand(format!("Variable {} not found", name))
                            })?;
                        let value = match value
                            {
                                Value::ArgumentExpansion(values) => Value::Array(values),
                                value => value
                            };
                        indexes.reverse();
                        Self::set_element(&location, &mut variable.value, &indexes, value)?;
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
                                            "Missing or invalid operand for GetVariable instruction.".to_string())
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
                                            "Missing or invalid operand for ExportVariable instruction.".to_string())
                                    })
                            };

                        if let Some(value) = self.variables.get_mut(&variable_name)
                        {
                            value.exported = ValueVisibility::Exported;
                        }
                        else
                        {
                            return Err(InterpreterError
                                {
                                    location: location.clone(),
                                    what: ErrorWhat::InvalidOperand(
                                        "Variable not found for ExportVariable instruction.".to_string())
                                });
                        }
                    },

                Code::GlobFiles =>
                    {
                        let pattern = Self::pop_as_text(&location, &mut stack)?;
                        let expand_tilde = matches!(instruction.operand, Some(Value::Boolean(true)));
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
                                    let length = range.len().ok_or_else(|| error("Cannot expand a range with omitted bounds"))?;
                                    let length = usize::try_from(length).map_err(|_| error("Range is too large to expand"))?;
                                    let mut values = Vec::new();
                                    values.try_reserve_exact(length).map_err(|_| error("Range is too large to expand"))?;
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
                        self.variables.push_scope();
                    },

                Code::ExitScope =>
                    {
                        if self.variables.current_scope() == initial_scope
                        {
                            return Err(InterpreterError
                                {
                                    location: location.clone(),
                                    what: ErrorWhat::InitialScopePopAttempt
                                });
                        }

                        self.variables.pop_scope();
                    },

                Code::EnterLoop =>
                    {
                        let Some(Value::Array(targets)) = &instruction.operand else
                        {
                            return Err(InterpreterError
                                {
                                    location: location.clone(),
                                    what: ErrorWhat::InvalidOperand("Expected two linked loop targets".to_string())
                                });
                        };
                        if targets.len() != 2
                        {
                            return Err(InterpreterError
                                {
                                    location: location.clone(),
                                    what: ErrorWhat::InvalidOperand("Expected two linked loop targets".to_string())
                                });
                        }
                        loops.push(LoopFrame
                            {
                                continue_target: Self::jump_target(instructions, targets.first(), &location)?,
                                break_target: Self::jump_target(instructions, targets.get(1), &location)?,
                                scope: self.variables.current_scope(),
                                stack_depth: stack.len(),
                                iteration_depth: iterations.len()
                            });
                    },

                Code::ExitLoop =>
                    {
                        loops.pop().ok_or_else(|| InterpreterError
                            {
                                location: location.clone(),
                                what: ErrorWhat::InvalidOperand("No active loop to exit".to_string())
                            })?;
                    },

                Code::Break | Code::Continue =>
                    {
                        let frame = loops.last().ok_or_else(|| InterpreterError
                            {
                                location: location.clone(),
                                what: ErrorWhat::LoopControlError(format!("Cannot {} outside a loop",
                                    if matches!(instruction.code, Code::Break) { "break" } else { "continue" }))
                            })?;
                        // A transfer may abandon nested blocks and partially evaluated
                        // expressions. Restore the state saved before the iteration body.
                        self.variables.reset_to_scope(frame.scope);
                        stack.truncate(frame.stack_depth);
                        iterations.truncate(frame.iteration_depth);
                        self.last_result = None;
                        instruction_pointer = if matches!(instruction.code, Code::Break)
                            { frame.break_target } else { frame.continue_target };
                        continue;
                    },

                Code::JumpTarget => {},

                Code::Jump | Code::JumpIfFalse | Code::JumpIfTrue =>
                    {
                        let target = Self::jump_target(instructions, instruction.operand.as_ref(), &location)?;
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
                                            what: ErrorWhat::InvalidOperand("Expected a boolean jump condition".to_string())
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

                Code::ToBoolean | Code::BooleanNot =>
                    {
                        let value = self.last_result.take().ok_or_else(|| InterpreterError
                            {
                                location: location.clone(),
                                what: ErrorWhat::NoResult
                            })?;
                        let value = value.as_bool();
                        self.last_result = Some(Value::Boolean(
                            if matches!(instruction.code, Code::BooleanNot) { !value } else { value }));
                    },

                Code::CompareEqual | Code::CompareNotEqual =>
                    {
                        let rhs = Self::pop(&location, &mut stack)?;
                        let lhs = Self::pop(&location, &mut stack)?;
                        let equal = lhs.equals(&rhs);
                        Self::push(&mut stack, Value::Boolean(
                            if matches!(instruction.code, Code::CompareEqual) { equal } else { !equal }));
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
                        if lhs.rejects_integer_conversion() || rhs.rejects_integer_conversion()
                        {
                            return Err(error("Enums cannot be used in arithmetic"));
                        }
                        let rhs = rhs.checked_integer().ok_or_else(|| error("Integer overflow"))?;
                        let lhs = lhs.checked_integer().ok_or_else(|| error("Integer overflow"))?;
                        if rhs == 0 && matches!(instruction.code, Code::MathDivide | Code::MathModulo)
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
        let _ = self.variables.create(name.to_string(), ScopedValue
            {
                value,
                exported: ValueVisibility::Private
            });
    }

    pub fn variable_names(&self) -> Vec<String>
    {
        let mut names: Vec<String> = self.variables.names()
            .chain(self.special_vars.keys().copied()).map(str::to_string).collect();
        names.sort();
        names.dedup();
        names
    }

    pub fn evaluate_variable(&self, name: &str) -> InterpreterResult<String>
    {
        if self.variables.get(name).is_none() && !self.special_vars.contains_key(name)
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
        if self.variables.get(name).is_none() && !self.special_vars.contains_key(name)
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

    fn read_raw_variable(&self, name: &str, location: &Location) -> InterpreterResult<Value>
    {
        if let Some(value) = self.variables.get(&name).cloned()
        {
            Ok(value.value)
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

    fn get_function<'a>(&self,
                        executable: &str,
                        function_block: &'a FunctionBlockRef) -> Option<FunctionRef>
    {
        if let Some(function) = function_block.borrow().functions.get(executable)
        {
            return Some(function.clone());
        }

        if let Some(parent_function_block) = &function_block.borrow().parent
        {
            return self.get_function(executable, parent_function_block);
        }

        None
    }

    fn can_execute(&self, executable: &str) -> bool
    {
        if    self.built_ins.contains_key(executable)
           || self.aliases.contains_key(executable)
           || self.get_function(executable, &self.base_function_block).is_some()
           || self.current_function_block.as_ref()
                .is_some_and(|block| self.get_function(executable, block).is_some())
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
        let path = self.variables.get("$PATH")
            .filter(|value| value.exported == ValueVisibility::Exported)
            .map(|value| self.eval_path_list_from(&value.value.as_text()))
            .unwrap_or_else(|| "/bin:/usr/bin".to_string());

        std::env::split_paths(&path).any(|directory| is_executable(&directory.join(executable)))
    }

    fn execute_function(&mut self,
                        location: &Location,
                        name: &str,
                        function: &FunctionRef,
                        args: &[Value]) -> InterpreterResult<()>
    {
        if function.arguments.len() != args.len()
        {
            let message = format!("Function {} expected {} arguments, but got {}.",
                                  name,
                                  function.arguments.len(),
                                  args.len());

            return Err(InterpreterError
                {
                    location: location.clone(),
                    what: ErrorWhat::ArgumentMismatch(message)
                });
        }

        self.variables.push_scope();

        for index in 0..function.arguments.len()
        {
            let result = self.variables.create(function.arguments[index].clone(),
                ScopedValue
                {
                    value: args[index].clone(),
                    exported: ValueVisibility::Private
                });

            if let Err(error) = result
            {
                return Err(InterpreterError
                    {
                        location: location.clone(),
                        what: ErrorWhat::ArgumentMismatch(error)
                    });
            }
        }

        let caller_function_block = self.current_function_block.replace(function.functions.clone());

        let call_result = self.execute_instructions(&function.code);

        self.current_function_block = caller_function_block;

        self.variables.pop_scope();

        call_result
    }

    fn find_and_execute_function(&mut self,
                        location: &Location,
                        executable: &str,
                        args: &[Value]) -> InterpreterResult<bool>
    {
        if    let Some(current_function_block) = self.current_function_block.clone()
           && let Some(function) = self.get_function(executable, &current_function_block)
        {
            self.execute_function(location, executable, &function, args)?;
            return Ok(true);
        }

        if let Some(function) = self.get_function(executable, &self.base_function_block)
        {
            self.execute_function(location, executable, &function, args)?;
            return Ok(true);
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
        let args: Vec<Value> = resolved_args.into_iter().map(Value::from_string).chain(args).collect();

        if let Some(built_in) = self.built_ins.get(executable.as_str()).cloned()
        {
            return built_in(self, location, &args.iter().map(Value::as_text).collect::<Vec<_>>());
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

        // Only include the environment variables explicitly set by the interpreter.
        let env_vars: Vec<(String, String)> = self.variables.get_all_flattened()
            .iter()
            .filter(|(_, value)| value.exported == ValueVisibility::Exported)
            .map(|(key, value)|
                {
                    let text = value.value.as_text();
                    let text = if key == "$PATH"
                        {
                            self.eval_path_list_from(&text)
                        }
                        else
                        {
                            self.eval_path_from(&text)
                        };

                    (key.strip_prefix('$').unwrap_or(key.as_str()).to_string(), text)
                })
            .collect();


        let mut command = Command::new(&executable);

        command.args(args.iter().map(|argument| self.eval_path_from(&argument.as_text())))
            .env_clear().envs(env_vars);

        let status_result = if self.captured_stdout.is_some()
            {
                command
                    .stdin(Stdio::inherit())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::inherit())
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
                        std::io::ErrorKind::NotFound =>
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

    fn command_name(location: &Location, value: Value) -> InterpreterResult<String>
    {
        match value
        {
            Value::Enum(_) => Err(InterpreterError
                { location: location.clone(),
                  what: ErrorWhat::InvalidOperand("Cannot execute an enum as a command".to_string()) }),
            Value::Range(_) => Err(InterpreterError
                {
                    location: location.clone(),
                    what: ErrorWhat::RangeError("Cannot execute a range as a command".to_string())
                }),
            Value::HashMap(_) => Err(InterpreterError
                {
                    location: location.clone(),
                    what: ErrorWhat::HashMapError("Cannot execute a hash map as a command".to_string())
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
            .map_err(|_| error(format!("Array index {} is out of bounds for length {}", index, length)))?;
        if index >= length
        {
            return Err(error(format!("Array index {} is out of bounds for length {}", index, length)));
        }
        Ok(index)
    }

    fn get_element(location: &Location, collection: &Value, index: &Value) -> InterpreterResult<Value>
    {
        match collection
        {
            Value::HashMap(values) => Ok(values.get(&MapKey::from_value(index)).cloned().unwrap_or(Value::None)),
            Value::Array(values) => Ok(values[Self::array_index(location, values, index)?].clone()),
            _ => Err(InterpreterError
                {
                    location: location.clone(),
                    what: ErrorWhat::ArrayError("Cannot index a value that is not an array or hash map".to_string())
                })
        }
    }

    fn set_element(location: &Location, collection: &mut Value,
                    indexes: &[Value], value: Value) -> InterpreterResult<()>
    {
        let Some((index, rest)) = indexes.split_first() else
        {
            *collection = value;
            return Ok(());
        };
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
                                what: ErrorWhat::HashMapError("Missing intermediate key in indexed assignment".to_string())
                            });
                    }
                    let child = Rc::make_mut(values).get_mut(&key).unwrap();
                    Self::set_element(location, child, rest, value)
                },
            Value::Array(values) =>
                {
                    let index = Self::array_index(location, values, index)?;
                    Self::set_element(location, &mut Rc::make_mut(values)[index], rest, value)
                },
            _ => Err(InterpreterError
                {
                    location: location.clone(),
                    what: ErrorWhat::ArrayError("Cannot index a value that is not an array or hash map".to_string())
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

    fn handle_string_interpolation(&self,
                                   location: &Location,
                                   stack: &mut VecDeque<Value>,
                                   escaped_dollars: &[usize], escape_glob: bool) -> InterpreterResult<()>
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

        let interpolated = self.interpolate_string_at(location, &text, escaped_dollars, !escape_glob, escape_glob)?;
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
            interpolated.push_str(&if escape_glob { glob::Pattern::escape(&text) } else { text });
        }

        Ok(interpolated)
    }

    /**
     * Resolve the current shell's home directory without interpolating its contents.
     */
    fn home_path(&self) -> Option<String>
    {
        self.variables.get("$HOME")
            .map(|home| home.value.as_text())
            .filter(|home| !home.is_empty())
            .or_else(|| std::env::home_dir().map(|home| home.to_string_lossy().into_owned()))
            .filter(|home| Path::new(home).is_absolute())
    }

    /**
     * Shorten a leading home directory for display, matching complete path segments.
     */
    fn eval_path_to(&self, path: &str) -> String
    {
        let Some(home) = self.home_path() else { return path.to_string(); };
        let home = home.trim_end_matches(std::path::is_separator);

        // A root home must not become an empty prefix that matches relative paths.
        if home.is_empty()
        {
            return if path == std::path::MAIN_SEPARATOR_STR
                {
                    "~".to_string()
                }
                else if path.starts_with(std::path::is_separator)
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

        if let Some(suffix) = path.strip_prefix(home)
           && suffix.starts_with(std::path::is_separator)
        {
            return format!("~{}", suffix);
        }

        path.to_string()
    }

    /**
     * Expand only the current user's leading ~ or ~/ prefix. Callers decide
     * whether the source word is eligible; quoted words and variables are literal.
     */
    fn eval_path_from(&self, path: &str) -> String
    {
        let Some(suffix) = path.strip_prefix('~') else { return path.to_string(); };
        if !suffix.is_empty() && !suffix.starts_with(std::path::is_separator)
        {
            return path.to_string();
        }

        let Some(home) = self.home_path() else { return path.to_string(); };
        if suffix.is_empty()
        {
            return home;
        }

        format!("{}{}", home.trim_end_matches(std::path::is_separator), suffix)
    }

    /**
     * PATH entries must be real paths when searching for or launching programs.
     */
    fn eval_path_list_from(&self, paths: &str) -> String
    {
        let expanded = std::env::split_paths(paths)
            .map(|path| PathBuf::from(self.eval_path_from(&path.to_string_lossy())));

        std::env::join_paths(expanded)
            .map(|paths| paths.to_string_lossy().into_owned())
            .unwrap_or_else(|_| paths.to_string())
    }

    fn handle_file_glob(&self, pattern: &str, expand_tilde: bool) -> InterpreterResult<Value>
    {
        // Glob results may omit an explicit "./" prefix or normalize separators.
        // Normalize both sides for matching without resolving parent directories.
        fn normalized_path(path: &std::path::Path) -> std::path::PathBuf
        {
            path.components()
                .filter(|component| !matches!(component, std::path::Component::CurDir))
                .collect()
        }

        let display_pattern = self.eval_path_to(pattern);
        let expanded = if expand_tilde { self.eval_path_from(pattern) } else { pattern.to_string() };
        let pattern = if expanded != pattern
            {
                let suffix = pattern.strip_prefix('~').unwrap();
                let home = expanded.strip_suffix(suffix).unwrap();
                format!("{}{}", glob::Pattern::escape(home), suffix)
            }
            else
            {
                expanded
            };

        let options = glob::MatchOptions
            {
                require_literal_separator: true,
                require_literal_leading_dot: true,
                ..glob::MatchOptions::new()
            };

        let invalid_pattern = |error| InterpreterError
            {
                location: Location::default(),
                what: ErrorWhat::InvalidOperand(
                    format!("Invalid glob pattern '{}': {}", display_pattern, error))
            };

        let matcher = glob::Pattern::new(
            &normalized_path(std::path::Path::new(&pattern)).to_string_lossy())
            .map_err(&invalid_pattern)?;

        // glob_with's leading-dot option prunes even explicitly requested hidden
        // entries. Enumerate normally, then enforce that rule with Pattern instead.
        let paths = glob::glob(&pattern).map_err(invalid_pattern)?;

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
            let entry_name = path_text.trim_end_matches(std::path::is_separator)
                .rsplit(std::path::is_separator).next();

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

        while let Some(alias) = self.aliases.get(name)
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

    fn handle_cd(&mut self, _location: &Location, args: &[String]) -> InterpreterResult<()>
    {
        if args.len() != 1
        {
            eprintln!("Usage: cd <directory>");
            self.last_result = Some(Value::ExecResult(ExecResult::Value(1)));
            return Ok(());
        }

        if let Err(error) = std::env::set_current_dir(self.eval_path_from(&args[0]))
        {
            eprintln!("Failed to change directory to {}: {}", self.eval_path_to(&args[0]), error);
            self.last_result = Some(Value::ExecResult(ExecResult::Value(1)));
            return Ok(());
        }

        self.last_result = Some(Value::ExecResult(ExecResult::Value(0)));
        Ok(())
    }

    fn handle_exit(&mut self, location: &Location, args: &[String]) -> InterpreterResult<()>
    {
        self.exit_code = match args
            {
                [] => 0,
                [code] => code.parse::<u8>().map_err(|_| InterpreterError
                    {
                        location: location.clone(),
                        what: ErrorWhat::ArgumentMismatch("exit expects a status from 0 to 255.".to_string())
                    })?,
                _ => return Err(InterpreterError
                    {
                        location: location.clone(),
                        what: ErrorWhat::ArgumentMismatch("exit expects at most one argument.".to_string())
                    })
            };
        self.halted = true;
        self.last_result = Some(Value::None);

        Ok(())
    }
}
