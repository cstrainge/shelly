
use std::{ cell::RefCell,
           collections::{ HashMap, HashSet, VecDeque },
           fmt::{ self, Debug, Display, Formatter },
           io::{ self, Write },
           process::{ Command, Stdio },
           rc::Rc };

use crate::language::{ bytecode::{ Code,
                                   Instruction,
                                   FunctionBlockRef,
                                   FunctionRef,
                                   FunctionBlock },
                       compiler::{ CompileError, compile_ast },
                       data::{ value::Value,
                               scoped_variables::{ ScopedValue,
                                                   ScopedVariables,
                                                   ValueVisibility } },
                       parser::{ ParserError, parse_text },
                       tokenizer::Tokenizer,
                       text::{ buffer::SimpleBuffer,location::Location } };



pub enum ErrorWhat
{
    ParserError(ParserError),
    CompileError(CompileError),
    InvalidOperand(String),
    CommandNotFound(String, Location),
    ArgumentMismatch(String),
    FileGlobError(String),
    StackUnderflow,
    ExecutableNotFound(String),
    ExecutableIoError(String),
    ExecutableBadReturn(u8),
    InitialScopePopAttempt
}


impl Display for ErrorWhat
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
    {
        match self
        {
            ErrorWhat::ParserError(error) => write!(f, "Parser error: {}", error),
            ErrorWhat::CompileError(error) => write!(f, "Compile error: {}", error),
            ErrorWhat::InvalidOperand(message) => write!(f, "Invalid operand: {}", message),
            ErrorWhat::CommandNotFound(command, location) => write!(f, "Command not found: {} at {}", command, location),
            ErrorWhat::FileGlobError(message) => write!(f, "File glob error: {}", message),
            ErrorWhat::StackUnderflow => write!(f, "Stack underflow"),
            ErrorWhat::ExecutableNotFound(name) => write!(f, "Executable not found: {}", name),
            ErrorWhat::ExecutableIoError(message) => write!(f, "Executable I/O error: {}", message),
            ErrorWhat::ExecutableBadReturn(code) =>
                {
                    write!(f, "Executable returned error code: {}", code)
                },

            ErrorWhat::ArgumentMismatch(message) => write!(f, "{}", message),
            ErrorWhat::InitialScopePopAttempt => write!(f, "Attempted to pop the initial scope")
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
    captured_stdout: Option<Vec<u8>>,
    pub halted: bool
}


impl Interpreter
{
    pub fn new() -> Self
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
                            Ok(Value::String(cwd.display().to_string()))
                        }) as ReadFunction
                ),
                (
                    "$HOSTNAME",
                    Rc::new(|_interpreter: &Interpreter|
                        {
                            let name = hostname::get()
                                .map(|name| name.to_string_lossy().into_owned())
                                .unwrap_or_else(|_| "unknown".to_string());

                            Ok(Value::String(name))
                        }) as ReadFunction
                )
            ]);

        Self
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
                captured_stdout: None,
                halted: false
            }
    }

    pub fn write_stdout(&mut self, bytes: &[u8]) -> io::Result<()>
    {
        if let Some(buffer) = &mut self.captured_stdout
        {
            buffer.extend_from_slice(bytes);
            Ok(())
        }
        else
        {
            let mut stdout = io::stdout().lock();
            stdout.write_all(bytes)?;
            stdout.flush()
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
            self.execute_function(&location, command, &function, &args)?;
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

    pub fn execute_code(&mut self, source: &str, code: &str) -> InterpreterResult<()>
    {
        let mut buffer = SimpleBuffer::new(source, code, None);
        let mut tokenizer = Tokenizer::new(&mut buffer);
        let statements = parse_text(&mut tokenizer)?;
        let instructions = compile_ast(&self.base_function_block, &statements)?;

        self.execute_instructions(&instructions)
    }

    pub fn execute_instructions(&mut self, instructions: &Vec<Instruction>) -> InterpreterResult<()>
    {
        let initial_scope = self.variables.current_scope();
        let result = self.execute_instructions_scoped(instructions, initial_scope);

        self.variables.reset_to_scope(initial_scope);

        result
    }

    fn execute_instructions_scoped(&mut self,
                                   instructions: &Vec<Instruction>,
                                   initial_scope: usize) -> InterpreterResult<()>
    {
        let mut stack: VecDeque<Value> = VecDeque::new();
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
                                    for expanded_arg in expanded_args.into_iter().rev()
                                    {
                                        args.push(expanded_arg.as_text());
                                    }
                                }
                                else
                                {
                                    args.push(arg.as_text());
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

                        let mut executable = Self::pop_as_text(&location, &mut stack)?;

                        // If the executable name starts with a $ eval as a variable first.
                        if executable.starts_with('$')
                        {
                            if executable.contains('/')
                            {
                                executable = self.interpolate_string(&location, &executable)?;
                            }
                            else
                            {
                                let value = self.read_variable(&executable, &location)?.as_text();
                                executable = value;
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
                                        [Value::String(alias), Value::String(target), arguments @ ..]
                                            if !alias.is_empty() && !target.is_empty() =>
                                                Some((alias, target, arguments)),
                                        _ => None
                                    },
                                _ => None
                            };

                        let Some((alias, target, arguments)) = definition else
                        {
                            return Err(InterpreterError
                                {
                                    location: location.clone(),
                                    what: ErrorWhat::InvalidOperand(
                                        "NewAlias requires an array containing a nonempty alias name, target command, and optional arguments."
                                            .to_string())
                                });
                        };

                        self.aliases.insert(alias.clone(), Alias
                            {
                                name: target.clone(),
                                arguments: arguments.iter().map(Value::as_text).collect()
                            });
                    },

                Code::NewVariable =>
                    {
                        let variable_name = match &instruction.operand
                            {
                                Some(Value::String(name)) => name.clone(),
                                _ => return Err(InterpreterError
                                    {
                                        location: location.clone(),
                                        what: ErrorWhat::InvalidOperand(
                                            "Missing or invalid operand for NewVariable instruction.".to_string())
                                    })
                            };

                        if let Err(error) = self.variables.create(variable_name,
                            ScopedValue
                                {
                                    value: Value::None,
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
                                Some(Value::String(name)) => name.clone(),
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

                Code::GetVariable =>
                    {
                        let variable_name = match &instruction.operand
                            {
                                Some(Value::String(name)) => name.clone(),
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

                Code::ExportVariable =>
                    {
                        let variable_name = match &instruction.operand
                            {
                                Some(Value::String(name)) => name.clone(),
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
                        stack.push_back(self.handle_file_glob(&instruction.operand)
                            .map_err(|mut error|
                            {
                                error.location = location.clone();
                                error
                            })?);
                    },

                Code::ExpandArray =>
                    {
                        let mut value = Self::pop(&location, &mut stack)?;

                        match value
                        {
                            Value::Array(array) => value = Value::ArgumentExpansion(array),

                            Value::ArgumentExpansion(_) => {},

                            _ =>
                                {
                                    value = Value::ArgumentExpansion(vec![value]);
                                }
                        }

                        Self::push(&mut stack, value);
                    },

                Code::InterpolateString =>
                    {
                        self.handle_string_interpolation(&location, &mut stack)?;
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

                Code::MathAdd =>
                    {
                        let rhs = Self::pop(&location, &mut stack)?;
                        let lhs = Self::pop(&location, &mut stack)?;
                        Self::push(&mut stack, Value::Integer(lhs.as_integer() + rhs.as_integer()));
                    },

                Code::MathSubtract =>
                    {
                        let rhs = Self::pop(&location, &mut stack)?;
                        let lhs = Self::pop(&location, &mut stack)?;
                        Self::push(&mut stack, Value::Integer(lhs.as_integer() - rhs.as_integer()));
                    },

                Code::MathMultiply =>
                    {
                        let rhs = Self::pop(&location, &mut stack)?;
                        let lhs = Self::pop(&location, &mut stack)?;
                        Self::push(&mut stack, Value::Integer(lhs.as_integer() * rhs.as_integer()));
                    },

                Code::MathDivide =>
                    {
                        let rhs = Self::pop(&location, &mut stack)?;
                        let lhs = Self::pop(&location, &mut stack)?;
                        Self::push(&mut stack, Value::Integer(lhs.as_integer() / rhs.as_integer()));
                    },

                Code::MathModulo =>
                    {
                        let rhs = Self::pop(&location, &mut stack)?;
                        let lhs = Self::pop(&location, &mut stack)?;
                        Self::push(&mut stack, Value::Integer(lhs.as_integer() % rhs.as_integer()));
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

    pub fn evaluate_variable(&self, name: &str) -> InterpreterResult<String>
    {
        if self.variables.get(name).is_none() && !self.special_vars.contains_key(name)
        {
            return Ok(String::new());
        }

        let location = Location::new(name, 1, 1);
        let value = self.read_variable(name, &location)?;

        self.interpolate_string(&location, &value.as_text())
    }

    fn read_variable(&self, name: &str, location: &Location) -> InterpreterResult<Value>
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

    fn execute_function(&mut self,
                        location: &Location,
                        name: &str,
                        function: &FunctionRef,
                        args: &Vec<String>) -> InterpreterResult<()>
    {
        self.variables.push_scope();

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

        for index in 0..function.arguments.len()
        {
            let result = self.variables.create(function.arguments[index].clone(),
                ScopedValue
                {
                    value: Value::String(args[index].clone()),
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

        self.current_function_block = Some(function.functions.clone());

        let call_result = self.execute_instructions(&function.code);

        self.current_function_block = function.functions.borrow().parent.clone();

        self.variables.pop_scope();

        call_result
    }

    fn find_and_execute_function(&mut self,
                        location: &Location,
                        executable: &str,
                        args: &Vec<String>) -> InterpreterResult<bool>
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
               args: Vec<String>) -> InterpreterResult<()>
    {
        let (executable, mut resolved_args) = self.resolve_alias(location, &executable)?;
        resolved_args.extend(args);
        let args = resolved_args;

        if let Some(built_in) = self.built_ins.get(executable.as_str()).cloned()
        {
            return built_in(self, location, &args);
        }

        if self.find_and_execute_function(location, &executable, &args)?
        {
            return Ok(());
        }

        // Only include the environment variables explicitly set by the interpreter.
        let env_vars: Vec<(String, String)> = self.variables.get_all_flattened()
            .iter()
            .filter(|(_, value)| value.exported == ValueVisibility::Exported)
            .map(|(key, value)|
                (key.strip_prefix('$').unwrap_or(key.as_str()).to_string(),
                 value.value.as_text()))
            .collect();


        let mut command = Command::new(&executable);

        command.args(args).env_clear().envs(env_vars);

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
                            ErrorWhat::ExecutableNotFound(executable.clone()),

                        _ => ErrorWhat::ExecutableIoError(
                            format!("'{}': {}", executable, error))
                    }
                })?;

        if !status.success()
        {
            return Err(InterpreterError
                {
                    location: location.clone(),
                    what: ErrorWhat::ExecutableBadReturn(status.code().unwrap_or(1) as u8)
                });
        }

        Ok(())
    }

    fn push(stack: &mut VecDeque<Value>, value: Value)
    {
        stack.push_back(value);
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
                                   stack: &mut VecDeque<Value>) -> InterpreterResult<()>
    {
        let invalid_operand = |message| InterpreterError
            {
                location: location.clone(),
                what: ErrorWhat::InvalidOperand(message)
            };

        let Value::String(text) = Self::pop(location, stack)? else
        {
            return Err(invalid_operand(
                "Expected a string for InterpolateString instruction.".to_string()));
        };

        let interpolated = self.interpolate_string(location, &text)?;
        Self::push(stack, Value::String(interpolated));
        Ok(())
    }

    fn interpolate_string(&self, location: &Location, text: &str) -> InterpreterResult<String>
    {
        let invalid_operand = |message| InterpreterError
            {
                location: location.clone(),
                what: ErrorWhat::InvalidOperand(message)
            };

        let mut interpolated = String::with_capacity(text.len());
        let mut characters = text.chars().peekable();

        while let Some(character) = characters.next()
        {
            if character != '$'
            {
                interpolated.push(character);
                continue;
            }

            let mut variable_name = String::from("$");

            if characters.peek() == Some(&'{')
            {
                characters.next();
                let mut closed = false;

                for character in characters.by_ref()
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
                while let Some(&character) = characters.peek()
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

            let value = self.read_variable(&variable_name, &location)?;

            // Append values directly so their contents are not interpolated again.
            interpolated.push_str(&value.as_text());
        }

        Ok(interpolated)
    }

    fn handle_file_glob(&self, operand: &Option<Value>) -> InterpreterResult<Value>
    {
        // Glob results may omit an explicit "./" prefix or normalize separators.
        // Normalize both sides for matching without resolving parent directories.
        fn normalized_path(path: &std::path::Path) -> std::path::PathBuf
        {
            path.components()
                .filter(|component| !matches!(component, std::path::Component::CurDir))
                .collect()
        }

        let pattern = match operand
            {
                Some(Value::String(pattern)) => pattern,
                _ => return Err(InterpreterError
                    {
                        location: Location::default(),
                        what: ErrorWhat::InvalidOperand(
                            "Missing or invalid operand for GlobFiles instruction.".to_string())
                    })
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
                    format!("Invalid glob pattern '{}': {}", pattern, error))
            };

        let matcher = glob::Pattern::new(
            &normalized_path(std::path::Path::new(pattern)).to_string_lossy())
            .map_err(&invalid_pattern)?;

        // glob_with's leading-dot option prunes even explicitly requested hidden
        // entries. Enumerate normally, then enforce that rule with Pattern instead.
        let paths = glob::glob(pattern).map_err(invalid_pattern)?;

        let mut arguments = Vec::new();

        for entry in paths
        {
            let path = entry.map_err(|error| InterpreterError
                {
                    location: Location::default(),
                    what: ErrorWhat::FileGlobError(
                        format!("Failed to expand '{}': {}", pattern, error))
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

            arguments.push(Value::String(path_text.into_owned()));
        }

        if arguments.is_empty()
        {
            return Err(InterpreterError
                {
                    location: Location::default(),
                    what: ErrorWhat::FileGlobError(
                        format!("No paths matched glob pattern '{}'.", pattern))
                });
        }

        Ok(Value::ArgumentExpansion(arguments))
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

    fn handle_cd(&mut self, location: &Location, args: &[String]) -> InterpreterResult<()>
    {
        if args.len() != 1
        {
            println!("Usage: cd <directory>");
            return Err(InterpreterError
                {
                    location: location.clone(),
                    what: ErrorWhat::ExecutableBadReturn(1)
                });
        }

        if let Err(error) = std::env::set_current_dir(&args[0])
        {
            println!("Failed to change directory: {}", error);
            return Err(InterpreterError
                {
                    location: location.clone(),
                    what: ErrorWhat::ExecutableBadReturn(1)
                });
        }

        Ok(())
    }

    fn handle_exit(&mut self, _location: &Location, _args: &[String]) -> InterpreterResult<()>
    {
        self.halted = true;

        Ok(())
    }
}
