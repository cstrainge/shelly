
use crate::language::text::location::Location;



pub enum ItemVisibility
{
    Private,
    Public
}


pub enum Environment
{
    Local,
    Exported
}


pub struct TypeAnnotation
{
    //
}


pub enum LiteralValue
{
    None,
    Boolean(bool),
    Integer(i64),
    Float(f64, Option<String>),
    String(String)
}


pub struct LiteralExpression
{
    pub location: Location,
    pub value: LiteralValue
}


pub enum MathOperation
{
    Addition,
    Subtraction,
    Multiplication,
    Division,
    Modulus
}


pub struct MathOperationExpression
{
    pub location: Location,
    pub operation: MathOperation,
    pub left: Box<Expression>,
    pub right: Box<Expression>
}


pub struct CallExpression
{
    pub location: Location,
    pub identifier: String,
    pub arguments: Vec<Expression>
}


pub enum Expression
{
    Literal(Box<LiteralExpression>),
    MathOperation(Box<MathOperationExpression>),
    Call(Box<CallExpression>)
}


impl Expression
{
    pub fn new_literal_expression(location: Location,
                                  value: LiteralValue) -> Self
    {
        Expression::Literal(Box::new(LiteralExpression
            {
                location,
                value
            }))
    }

    pub fn new_math_operation_expression(location: Location,
                                         operation: MathOperation,
                                         left: Expression,
                                         right: Expression) -> Self
    {
        Expression::MathOperation(Box::new(MathOperationExpression
            {
                location,
                operation,
                left: Box::new(left),
                right: Box::new(right)
            }))
    }

    pub fn new_call_expression(location: Location,
                               identifier: String,
                               arguments: Vec<Expression>) -> Self
    {
        Expression::Call(Box::new(CallExpression
            {
                location,
                identifier,
                arguments
            }))
    }
}


pub struct LetStatement
{
    pub location: Location,
    pub visibility: ItemVisibility,
    pub environment: Environment,
    pub identifier: String,
    pub type_annotation: Option<Box<TypeAnnotation>>,
    pub expression: Option<Box<Expression>>
}


pub struct SetStatement
{
    pub location: Location,
    pub identifier: String,
    pub expression: Box<Expression>
}


pub struct AssignmentStatement
{
    pub location: Location,
    pub identifier: String,
    pub expression: Box<Expression>
}


pub enum Statement
{
    Null,
    Let(Box<LetStatement>),
    Assignment(Box<AssignmentStatement>),
    Expression(Box<Expression>)

    // FunctionDefinitionStatement
    // StructDefinitionStatement
    // EnumDefinitionStatement
}


impl Statement
{
    pub fn new_null_statement() -> Self
    {
        Statement::Null
    }

    pub fn new_let_statement(location: Location,
                             visibility: ItemVisibility,
                             environment: Environment,
                             identifier: String,
                             type_annotation: Option<TypeAnnotation>,
                             expression: Option<Expression>) -> Self
    {
        Statement::Let(Box::new(LetStatement
            {
                location,
                visibility,
                environment,
                identifier,
                type_annotation: type_annotation.map(Box::new),
                expression: expression.map(Box::new)
            }))
    }

    pub fn new_assignment_statement(location: Location,
                                    identifier: String,
                                    expression: Box<Expression>) -> Self
    {
        Statement::Assignment(Box::new(AssignmentStatement
            {
                location,
                identifier,
                expression
            }))
    }

    pub fn new_expression_statement(expression: Expression) -> Self
    {
        Statement::Expression(Box::new(expression))
    }
}


pub type TopLevel = Vec<Statement>;
