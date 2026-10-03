
use std::fmt::{ self, Display, Formatter };

use crate::language::{ ast::*,
                       bytecode::{ Code, Instruction },
                       data::value::Value,
                       text::location::Location,
                       parser::ParserError };


pub enum ErrorWhat
{
    ParserError(ParserError),
}


pub struct CompileError
{
    pub location: Option<Location>,
    pub what: ErrorWhat
}


impl Display for CompileError
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
    {
        match &self.what
        {
            ErrorWhat::ParserError(error) =>
                {
                    if let Some(location) = &self.location
                    {
                        write!(f, "Parser error: {}: {}", location, error)
                    }
                    else
                    {
                        write!(f, "Parser error: {}", error)
                    }
                }
        }
    }
}


impl From<ParserError> for CompileError
{
    fn from(error: ParserError) -> Self
    {
        CompileError
        {
            location: None,
            what: ErrorWhat::ParserError(error)
        }
    }
}


pub type CompileResult<T> = Result<T, CompileError>;


pub fn compile_ast(ast: &AstTopLevel) -> CompileResult<Vec<Instruction>>
{
    let mut instructions = Vec::new();

    for ast_item in ast
    {
        match ast_item
        {
            AstStatement::ExecuteStatement(execute_statement) =>
                {
                    instructions.push(Instruction
                        {
                            location: None,
                            code: Code::Push,
                            operand: Some(Value::String(execute_statement.executable_name.clone()))
                        });

                    for argument in &execute_statement.arguments
                    {
                        match argument
                        {
                            AstExpression::Symbol(symbol) =>
                                {
                                    instructions.push(Instruction
                                        {
                                            location: Some(symbol.location.clone()),
                                            code: Code::Push,
                                            operand: Some(Value::String(symbol.name.clone()))
                                        });
                                }
                        }
                    }

                    instructions.push(Instruction
                        {
                            location: Some(execute_statement.location.clone()),
                            code: Code::Execute,
                            operand: Some(Value::Integer(execute_statement.arguments.len() as i64))
                        });
                }
        }
    }

    Ok(instructions)
}
