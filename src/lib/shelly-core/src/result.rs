
use std::fmt::{ self, Debug, Display, Formatter };

use crate::language::{ data::value::TypeId, text::location::Location };



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

    /**
     * Format a type error for display to the user.
     */
    fn fmt_type_error(&self, f: &mut Formatter<'_>, expected: TypeId, found: TypeId) -> fmt::Result
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
        match self.kind
        {
            CoreErrorKind::LexicalError => self.fmt_lexical_error(f),
            CoreErrorKind::TypeError(expected, found) => self.fmt_type_error(f, expected, found)
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
