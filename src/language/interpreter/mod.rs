
mod interpreter;
mod iteration;
mod redirection;
mod scope;
mod modules;
mod prompt;
mod visibility;

pub use crate::language::interpreter::interpreter::*;
pub use crate::language::interpreter::scope::Alias;
