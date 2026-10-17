
use super::*;



/**
 * Attempts to match the next token in the buffer against the expected token kind.
 * Returns `Ok(Some(token))` if the next token matches, `Ok(None)` if it doesn't, and an error
 * if there is an issue reading the next token.
 *
 * If the match fails then the buffer state is not advanced and the next token can be re-evaluated.
 */
pub(super) fn try_expect_token(buffer: &mut TokenBuffer<'_, '_>,
                               expected: TokenKind) -> CoreResult<Option<Token>>
{
    let mut lookahead = LookaheadGuard::new(buffer);
    let token = lookahead.buffer.next()?;

    if    let Some(token) = token
       && token.kind == expected
    {
        lookahead.commit();
        return Ok(Some(token));
    }

    Ok(None)
}


/**
 * Attempts to match the next token in the buffer against the expected token kind. If the match
 * fails then an error is returned indicating the unexpected token or end of input.
 */
pub(super) fn expect_token(buffer: &mut TokenBuffer<'_, '_>,
                           expected: TokenKind) -> CoreResult<Token>
{
    let token = buffer.next()?;

    if let Some(token) = token
    {
        if token.kind == expected
        {
            return Ok(token);
        }

        return Err(CoreError
            {
                location: Some(token.location),
                kind: CoreErrorKind::UnexpectedToken(expected, token.kind),
                message: None
            });
    }

    Err(CoreError
        {
            location: None,
            kind: CoreErrorKind::UnexpectedEOF(expected),
            message: None
        })
}


/**
 * Attempts to match the next token in the buffer against any of the expected token kinds.
 * Returns `Ok(Some(token))` if the next token matches one of the expected kinds, `Ok(None)` if it
 * doesn't, and an error if there is an issue reading the next token.
 *
 * If the match fails then the buffer state is not advanced and the next token can be re-evaluated.
 */
pub(super) fn try_expect_one_of_tokens(buffer: &mut TokenBuffer<'_, '_>,
                                       expected: &[TokenKind]) -> CoreResult<Option<Token>>
{
    let mut lookahead = LookaheadGuard::new(buffer);
    let token = lookahead.buffer.next()?;

    if    let Some(token) = token
       && expected.contains(&token.kind)
    {
        lookahead.commit();
        return Ok(Some(token));
    }

    Ok(None)
}


/**
 * Definition of a parser function type for passing to batch matchers.
 */
pub(super) type ParserFunction<T> = fn (buffer: &mut TokenBuffer<'_, '_>) -> CoreResult<Option<T>>;


/**
 * Attempts to match one of the provided parser functions against the input buffer. Order of
 * submission is important as this is a first match wins strategy. So make sure more specific
 * parsers are specified before more loosely matching ones.
 */
pub(super) fn match_one_of<T>(buffer: &mut TokenBuffer<'_, '_>,
                              parsers: &[ParserFunction<T>]) -> CoreResult<Option<T>>
{
    for parser in parsers
    {
        let mut lookahead = LookaheadGuard::new(buffer);

        match parser(&mut lookahead.buffer)
        {
            Ok(Some(value)) =>
                {
                    lookahead.commit();
                    return Ok(Some(value));
                },

            Ok(None) =>
                {
                    // Do nothing, just continue to the next parser.
                },

            Err(error) =>
                {
                    return Err(error);
                }
        }
    }

    return Ok(None);
}


#[derive(PartialEq, Eq, Clone, Copy)]
pub(super) enum EofOk
{
    No,
    Yes
}


/**
 * Run the provided parser repeatedly until one of the specified stop tokens is encountered.
 */
pub(super) fn parse_until_tokens<T>(location: &Location,
                                    buffer: &mut TokenBuffer<'_, '_>,
                                    eof_ok: EofOk,
                                    parser: ParserFunction<T>,
                                    stop_tokens: &[TokenKind]) -> CoreResult<Vec<T>>
{
    let mut results = vec![];

    loop
    {
        let token = try_expect_one_of_tokens(buffer, stop_tokens)?;

        if let Some(_) = token
        {
            break;
        }

        let result = parser(buffer)?;

        if let Some(result) = result
        {
            results.push(result);
        }
        else if eof_ok == EofOk::No
        {
            return Err(CoreError
                {
                    location: Some(location.clone()),
                    kind: CoreErrorKind::ExpectedExpressionOr(stop_tokens.to_vec()),
                    message: None
                });
        }
        else
        {
            break;
        }
    }

    Ok(results)
}


/**
 * Run the provided parser repeatedly until a statement break or line break token is encountered.
 */
pub(super) fn parse_until_break<T>(buffer: &mut TokenBuffer<'_, '_>,
                                   location: &Location,
                                   eof_ok: EofOk,
                                   parser: ParserFunction<T>) -> CoreResult<Vec<T>>
{
    parse_until_tokens(location,
                       buffer,
                       eof_ok,
                       parser,
                       &[TokenKind::StatementBreak, TokenKind::LineBreak])
}
