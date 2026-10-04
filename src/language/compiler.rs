
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


fn compile_expression(instructions: &mut Vec<Instruction>, expression: &AstExpression)
{
    match &expression.kind
    {
        AstExpressionKind::Symbol(symbol) =>
            {
                instructions.push(Instruction
                    {
                        location: Some(expression.location.clone()),
                        code: Code::Push,
                        operand: Some(Value::String(symbol.name.clone()))
                    });
            },

        AstExpressionKind::Literal(value) =>
            {
                instructions.push(Instruction
                    {
                        location: Some(expression.location.clone()),
                        code: Code::Push,
                        operand: Some(value.value.clone())
                    });
            },

        AstExpressionKind::Variable(variable) =>
            {
                instructions.push(Instruction
                    {
                        location: Some(expression.location.clone()),
                        code: Code::GetVariable,
                        operand: Some(Value::String(variable.name.clone()))
                    });
            }
    }
}


fn compile_let_statement(instructions: &mut Vec<Instruction>, let_statement: &AstLetStatement)
{
    instructions.push(Instruction
        {
            location: Some(let_statement.location.clone()),
            code: Code::NewVariable,
            operand: Some(Value::String(let_statement.identifier.clone()))
        });

    compile_expression(instructions, &let_statement.expression);

    instructions.push(Instruction
        {
            location: Some(let_statement.location.clone()),
            code: Code::SetVariable,
            operand: Some(Value::String(let_statement.identifier.clone()))
        });
}


fn compile_execute_statement(instructions: &mut Vec<Instruction>,
                             execute_statement: &AstExecuteStatement)
{
    instructions.push(Instruction
        {
            location: None,
            code: Code::Push,
            operand: Some(Value::String(execute_statement.executable_name.clone()))
        });

    for argument in &execute_statement.arguments
    {
        compile_expression(instructions, argument);
    }

    instructions.push(Instruction
        {
            location: Some(execute_statement.location.clone()),
            code: Code::Execute,
            operand: Some(Value::Integer(execute_statement.arguments.len() as i64))
        });
}


pub fn compile_ast(ast: &AstTopLevel) -> CompileResult<Vec<Instruction>>
{
    let mut instructions = Vec::new();

    for ast_item in ast
    {
        match ast_item
        {
            AstStatement::NullStatement => {},

            AstStatement::LetStatement(let_statement) =>
                {
                    compile_let_statement(&mut instructions, let_statement);
                },

            AstStatement::ExecuteStatement(execute_statement) =>
                {
                    compile_execute_statement(&mut instructions, execute_statement);
                }
        }
    }

    Ok(instructions)
}
