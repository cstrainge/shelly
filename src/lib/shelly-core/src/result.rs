
use std::fmt::{ self, Debug, Display, Formatter };

use crate::language::{ data::value::TypeId, text::location::Location, tokenizer::TokenKind };



/**
 * Represents all of the kinds of errors that can occur in this crate.
 */
pub enum CoreErrorKind
{
    /**
     * An error occurred during lexing of the source code.
     */
    LexicalError,

    /**
     * An error indicating that a pattern match failed. Contains a vector of the errors that
     * occurred during the sub-match attempts.
     */
    FailedMatch(Vec<CoreError>),

    /**
     * An unexpected token was encountered in the source code. The first `TokenKind` represents the
     * expected token, and the second `TokenKind` represents the actual token found.
     */
    UnexpectedToken(TokenKind, TokenKind),

    /**
     * An error indicating that an expression was expected but not found.
     */
    ExpectedExpression,

    /**
     * An error indicating that an expression was expected but not found. Contains a vector of the
     * token kinds that were expected as a terminator.
     */
    ExpectedExpressionOr(Vec<TokenKind>),

    /**
     * An unexpected end of file was encountered in the source code. The `TokenKind` represents the
     * token that was expected but not found.
     */
    UnexpectedEOF(TokenKind),

    /**
     * The code expected a value to be one type but found another type.
     */
    TypeError(TypeId, TypeId)
}


pub struct CoreError
{
    /**
     * If supplied, the location in the source code where the error occurred.
     */
    pub location: Option<Location>,

    /**
     * The kind of error that occurred.
     */
    pub kind: CoreErrorKind,

    /**
     * An optional message providing additional details about the error.
     */
    pub message: Option<String>
}


/**
 * A specialized `Result` type for operations that can return a `CoreError`.
 */
pub type CoreResult<T> = Result<T, CoreError>;


impl CoreError
{
    /**
     * Format a lexical error for display to the user.
     */
    fn fmt_lexical_error(&self, f: &mut Formatter<'_>) -> fmt::Result
    {
        if let Some(location) = &self.location
        {
            write!(f, "{}: ", location)?;
        }

        write!(f, "Lexical error")?;

        if let Some(message) = &self.message
        {
            write!(f, ": {}", message)?;
        }
        else
        {
            write!(f, ".")?;
        }

        Ok(())
    }


    fn fmt_failed_match(&self, f: &mut Formatter<'_>,
                        errors: &[CoreError]) -> fmt::Result
    {
        if let Some(location) = &self.location
        {
            write!(f, "{}: ", location)?;
        }

        write!(f, "Failed match")?;

        if let Some(message) = &self.message
        {
            write!(f, ": {}", message)?;
        }
        else
        {
            write!(f, ". Attempted sub-match errors:")?;

            for error in errors
            {
                write!(f, "\n  {}", error)?;
            }
        }

        Ok(())
    }

    /**
     * Format an unexpected token error for display to the user.
     */
    fn fmt_unexpected_token(&self, f: &mut Formatter<'_>,
                            expected: &TokenKind,
                            found: &TokenKind) -> fmt::Result
    {
        if let Some(location) = &self.location
        {
            write!(f, "{}: ", location)?;
        }

        write!(f, "Unexpected token")?;

        if let Some(message) = &self.message
        {
            write!(f, ": {}", message)?;
        }
        else
        {
            write!(f, ". Expected {:?}, found {:?}", expected, found)?;
        }

        Ok(())
    }

    fn fmt_expected_expression(&self, f: &mut Formatter<'_>) -> fmt::Result
    {
        if let Some(location) = &self.location
        {
            write!(f, "{}: ", location)?;
        }

        write!(f, "Expected expression")?;

        if let Some(message) = &self.message
        {
            write!(f, ": {}", message)?;
        }
        else
        {
            write!(f, ".")?;
        }

        Ok(())
    }

    fn fmt_expected_expression_or(&self, f: &mut Formatter<'_>,
                                  stop_tokens: &[TokenKind]) -> fmt::Result
    {
        if let Some(location) = &self.location
        {
            write!(f, "{}: ", location)?;
        }

        write!(f, "Expected expression or one of the following tokens: {:?}", stop_tokens)?;

        if let Some(message) = &self.message
        {
            write!(f, ": {}", message)?;
        }
        else
        {
            write!(f, ".")?;
        }

        Ok(())
    }

    /**
     * Format an unexpected end of file error for display to the user.
     */
    fn fmt_unexpected_eof(&self, f: &mut Formatter<'_>,
                           expected: &TokenKind) -> fmt::Result
    {
        if let Some(location) = &self.location
        {
            write!(f, "{}: ", location)?;
        }

        write!(f, "Unexpected end of file")?;

        if let Some(message) = &self.message
        {
            write!(f, ": {}", message)?;
        }
        else
        {
            write!(f, ". Expected {:?}", expected)?;
        }

        Ok(())
    }

    /**
     * Format a type error for display to the user.
     */
    fn fmt_type_error(&self, f: &mut Formatter<'_>,
                      expected: &TypeId,
                      found: &TypeId) -> fmt::Result
    {
        if let Some(location) = &self.location
        {
            write!(f, "{}: ", location)?;
        }

        write!(f, "Type error")?;

        if let Some(message) = &self.message
        {
            write!(f, ": {}", message)?;
        }
        else
        {
            write!(f, ". Expected {:?}, found {:?}", expected, found)?;
        }

        Ok(())
    }
}


impl Display for CoreError
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
    {
        match &self.kind
        {
            CoreErrorKind::LexicalError =>
                {
                    self.fmt_lexical_error(f)
                },

            CoreErrorKind::FailedMatch(errors) =>
                {
                    self.fmt_failed_match(f, errors)
                },

            CoreErrorKind::UnexpectedToken(expected, found) =>
                {
                    self.fmt_unexpected_token(f, expected, found)
                },

            CoreErrorKind::ExpectedExpression =>
                {
                    self.fmt_expected_expression(f)
                },

            CoreErrorKind::ExpectedExpressionOr(stop_tokens) =>
                {
                    self.fmt_expected_expression_or(f, stop_tokens)
                },

            CoreErrorKind::UnexpectedEOF(expected) =>
                {
                    self.fmt_unexpected_eof(f, expected)
                },

            CoreErrorKind::TypeError(expected, found) =>
                {
                    self.fmt_type_error(f, expected, found)
                }
        }
    }
}


impl Debug for CoreError
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
    {
        write!(f, "{}", self)
    }
}


impl std::error::Error for CoreError
{
}
