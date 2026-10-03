
use std::{ collections::{ HashMap, VecDeque },
           fmt::{ self, Display, Formatter },
           process::Command };

use super::{ bytecode::{ Instruction, Code },
             compiler::CompileError,
             data::value::Value,
             parser::ParserError,
             text::location::Location };



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


impl Display for InterpreterError
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
    {
        write!(f, "Interpreter error at {}: {}", self.location, self.what)
    }
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


fn push(stack: &mut VecDeque<Value>, value: Value)
{
    stack.push_back(value);
}

fn pop(location: &Location, stack: &mut VecDeque<Value>) -> Result<Value, InterpreterError>
{
    stack.pop_back().ok_or(InterpreterError
        {
            location: location.clone(),
            what: ErrorWhat::StackUnderflow
        })
}


fn pop_as_text(location: &Location,
               stack: &mut VecDeque<Value>) -> Result<String, InterpreterError>
{
    Ok(pop(location, stack)?.as_text())
}


fn execute(location: &Location,
           built_ins: &BuiltIns,
           executable: String,
           args: Vec<String>) -> Result<(), InterpreterError>
{
    if let Some(built_in) = built_ins.get(&executable)
    {
        return built_in(location, &args);
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


pub type BuiltIn<'a> = Box<dyn Fn(&Location, &[String]) -> Result<(), InterpreterError> + 'a>;

pub type BuiltIns<'a> = HashMap<String, BuiltIn<'a>>;


pub fn interpret(instructions: Vec<Instruction>, built_ins: &BuiltIns) -> Result<(), InterpreterError>
{
    let mut stack: VecDeque<Value> = VecDeque::new();
    let mut instruction_pointer: usize = 0;
    let mut location: Location = Location::default();

    while instruction_pointer < instructions.len()
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
                        push(&mut stack, operand.clone());
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
                            args.push(pop_as_text(&location, &mut stack)?);
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

                    let executable = pop_as_text(&location, &mut stack)?;

                    execute(&location, built_ins,executable, args)?;
                }
        }

        instruction_pointer += 1;
    }

    Ok(())
}
