
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
    //variables: HashMap<String, Value>,
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
                //variables: HashMap::new(),
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
                            let value = operand.as_int();

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
                                args.push(Self::pop_as_text(&location, &mut stack)?);
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

                        let executable = Self::pop_as_text(&location, &mut stack)?;

                        self.execute(&location, executable, args)?;
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

    fn handle_exit(&mut self, location: &Location, args: &[String]) -> InterpreterResult<()>
    {
        self.halted = true;
        Ok(())
    }
}
