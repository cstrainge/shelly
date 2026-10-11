
mod interpreter;
mod conversions;
mod iteration;
mod redirection;
mod pipeline;
mod scope;
mod modules;
mod prompt;
mod visibility;

pub use crate::language::interpreter::interpreter::*;
pub use crate::language::interpreter::scope::Alias;
