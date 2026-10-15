
use std::fmt::{ self, Debug, Display, Formatter };

use crate::language::text::location::Location;


/**
 * Represents all of the kinds of errors that can occur in this crate.
 */
pub enum CoreErrorKind
{
    /**
     * An error occurred during lexing of the source code.
     */
    LexicalError
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
            write!(f, "Lexical error: {}", location)?;
        }
        else
        {
            write!(f, "Lexical error")?;
        }

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
}


impl Display for CoreError
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
    {
        match self.kind
        {
            CoreErrorKind::LexicalError => self.fmt_lexical_error(f)
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
