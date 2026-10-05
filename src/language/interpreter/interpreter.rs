
use std::{ collections::{ HashMap, VecDeque },
           fmt::{ self, Display, Formatter },
           process::Command,
           rc::Rc };

use crate::language::{ bytecode::{ Code, Instruction },
                       compiler::{ CompileError, compile_ast },
                       data::value::Value,
                       parser::{ ParserError, parse_text },
                       tokenizer::Tokenizer,
                       text::{ buffer::SimpleBuffer,location::Location } };



pub enum ErrorWhat
{
    ParserError(ParserError),
    CompileError(CompileError),
    InvalidOperand(String),
    FileGlobError(String),
    StackUnderflow,
    ExecutableNotFound(String),
    ExecutableIoError(String),
    ExecutableBadReturn(u8)
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
            ErrorWhat::FileGlobError(message) => write!(f, "File glob error: {}", message),
            ErrorWhat::StackUnderflow => write!(f, "Stack underflow"),
            ErrorWhat::ExecutableNotFound(name) => write!(f, "Executable not found: {}", name),
            ErrorWhat::ExecutableIoError(message) => write!(f, "Executable I/O error: {}", message),
            ErrorWhat::ExecutableBadReturn(code) =>
                {
                    write!(f, "Executable returned error code: {}", code)
                }
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


pub type BuiltIn<'a> = Rc<dyn Fn(&mut Interpreter,
                                 &Location,
                                 &[String]) -> InterpreterResult<()> + 'a>;

pub type BuiltIns<'a> = HashMap<&'static str, BuiltIn<'a>>;


pub type InterpreterResult<T> = Result<T, InterpreterError>;


pub struct Interpreter
{
    variables: HashMap<String, Value>,
    built_ins: BuiltIns<'static>,
    pub halted: bool
}


impl Interpreter
{
    pub fn new() -> Self
    {
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

        Self
            {
                variables: HashMap::new(),
                built_ins,
                halted: false
            }
    }

    pub fn execute_code(&mut self, source: &str, code: &str) -> InterpreterResult<()>
    {
        let mut buffer = SimpleBuffer::new(source, code, None);
        let mut tokenizer = Tokenizer::new(&mut buffer);
        let statements = parse_text(&mut tokenizer)?;
        let instructions = compile_ast(&statements)?;

        self.execute_instructions(&instructions)
    }

    pub fn execute_instructions(&mut self, instructions: &Vec<Instruction>) -> InterpreterResult<()>
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
                            if let Some(value) = self.variables.get(&executable)
                            {
                                executable = value.as_text();
                            }
                            else
                            {
                                return Err(InterpreterError
                                    {
                                        location: location.clone(),
                                        what: ErrorWhat::InvalidOperand(
                                            "Variable not found for Execute instruction.".to_string())
                                    });
                            }
                        }

                        self.execute(&location, executable, args)?;
                    }

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

                        self.variables.insert(variable_name, Value::Integer(0));
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

                        if !self.variables.contains_key(&variable_name)
                        {
                            return Err(InterpreterError
                                {
                                    location: location.clone(),
                                    what: ErrorWhat::InvalidOperand(
                                        "Variable not found for SetVariable instruction.".to_string())
                                });
                        }

                        self.variables.insert(variable_name, value);
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

                        if let Some(value) = self.variables.get(&variable_name).cloned()
                        {
                            Self::push(&mut stack, value);
                        }
                        else
                        {
                            return Err(InterpreterError
                                {
                                    location: location.clone(),
                                    what: ErrorWhat::InvalidOperand(
                                        "Variable not found for GetVariable instruction.".to_string())
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

    fn execute(&mut self,
               location: &Location,
               executable: String,
               args: Vec<String>) -> InterpreterResult<()>
    {
        if let Some(built_in) = self.built_ins.get(executable.as_str()).cloned()

        {
            return built_in(self, location, &args);
        }

        let status = Command::new(&executable)
            .args(args)
            .status()
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

            let value = self.variables.get(&variable_name).ok_or_else(||
                invalid_operand(format!("Variable '{}' not found for string interpolation.",
                                        variable_name)))?;

            // Append values directly so their contents are not interpolated again.
            interpolated.push_str(&value.as_text());
        }

        Self::push(stack, Value::String(interpolated));
        Ok(())
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
