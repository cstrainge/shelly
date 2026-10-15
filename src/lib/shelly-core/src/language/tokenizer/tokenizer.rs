
use crate::{ language::{ text::buffer::Buffer, tokenizer::token::Token },
             result::CoreResult };


/**
 * The Shelly tokenizer lazily extracts tokens from the input buffer. It only supports sequential
 * extraction with zero lookahead.
 *
 * To enable lookahead and buffering look at `TokenBuffer` that wraps this tokenizer and provides
 * buffering and lookahead capabilities.
 */
pub struct Tokenizer<'a>
{
    /**
     * The input stream from which tokens are extracted. It only supports sequential extraction
     * with a single character of lookahead. Once a character is consumed, it cannot be revisited.
     */
    input: &'a mut dyn Buffer,
}


impl<'a> Tokenizer<'a>
{
    /**
     * Creates a new tokenizer that extracts tokens from the given input buffer.
     */
    pub fn new(input: &'a mut dyn Buffer) -> Self
    {
        Self { input }
    }

    /**
     * Extract the next token from the input buffer. Returns `None` if the end of the input is
     * reached.
     */
    pub fn next(&mut self) -> CoreResult<Option<Token>>
    {
        Ok(None)
    }
}
