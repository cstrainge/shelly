
use std::fmt::{ self, Display, Formatter };

use crate::language::text::location::Location;



/**
 * The different kinds of tokens that can be recognized by the tokenizer.
 */
#[derive(Clone, Debug)]
pub enum TokenKind
{
    /**
     * Literal token representing things like numbers, strings, and booleans.
     */
    Literal,

    /**
     * A variable name in the source. It always starts with a $.
     */
    Identifier,

    /**
     * The auto identifier. It's only a $.
     */
    AutoIdentifier,

    /**
     * A symbol or block of text in the source code, not to be confused with a string literal.
     * Symbols are used for various syntactic elements. Like executable names.
     */
    Symbol,

    /**
     * The let keyword used for variable declaration.
     */
    Let,

    /**
     * The export keyword used for exporting variables to sub processes.
     */
    Export,

    /**
     * The pub keyword used for making items publicly exported from their module.
     */
    Pub,

    /**
     * The sub keyword for spawning a sub-process from a block of code.
     */
    Sub,

    /**
     * The if keyword used for conditional branching.
     */
    If,

    /**
     * The else keyword used for conditional branching when the if condition is not met.
     */
    Else,

    /**
     * The while keyword used for looping while a condition is true.
     */
    While,

    /**
     * The loop keyword used for creating an non-self-terminating loop.
     */
    Loop,

    /**
     * The match keyword used for pattern matching.
     */
    Match,

    /**
     * The return keyword used for returning a value or just early exit from a function.
     */
    Return,

    /**
     * The fn keyword used for defining a function.
     */
    Fn,

    /**
     * The struct keyword used for defining a structure.
     */
    Struct,

    /**
     * The enum keyword used for defining an enumeration.
     */
    Enum,

    /**
     * The import keyword used for importing modules or items.
     */
    Import,

    /**
     * The pipe operator used for piping output from one command to another.
     */
    Pipe,

    /**
     * The statement break token, a ; that separates commands.
     */
    StatementBreak,

    /**
     * A regular parenthesis open token.
     */
    ParenOpen,

    /**
     * A regular parenthesis close token.
     */
    ParenClose,

    /**
     * A regular square bracket open token.
     */
    SquareOpen,

    /**
     * A regular square bracket close token.
     */
    SquareClose,

    /**
     * The exec escape token, used for escaping execution in certain contexts. Represented by a `.
     */
    ExecEscape,

    /**
     * A line break token, used for indicating the end of a line.
     */
    LineBreak,

    /**
     * A line continue token, used for indicating that the current line continues onto the next
     * line.
     */
    LineContinue,

    /**
     * A comma token, used for separating items in a list.
     */
    Comma,

    /**
     * A scope token, used for indicating the scope of a function or executable.
     */
    Scope,

    /**
     * The separator token between a name and it's type or return type.
     */
    TypeDelimiter,

    /**
     * The assignment operator.
     */
    Assign,

    /**
     * A open bracket for a block of code.
     */
    BlockOpen,

    /**
     * A close bracket for a block of code.
     */
    BlockClose,

    /**
     * The minus operator. Unary or binary usage.
     */
    Minus,

    /**
     * Indicates using std error as the source of text.
     */
    ErrorSource,

    /**
     * The plus operator. Unary or binary usage.
     */
    Plus,

    /**
     * The multiply or glob operator.
     */
    Asterisk,

    /**
     * The division operator.
     */
    Slash,

    /**
     * The modulo operator.
     */
    Percent,

    /**
     * The redirect to operator.
     */
    RedirectTo,

    /**
     * The redirect from operator.
     */
    RedirectFrom,

    /**
     * Always represents a file glob operation.
     */
    Glob,

    /**
     * The splat operator, used for unpacking or spreading elements.
     */
    Splat
}


/**
 * Indicate if the string should participate in interpolation or not.
 */
#[derive(Clone, PartialEq, Eq)]
pub enum StringFlag
{
    Interpolated,
    NonInterpolated
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
     * A floating-point literal value. We also take the original string representation for accurate
     * formatting and error reporting in case the number is to be passed as text to an external
     * binary. This allows us to make sure we reproduce the exact textual representation when
     * needed.
     */
    Float(f64, String),

    /**
     * A boolean literal value.
     */
    Boolean(bool),

    /**
     * A string literal value.
     */
    String(String, StringFlag)
}


impl Display for TokenLiteral
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
    {
        match self
        {
            TokenLiteral::Integer(value)   => write!(f, "{}", value),
            TokenLiteral::Float(_, repr)   => write!(f, "{}", repr),
            TokenLiteral::Boolean(value)   => write!(f, "{}", value),
            TokenLiteral::String(value, _) => write!(f, "{:?}", value)
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


impl Display for Token
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
    {
        write!(f, "{}: {:?} - {}", self.location, self.kind, self.value)
    }
}
