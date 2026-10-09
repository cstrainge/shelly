
use crate::language::{ text::location::Location, data::{ value::Value, types::TypeId } };

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
    // Same-line `name ()`: the checker resolves the type/command ambiguity.
    SpacedEmptyCall(String),
    EnumVariant(String, String),
    StructConstructor(Box<AstStructConstructor>),
    Field(Box<AstExpression>, String, Option<usize>),
    Variable(AstSymbol),
    VariableSplat(AstSymbol),
    Array(Vec<AstExpression>),
    HashMap(Vec<(AstExpression, AstExpression)>),
    Range(Option<Box<AstExpression>>, Option<Box<AstExpression>>, bool),
    Index(Box<AstExpression>, Box<AstExpression>),
    Splat(Box<AstExpression>),
    Symbol(AstSymbol),
    Literal(AstLiteral),
    Grouped(Box<AstExpression>),
    // Preserve the referenced value without implicitly invoking it as an argument.
    ExecutableReference(Box<AstExpression>),
    Execute(Box<AstExecuteStatement>),
    Redirect(Box<AstExpression>, Vec<AstRedirection>),
    TryExecute(Box<AstExpression>),
    MathExpression(AstMathOperator, Box<AstExpression>, Box<AstExpression>),
    BooleanExpression(AstBooleanOperator, Box<AstExpression>, Box<AstExpression>),
    BooleanNot(Box<AstExpression>),
    TypeConversion(String, Box<AstExpression>, Option<TypeId>),
    IfExpression(Box<AstIfExpression>),
    MatchExpression(Box<AstMatchExpression>)
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


pub fn new_ast_symbol(location: Location, name: String,
                      string_flag: Option<AstStringFlag>) -> AstExpression
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
            AstExpressionKind::EnumVariant(_, _) =>
                Err(AstError::ExpressionNotString(self.location.clone())),
            AstExpressionKind::Variable(variable) => Ok(variable.name.clone()),
            AstExpressionKind::VariableSplat(variable) => Ok(variable.name.clone()),
            AstExpressionKind::Literal(literal) => Ok(literal.value.as_text()),
            AstExpressionKind::Grouped(_)
            | AstExpressionKind::Array(_)
            | AstExpressionKind::HashMap(_)
            | AstExpressionKind::Range(_, _, _)
            | AstExpressionKind::StructConstructor(_)
            | AstExpressionKind::SpacedEmptyCall(_)
            | AstExpressionKind::Field(_, _, _)
            | AstExpressionKind::Index(_, _)
            | AstExpressionKind::Splat(_)
            | AstExpressionKind::ExecutableReference(_)
            | AstExpressionKind::Execute(_)
            | AstExpressionKind::Redirect(_, _)
            | AstExpressionKind::TryExecute(_)
            | AstExpressionKind::MathExpression(_, _, _)
            | AstExpressionKind::BooleanExpression(_, _, _)
            | AstExpressionKind::BooleanNot(_)
            | AstExpressionKind::TypeConversion(_, _, _)
            | AstExpressionKind::IfExpression(_)
            | AstExpressionKind::MatchExpression(_) =>
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
    pub annotation: Option<AstType>,
    pub type_id: Option<TypeId>,
    pub default_initialize: bool,
    pub expression: AstExpression
}


pub struct AstExecuteStatement
{
    pub location: Location,
    pub executable: AstExpression,
    pub expand_path: bool,
    pub arguments: Vec<AstExpression>
}


#[derive(Clone, Copy)]
pub enum RedirectStream
{
    Output,
    Error,
    Both
}


pub struct AstRedirection
{
    pub location: Location,
    pub stream: RedirectStream,
    pub target: AstExpression
}


pub struct AstSetStatement
{
    pub location: Location,
    pub identifier: String,
    pub indexes: Vec<AstAccess>,
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


pub struct AstParameter
{
    pub location: Location,
    pub name: String,
    pub annotation: Option<AstType>,
    pub optional: bool,
    pub variadic: bool,
    pub type_id: Option<TypeId>
}


pub struct AstFunctionStatement
{
    pub location: Location,
    pub name: String,
    pub receiver: Option<String>,
    pub receiver_type: Option<TypeId>,
    pub parameters: Vec<AstParameter>,
    pub return_annotation: Option<AstType>,
    pub return_type: Option<TypeId>,
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
            annotation: None,
            type_id: None,
            default_initialize: false,
            expression,
        })))
}


pub fn new_ast_set_statement(location: Location,
                             identifier: String,
                             indexes: Vec<AstAccess>,
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


pub fn new_ast_function_statement(location: Location,
                                  name: String,
                                  parameters: Vec<AstParameter>,
                                  return_annotation: Option<AstType>,
                                  body: Vec<AstStatement>) -> Option<AstStatement>
{
    Some(AstStatement::FunctionDefinition(Box::new(AstFunctionStatement
        {
            location,
            name,
            receiver: None,
            receiver_type: None,
            parameters,
            return_annotation,
            return_type: None,
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


pub struct AstMatchArm
{
    // None denotes the bare wildcard, not a value expression.
    pub pattern: Option<AstExpression>,
    pub body: AstBlockStatement
}


pub struct AstMatchExpression
{
    pub location: Location,
    pub value: AstExpression,
    pub arms: Vec<AstMatchArm>
}


pub struct AstForStatement
{
    pub location: Location,
    pub bindings: Vec<String>,
    pub destructure: bool,
    pub iterable: AstExpression,
    pub body: AstBlockStatement
}


pub struct AstConditionalLoopStatement
{
    pub condition: AstExpression,
    pub body: AstBlockStatement,
    pub until: bool
}


pub enum AstType
{
    Named(String),
    Array(Box<AstType>),
    Map(Box<AstType>, Box<AstType>),
    Optional(Box<AstType>)
}

pub struct AstFieldDeclaration
{
    pub name: String,
    pub annotation: AstType,
    pub optional: bool,
    pub location: Location
}

pub struct AstStructDeclaration
{
    pub name: String,
    pub fields: Vec<AstFieldDeclaration>,
    pub location: Location
}

pub struct AstStructConstructor
{
    pub name: String,
    pub fields: Vec<(String, Location, AstExpression)>,
    pub type_id: Option<TypeId>,
    pub field_indexes: Vec<usize>
}

pub enum AstAccess
{
    Index(AstExpression),
    Field(String)
}


pub struct AstEnumDeclaration
{
    pub location: Location,
    pub name: String,
    pub variants: Vec<(String, Location)>
}


pub enum AstStatement
{
    EnumDeclaration(Box<AstEnumDeclaration>),
    StructDeclaration(Box<AstStructDeclaration>),
    LetStatement(Box<AstLetStatement>),
    DiscardStatement(AstExpression),
    SetStatement(Box<AstSetStatement>),
    AliasStatement(Box<AstAliasStatement>),
    ExecuteStatement(Box<AstExecuteStatement>),
    ExpressionStatement(AstExpression),
    ReturnStatement(Box<AstReturnStatement>),
    FunctionDefinition(Box<AstFunctionStatement>),
    BlockStatement(Box<AstBlockStatement>),
    ForStatement(Box<AstForStatement>),
    LoopStatement(Box<AstBlockStatement>),
    ConditionalLoopStatement(Box<AstConditionalLoopStatement>),
    BreakStatement(Location),
    ContinueStatement(Location),
    NullStatement
}


pub type AstTopLevel = Vec<AstStatement>;
