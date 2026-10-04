
use crate::language::{ tokenizer::{ Token, TokenBuffer, TokenKind },
                       parser::results::{ ParserError, ParserErrorKind, ParseResult } };



pub struct Lookahead<'buffer, 'tokenizer, 'input>
{
    pub buffer: &'buffer mut TokenBuffer<'tokenizer, 'input>,
    committed: bool
}


impl<'buffer, 'tokenizer, 'input> Lookahead<'buffer, 'tokenizer, 'input>
{
    pub fn new(buffer: &'buffer mut TokenBuffer<'tokenizer, 'input>) -> Self
    {
        buffer.mark_lookahead();

        Self
            {
                buffer,
                committed: false
            }
    }

    pub fn commit(&mut self)
    {
        self.buffer.commit_lookahead();
        self.committed = true;
    }
}


impl Drop for Lookahead<'_, '_, '_>
{
    fn drop(&mut self)
    {
        if !self.committed
        {
            self.buffer.rollback_lookahead();
        }
    }
}


pub type ParserFunction<T> = fn (buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<T>>;



pub fn expect_token(buffer: &mut TokenBuffer<'_, '_>, kind: TokenKind) -> ParseResult<Token>
{
    match buffer.next()?
    {
        None => Err(ParserError
            {
                location: None,
                kind: ParserErrorKind::UnexpectedEOF(kind)
            }),

        Some(next) if next.kind == kind => Ok(next),

        Some(next) => Err(ParserError
            {
                location: Some(next.location.clone()),
                kind: ParserErrorKind::ExpectedToken(kind, Some(next.kind))
            })
    }
}


pub fn try_expect_token(buffer: &mut TokenBuffer<'_, '_>,
                        kind: TokenKind) -> ParseResult<Option<Token>>
{
    let mut lookahead = Lookahead::new(buffer);

    let token = expect_token(&mut *lookahead.buffer, kind);

    if let Ok(token) = token
    {
        lookahead.commit();
        return Ok(Some(token));
    }

    if let Err(error) = token
    {
        match error.kind
        {
            ParserErrorKind::ExpectedToken(_, _) => return Ok(None),
            _ => return Err(error)
        }
    }

    Ok(None)
}


pub fn match_one_of<T>(buffer: &mut TokenBuffer<'_, '_>,
                       parsers: &[ParserFunction<T>]) -> ParseResult<Option<T>>
{
    let mut results = vec![];

    for parser in parsers
    {
        let mut lookahead = Lookahead::new(buffer);
        let result = parser(&mut *lookahead.buffer);

        if    let Ok(ok_result) = &result
           && let Some(_) = ok_result
        {
            lookahead.commit();
            return result;
        }

        if let Err(error) = result
        {
            // A tokenizer failure may have advanced the character stream. Token
            // lookahead cannot undo that, so another alternative must not retry.
            if matches!(&error.kind, ParserErrorKind::TokenizerError(_))
            {
                return Err(error);
            }

            results.push(error);
        }
    }

    Err(ParserError
        {
            location: None,
            kind: ParserErrorKind::MatchError(results)
        })
}


/**
 * Collect zero or more items until EOF or a terminator, consuming the terminator.
 * Non-terminating input must match an item or an error is returned.
 * Successful items are committed individually; a failed item is rolled back.
 * The caller can wrap the whole list in Lookahead to make it atomic.
 */
pub fn match_multiple_of<T>(buffer: &mut TokenBuffer<'_, '_>,
                            parsers: &[ParserFunction<T>],
                            terminators: &[TokenKind]) -> ParseResult<Vec<T>>
{
    let mut items = Vec::new();

    loop
    {
        // Consume a terminator; otherwise rewind so the item parser sees the
        // token we just inspected.
        let next =
            {
                let mut peek = Lookahead::new(buffer);
                match peek.buffer.next()?
                {
                    None => return Ok(items),
                    Some(next) if terminators.contains(&next.kind) =>
                    {
                        peek.commit();
                        return Ok(items);
                    }
                    Some(next) => next
                }
            };

        let mut lookahead = Lookahead::new(buffer);

        // This cursor is stable while the outer lookahead prevents cache cleanup.
        let start = lookahead.buffer.position();
        let item = match_one_of(&mut *lookahead.buffer, parsers)?;

        match item
        {
            Some(item) =>
                {
                    if lookahead.buffer.position() == start
                    {
                        return Err(ParserError
                        {
                            location: Some(next.location),
                            kind: ParserErrorKind::NoProgress
                        });
                    }

                    lookahead.commit();
                    items.push(item);
                },

            None => return Err(ParserError
                {
                    location: Some(next.location),
                    kind: ParserErrorKind::MatchError(Vec::new())
                })
        }
    }
}


/**
 * Parse a list enclosed by beginning and ending tokens, consuming both. A delimiter, when
 * specified, is required between items. Empty lists are allowed; trailing delimiters are not.
 * Failure rolls back the whole block.
 */
pub fn expect_block_list_of<T>(buffer: &mut TokenBuffer<'_, '_>,
                               beginning: TokenKind,
                               ending: TokenKind,
                               delimiter: Option<TokenKind>,
                               item_parser: &ParserFunction<T>) -> ParseResult<Vec<T>>
{
    let mut lookahead = Lookahead::new(buffer);
    expect_token(&mut *lookahead.buffer, beginning)?;

    let mut items = Vec::new();

    loop
    {
        // Check the closing token before requiring a separator or another item.
        // try_expect_token reports EOF as an error, so an unclosed block fails.
        if try_expect_token(&mut *lookahead.buffer, ending.clone())?.is_some()
        {
            lookahead.commit();
            return Ok(items);
        }

        if !items.is_empty()
        {
            if let Some(delimiter) = &delimiter
            {
                expect_token(&mut *lookahead.buffer, delimiter.clone())?;
            }
        }

        let location =
            {
                let peek = Lookahead::new(&mut *lookahead.buffer);
                peek.buffer.next()?.map(|token| token.location)
            };

        let start = lookahead.buffer.position();
        let item = item_parser(&mut *lookahead.buffer)?;

        match item
        {
            Some(item) =>
                {
                    if lookahead.buffer.position() == start
                    {
                        return Err(ParserError
                        {
                            location,
                            kind: ParserErrorKind::NoProgress
                        });
                    }

                    items.push(item);
                },

            None => return Err(ParserError
                {
                    location,
                    kind: ParserErrorKind::ExpectedExpression
                })
        }
    }
}
