
use crate::language::{ ast::AstTopLevel,
                       tokenizer::{ TokenBuffer, Tokenizer },
                       parser::{ statements::parse_statement, results::ParseResult } };
pub use crate::language::parser::results::ParserError;

mod results;
mod base_utils;
mod expressions;
mod statements;

pub type ParserResult<T> = ParseResult<T>;



/**
 * Take a block of source code and parse it into an abstract syntax tree (AST) representing the
 * top-level structure, all the way down to individual expressions.
 */
pub fn parse_text<'a>(tokenizer: &mut Tokenizer<'a>) -> ParserResult<AstTopLevel>
{
    let mut token_buffer = TokenBuffer::new(tokenizer);
    let mut ast_top_level = AstTopLevel::new();

    while let Some(statement) = parse_statement(&mut token_buffer)?
    {
        ast_top_level.push(statement);
    }

    Ok(ast_top_level)
}
