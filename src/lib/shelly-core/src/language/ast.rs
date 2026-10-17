
use std::fmt::{self, Display, Formatter };

use crate::language::{ text::location::Location, tokenizer::{ TokenLiteral, StringFlag } };



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
    String(String, StringFlag)
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
    pub fn new_literal_none(location: Location) -> Self
    {
        Expression::Literal(Box::new(LiteralExpression
            {
                location,
                value: LiteralValue::None
            }))
    }

    pub fn new_literal_expression(location: Location,
                                  value: TokenLiteral) -> Self
    {
        let value = match value
            {
                TokenLiteral::Boolean(b)   => LiteralValue::Boolean(b),
                TokenLiteral::Integer(i)   => LiteralValue::Integer(i),
                TokenLiteral::Float(f, s)  => LiteralValue::Float(f, Some(s)),
                TokenLiteral::String(s, f) => LiteralValue::String(s, f)
            };

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


impl Display for Expression
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
    {
        write!(f, "<expression>")
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
    Expression(Box<Expression>),
    Call(Box<CallExpression>)

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

    pub fn new_call_statement(location: Location,
                              identifier: String,
                              arguments: Vec<Expression>) -> Self
    {
        Statement::Call(Box::new(CallExpression
            {
                location,
                identifier,
                arguments
            }))
    }
}


pub type TopLevel = Vec<Statement>;


/**
 * Print the AST as a tree, including nested expressions and the roles of their child nodes.
 */
pub fn print_ast(ast: &TopLevel)
{
    fn print_branch(prefix: &str, last: bool, label: &str) -> String
    {
        let branch = if last { "└── " } else { "├── " };
        let continuation = if last { "    " } else { "│   " };

        println!("{prefix}{branch}{label}");

        format!("{prefix}{continuation}")
    }

    fn literal_label(value: &LiteralValue) -> String
    {
        match value
        {
            LiteralValue::None           => "None ()".into(),
            LiteralValue::Boolean(value) => format!("Boolean {value}"),
            LiteralValue::Integer(value) => format!("Integer {value}"),
            LiteralValue::Float(_, Some(representation)) =>
                {
                    format!("Float {}", representation.escape_debug())
                },

            LiteralValue::Float(value, None) => format!("Float {value:?}"),

            LiteralValue::String(value, flag) =>
                {
                    let interpolation = match flag
                        {
                            StringFlag::Interpolated    => "interpolated",
                            StringFlag::NonInterpolated => "non-interpolated"
                        };

                    // Escape control characters so a string cannot break the tree's layout.
                    format!("String ({interpolation}) {value:?}")
                }
        }
    }

    fn print_expression(expression: &Expression, prefix: &str, last: bool, label: &str)
    {
        match expression
        {
            Expression::Literal(expression) =>
                {
                    print_branch(prefix, last, &format!("{label}{}",
                                                       literal_label(&expression.value)));
                },

            Expression::MathOperation(expression) =>
                {
                    let operation = match expression.operation
                        {
                            MathOperation::Addition       => "Addition (+)",
                            MathOperation::Subtraction    => "Subtraction (-)",
                            MathOperation::Multiplication => "Multiplication (*)",
                            MathOperation::Division       => "Division (/)",
                            MathOperation::Modulus        => "Modulus (%)"
                        };

                    let children = print_branch(prefix, last, &format!("{label}{operation}"));

                    print_expression(&expression.left, &children, false, "Left: ");
                    print_expression(&expression.right, &children, true, "Right: ");
                },

            Expression::Call(call) => print_call(call, prefix, last, &format!("{label}Call: "))
        }
    }

    fn print_call(call: &CallExpression, prefix: &str, last: bool, label: &str)
    {
        let children = print_branch(prefix, last, &format!("{label}{}",
                                                           call.identifier.escape_debug()));

        for (index, argument) in call.arguments.iter().enumerate()
        {
            print_expression(argument, &children, index + 1 == call.arguments.len(),
                             &format!("Argument {}: ", index + 1));
        }
    }

    fn print_statement(statement: &Statement, last: bool)
    {
        match statement
        {
            Statement::Null =>
                {
                    print_branch("", last, "Null statement.");
                },

            Statement::Let(statement) =>
                {
                    let visibility = match statement.visibility
                        {
                            ItemVisibility::Private => "private",
                            ItemVisibility::Public  => "public"
                        };

                    let environment = match statement.environment
                        {
                            Environment::Local    => "local",
                            Environment::Exported => "exported"
                        };

                    let children = print_branch("", last,
                        &format!("Let {} ({visibility}, {environment})",
                                 statement.identifier.escape_debug()));

                    if statement.type_annotation.is_some()
                    {
                        print_branch(&children, statement.expression.is_none(), "Type annotation");
                    }

                    if let Some(expression) = &statement.expression
                    {
                        print_expression(expression, &children, true, "Initializer: ");
                    }
                },

            Statement::Assignment(statement) =>
                {
                    let children = print_branch("", last,
                        &format!("Assignment {}", statement.identifier.escape_debug()));

                    print_expression(&statement.expression, &children, true, "Value: ");
                },

            Statement::Expression(expression) =>
                {
                    let children = print_branch("", last, "Expression statement");

                    print_expression(expression, &children, true, "");
                },

            Statement::Call(call) => print_call(call, "", last, "Call statement: ")
        }
    }

    if ast.is_empty()
    {
        println!("AST (empty)");
        return;
    }

    println!("AST");

    for (index, statement) in ast.iter().enumerate()
    {
        print_statement(statement, index + 1 == ast.len());
    }
}
