
use crate::language::{ text::location::Location, data::value::Value };


#[derive(Debug)]
pub enum AstError
{
    ExpressionNotString(Location)
}


pub struct AstSymbol
{
    pub name: String
}


impl AstSymbol
{
    pub fn is_glob(&self) -> bool
    {
        self.name.contains(['*', '?', '['])
    }
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


pub enum AstBooleanOperator
{
    Equal,
    NotEqual,
    And,
    Or
}


pub enum AstExpressionKind
{
    Variable(AstSymbol),
    VariableSplat(AstSymbol),
    Array(Vec<AstExpression>),
    HashMap(Vec<(AstExpression, AstExpression)>),
    Index(Box<AstExpression>, Box<AstExpression>),
    Splat(Box<AstExpression>),
    Symbol(AstSymbol),
    Literal(AstLiteral),
    Grouped(Box<AstExpression>),
    // Preserve the referenced value without implicitly invoking it as an argument.
    ExecutableReference(Box<AstExpression>),
    Execute(Box<AstExecuteStatement>),
    TryExecute(Box<AstExpression>),
    MathExpression(AstMathOperator, Box<AstExpression>, Box<AstExpression>),
    BooleanExpression(AstBooleanOperator, Box<AstExpression>, Box<AstExpression>),
    BooleanNot(Box<AstExpression>),
    IfExpression(Box<AstIfExpression>)
}


pub enum AstStringFlag
{
    Interpolated(Vec<usize>),
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
            AstExpressionKind::Grouped(_)
            | AstExpressionKind::Array(_)
            | AstExpressionKind::HashMap(_)
            | AstExpressionKind::Index(_, _)
            | AstExpressionKind::Splat(_)
            | AstExpressionKind::ExecutableReference(_)
            | AstExpressionKind::Execute(_)
            | AstExpressionKind::TryExecute(_)
            | AstExpressionKind::MathExpression(_, _, _)
            | AstExpressionKind::BooleanExpression(_, _, _)
            | AstExpressionKind::BooleanNot(_)
            | AstExpressionKind::IfExpression(_) =>
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
    pub executable: AstExpression,
    pub expand_path: bool,
    pub arguments: Vec<AstExpression>
}


pub struct AstSetStatement
{
    pub location: Location,
    pub identifier: String,
    pub indexes: Vec<AstExpression>,
    pub expression: AstExpression,
}


pub struct AstAliasArgument
{
    pub value: Value,
    pub expand_path: bool
}


pub struct AstAliasStatement
{
    pub location: Location,
    pub alias: String,
    pub target: String,
    pub arguments: Vec<AstAliasArgument>
}


pub struct AstFunctionStatement
{
    //pub location: Location,
    pub name: String,
    pub parameters: Vec<String>,
    pub body: AstTopLevel
}


pub struct AstReturnStatement
{
    pub location: Location,
    pub expression: Option<AstExpression>
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
                             indexes: Vec<AstExpression>,
                             expression: AstExpression) -> Option<AstStatement>
{
    Some(AstStatement::SetStatement(Box::new(AstSetStatement
        {
            location,
            identifier,
            indexes,
            expression,
        })))
}


pub fn new_ast_execute_statement(location: Location,
                                 expand_path: bool,
                                 executable: AstExpression,
                                 arguments: Vec<AstExpression>) -> Option<AstStatement>
{
    Some(AstStatement::ExecuteStatement(Box::new(AstExecuteStatement
        {
            location,
            executable,
            expand_path,
            arguments,
        })))
}


pub fn new_ast_function_statement(name: String,
                                  parameters: Vec<String>,
                                  body: Vec<AstStatement>) -> Option<AstStatement>
{
    Some(AstStatement::FunctionDefinition(Box::new(AstFunctionStatement
        {
            name,
            parameters,
            body,
        })))
}


pub fn new_ast_alias_statement(location: Location,
                               alias: String,
                               target: String,
                               arguments: Vec<AstAliasArgument>) -> Option<AstStatement>
{
    Some(AstStatement::AliasStatement(Box::new(AstAliasStatement
        {
            location,
            alias,
            target,
            arguments,
        })))
}


pub struct AstBlockStatement
{
    pub location: Location,
    pub body: AstTopLevel
}


pub struct AstIfBranch
{
    pub condition: AstExpression,
    pub body: AstBlockStatement
}


pub struct AstIfExpression
{
    pub location: Location,
    pub branches: Vec<AstIfBranch>,
    pub else_body: Option<AstBlockStatement>
}


pub enum AstStatement
{
    LetStatement(Box<AstLetStatement>),
    SetStatement(Box<AstSetStatement>),
    AliasStatement(Box<AstAliasStatement>),
    ExecuteStatement(Box<AstExecuteStatement>),
    ExpressionStatement(AstExpression),
    ReturnStatement(Box<AstReturnStatement>),
    FunctionDefinition(Box<AstFunctionStatement>),
    BlockStatement(Box<AstBlockStatement>),
    NullStatement
}


pub type AstTopLevel = Vec<AstStatement>;
