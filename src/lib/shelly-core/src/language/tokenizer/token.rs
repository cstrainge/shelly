
use std::fmt::{ self, Display, Formatter };

use crate::language::text::location::Location;



#[derive(Clone)]
pub enum TokenKind
{
    Invalid,
}


/**
 * Represents the different types of literals that can be associated with a token.
 */
#[derive(Clone)]
pub enum TokenLiteral
{
    /**
     * An integer literal value.
     */
    Integer(i64),

    /**
     * A floating-point literal value.
     */
    Float(f64, String),

    /**
     * A boolean literal value.
     */
    Boolean(bool),

    /**
     * A string literal value.
     */
    String(String)
}


impl Display for TokenLiteral
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
    {
        match self
        {
            TokenLiteral::Integer(value) => write!(f, "{}", value),
            TokenLiteral::Float(_, repr) => write!(f, "{}", repr),
            TokenLiteral::Boolean(value) => write!(f, "{}", value),
            TokenLiteral::String(value)  => write!(f, "{}", value)
        }
    }
}

/**
 * Represents a value that can be associated with a token, such as a literal, identifier, or none.
 */
#[derive(Clone)]
pub enum TokenValue
{
    None,
    Literal(TokenLiteral),
    Identifier(String),
    Symbol(String)
}


impl Display for TokenValue
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
    {
        match self
        {
            TokenValue::None                   => write!(f, "()"),
            TokenValue::Literal(literal)       => write!(f, "{}", literal),
            TokenValue::Identifier(identifier) => write!(f, "{}", identifier),
            TokenValue::Symbol(symbol)         => write!(f, "{}", symbol)
        }
    }
}


/**
 * Represents a single token of language as extracted from the input source code.
 */
#[derive(Clone)]
pub struct Token
{
    /**
     * The logical location in the source code this token was extracted from.
     */
    pub location: Location,

     /**
     * The type of the token, indicating its role in the language syntax.
     */
    pub kind: TokenKind,

    /**
     * The value of the token, which may be a literal, identifier, or none depending on the token
     * type.
     */
    pub value: TokenValue
}
