
use crate::{ language::{ ast::*,
                         text::location::Location,
                         tokenizer::{ TokenBuffer, Token, TokenKind, TokenValue } },
             result::{ CoreResult, CoreError, CoreErrorKind } };


mod lookahead_guard;
mod basics;

// Make the LookaheadGuard and all the 'basic' functions available for use in the rest of this
// module.
use lookahead_guard::LookaheadGuard;
use basics::*;

mod expressions;

use expressions::{CallContext, parse_call, parse_expression};

mod statements;

use statements::parse_statement;



pub fn parse(input: &mut TokenBuffer<'_, '_>) -> CoreResult<TopLevel>
{
    let mut top_level = TopLevel::default();

    while let Some(statement) = parse_statement(input)?
    {
        top_level.push(statement);
    }

    Ok(top_level)
}
