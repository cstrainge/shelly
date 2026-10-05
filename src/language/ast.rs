
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


pub enum AstStringFlag
{
    Interpolated,
    NonInterpolated,
}


pub struct AstExpression
{
    pub location: Location,
    pub kind: AstExpressionKind,
    pub string_flag: Option<AstStringFlag>
}


pub fn new_ast_variable(location: Location, name: String) -> AstExpression
{
    AstExpression
        {
            location: location.clone(),
            kind: AstExpressionKind::Variable(AstSymbol
                {
                    name
                }),
            string_flag:None
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
                }),
            string_flag:None
        }
}


pub fn new_ast_symbol(location: Location, name: String, string_flag: Option<AstStringFlag>) -> AstExpression
{
    AstExpression
        {
            location: location.clone(),
            kind: AstExpressionKind::Symbol(AstSymbol
                {
                    name
                }),
            string_flag
        }
}


pub fn new_ast_literal(location: Location,
                       value: Value,
                       string_flag: Option<AstStringFlag>) -> AstExpression
{
    AstExpression
        {
            location: location.clone(),
            kind: AstExpressionKind::Literal(AstLiteral
                {
                    value
                }),
            string_flag
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


#[derive(PartialEq, Eq)]
pub enum AstExportFlag
{
    Exported,
    NonExported,
}


pub struct AstLetStatement
{
    pub location: Location,
    pub export_flag: AstExportFlag,
    pub identifier: String,
    pub expression: AstExpression
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
                             export_flag: AstExportFlag,
                             identifier: String,
                             expression: AstExpression) -> Option<AstStatement>
{
    Some(AstStatement::LetStatement(Box::new(AstLetStatement
        {
            location,
            export_flag,
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
