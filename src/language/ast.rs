
use crate::language::{ text::location::Location, data::value::Value };


pub enum AstError
{
    ExpressionNotString(Location)
}


pub struct AstSymbol
{
    pub name: String
}


pub struct AstLiteral
{
    pub value: Value
}


pub enum AstMathOperator
{
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo
}


pub enum AstExpressionKind
{
    Variable(AstSymbol),
    VariableSplat(AstSymbol),
    Symbol(AstSymbol),
    Literal(AstLiteral),
    MathExpression(AstMathOperator, Box<AstExpression>, Box<AstExpression>)
}


pub struct AstExpression
{
    pub location: Location,
    pub kind: AstExpressionKind
}


pub fn new_ast_variable(location: Location, name: String) -> AstExpression
{
    AstExpression
        {
            location: location.clone(),
            kind: AstExpressionKind::Variable(AstSymbol
                {
                    name
                })
        }
}


pub fn new_ast_variable_splat(location: Location, name: String) -> AstExpression
{
    AstExpression
        {
            location: location.clone(),
            kind: AstExpressionKind::VariableSplat(AstSymbol
                {
                    name
                })
        }
}


pub fn new_ast_symbol(location: Location, name: String) -> AstExpression
{
    AstExpression
        {
            location: location.clone(),
            kind: AstExpressionKind::Symbol(AstSymbol
                {
                    name
                })
        }
}


pub fn new_ast_literal(location: Location, value: Value) -> AstExpression
{
    AstExpression
        {
            location: location.clone(),
            kind: AstExpressionKind::Literal(AstLiteral
                {
                    value
                })
        }
}


impl AstExpression
{
    pub fn resolve_as_text(&self) -> Result<String, AstError>
    {
        match &self.kind
        {
            AstExpressionKind::Symbol(symbol) => Ok(symbol.name.clone()),
            AstExpressionKind::Variable(variable) => Ok(variable.name.clone()),
            AstExpressionKind::VariableSplat(variable) => Ok(variable.name.clone()),
            AstExpressionKind::Literal(literal) => Ok(literal.value.as_text()),
            AstExpressionKind::MathExpression(_, _, _) =>
                Err(AstError::ExpressionNotString(self.location.clone()))
        }
    }
}


pub struct AstLetStatement
{
    pub location: Location,
    pub identifier: String,
    pub expression: AstExpression,
}


pub struct AstExecuteStatement
{
    pub location: Location,
    pub executable_name: String,
    pub arguments: Vec<AstExpression>
}


pub struct AstSetStatement
{
    pub location: Location,
    pub identifier: String,
    pub expression: AstExpression,
}


pub fn new_ast_let_statement(location: Location,
                             identifier: String,
                             expression: AstExpression) -> Option<AstStatement>
{
    Some(AstStatement::LetStatement(Box::new(AstLetStatement
        {
            location,
            identifier,
            expression,
        })))
}


pub fn new_ast_set_statement(location: Location,
                             identifier: String,
                             expression: AstExpression) -> Option<AstStatement>
{
    Some(AstStatement::SetStatement(Box::new(AstSetStatement
        {
            location,
            identifier,
            expression,
        })))
}


pub fn new_ast_execute_statement(location: Location,
                                 executable_name: String,
                                 arguments: Vec<AstExpression>) -> Option<AstStatement>
{
    Some(AstStatement::ExecuteStatement(Box::new(AstExecuteStatement
        {
            location,
            executable_name,
            arguments,
        })))
}


pub enum AstStatement
{
    LetStatement(Box<AstLetStatement>),
    SetStatement(Box<AstSetStatement>),
    ExecuteStatement(Box<AstExecuteStatement>),
    NullStatement
}


pub type AstTopLevel = Vec<AstStatement>;
