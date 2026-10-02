
use super::text::location::Location;



pub struct AstVariableDefinition
{
    pub location: Location,
    pub name: String,
    pub type_name: Option<String>,
    pub expression: Option<AstExpression>
}


pub struct AstVariableAssignment
{
    pub location: Location,
    pub name: String,
    pub expression: AstExpression
}


pub struct AstOperation
{
    pub location: Location,
    pub left: AstExpression,
    pub operator: String,
    pub right: AstExpression
}


pub enum AstLiteral
{
    Boolean(bool),
    Integer(i64),
    Number(f64),
    String(String),
    Empty
}


pub struct AstVariableRef
{
    pub location: Location,
    pub name: String
}


pub enum AstExpression
{
    VariableRef(AstVariableRef),
    Literal(AstLiteral),
    Execute(Box<AstExecuteStatement>),
    Operation(Box<AstOperation>),
    Expression(Box<AstExpression>)
}


pub struct AstExecuteStatement
{
    pub location: Location,
    pub executable_name: String,
    pub arguments: Vec<AstExpression>
}


pub enum AstStatement
{
    VariableDefinition(Box<AstVariableDefinition>),
    VariableAssignment(Box<AstVariableAssignment>),
    ExecuteStatement(Box<AstExecuteStatement>)
}


pub type AstTopLevel = Vec<AstStatement>;
