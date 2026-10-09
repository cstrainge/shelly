
use std::fmt::{ self, Display, Formatter, Debug };

use crate::language::{ text::location::Location,
                       tokenizer::{ TokenKind, TokenizerError },
                       ast::AstError };



pub enum ParserErrorKind
{
    TokenizerError(TokenizerError),
    UnexpectedEOF(TokenKind),
    MatchError(Vec<ParserError>),
    ExpectedToken(TokenKind, Option<TokenKind>),
    ExpectedExpression,
    SplatExecutable,
    UnexpectedElse,
    ExpressionNotString,
    DuplicateParameter(String),
    NoProgress,
}


impl Display for ParserErrorKind
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
    {
        match self
        {
            ParserErrorKind::TokenizerError(error) => write!(f, "Tokenizer error: {}.", error),

            ParserErrorKind::UnexpectedEOF(kind) =>
                {
                    write!(f, "Unexpected end of file, expected token: {}.", kind)
                },

            ParserErrorKind::MatchError(errors) => write!(f, "Match error: {:?}.", errors),
            ParserErrorKind::ExpectedToken(expected, found) =>
                {
                    match found
                    {
                        Some(found) => write!(f, "Expected token: {}, but found: {}.", expected, found),
                        None => write!(f, "Expected token: {}, but found end of file.", expected)
                    }
                }

            ParserErrorKind::ExpectedExpression =>
                {
                    write!(f, "Expected expression.")
                },

            ParserErrorKind::SplatExecutable =>
                write!(f, "A variable splat cannot be used as an executable."),

            ParserErrorKind::ExpressionNotString =>
                {
                    write!(f, "Expression does not resolve to a string.")
                },

            ParserErrorKind::UnexpectedElse =>
                write!(f, "Else must follow an if branch and may appear only once at the end of a chain."),

            ParserErrorKind::DuplicateParameter(name) =>
                write!(f, "Duplicate function parameter: {}.", name),

            ParserErrorKind::NoProgress =>
                write!(f, "Repeated parser succeeded without consuming a token.")
        }
    }
}


pub struct ParserError
{
    pub location: Option<Location>,
    pub kind: ParserErrorKind
}


pub type ParseResult<T> = Result<T, ParserError>;


impl From<TokenizerError> for ParserError
{
    fn from(error: TokenizerError) -> Self
    {
        ParserError
            {
                location: None,
                kind: ParserErrorKind::TokenizerError(error),
            }
    }
}


impl From<AstError> for ParserError
{
    fn from(error: AstError) -> Self
    {
        match error
        {
            AstError::ExpressionNotString(location) =>
                ParserError
                    {
                        location: Some(location),
                        kind: ParserErrorKind::ExpressionNotString
                    }
        }
    }
}


impl Display for ParserError
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
    {
        write!(f, "{:?}", self)
    }
}


impl Debug for ParserError
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
    {
        if let Some(location) = &self.location
        {
            write!(f, "Parser error: {}: {}", location, self.kind)
        }
        else
        {
            write!(f, "Parser error: {}", self.kind)
        }
    }
}
