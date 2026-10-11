
mod interpreter;
mod conversions;
mod iteration;
mod redirection;
mod pipeline;
mod scope;
mod modules;
mod bundled_std;
mod prompt;
mod visibility;
mod properties;

pub use crate::language::interpreter::interpreter::*;
pub use crate::language::interpreter::scope::Alias;
