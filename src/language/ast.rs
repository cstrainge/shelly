
use super::text::location::Location;


pub struct AstSymbol
{
    pub location: Location,
    pub name: String
}


pub enum AstExpression
{
    Symbol(AstSymbol)
}


pub struct AstExecuteStatement
{
    pub location: Location,
    pub executable_name: String,
    pub arguments: Vec<AstExpression>
}


pub enum AstStatement
{
    ExecuteStatement(Box<AstExecuteStatement>)
}


pub type AstTopLevel = Vec<AstStatement>;
