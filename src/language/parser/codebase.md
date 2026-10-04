## expressions.rs

```rust

use crate::language::{ ast::{ * },
                       data::value::Value,
                       tokenizer::{ TokenBuffer, TokenKind, TokenLiteral, TokenValue },
                       parser::{ base_utils::{ expect_token, match_one_of },
                       results::{ ParseResult, ParserError, ParserErrorKind } } };



fn parse_variable_expression(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstExpression>>
{
    let identifier = expect_token(buffer, TokenKind::Identifier)?;

    let identifier_value = match identifier.value
        {
            TokenValue::Identifier(identifier) => identifier,

            _ => return Err(ParserError
                {
                    location: Some(identifier.location.clone()),
                    kind: ParserErrorKind::ExpectedExpression,
                })
        };

    Ok(Some(new_ast_variable(identifier.location.clone(), identifier_value)))
}


fn parse_symbol_expression(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstExpression>>
{
    let symbol = expect_token(buffer, TokenKind::Symbol)?;

    let symbol_value = match symbol.value
        {
            TokenValue::Symbol(symbol) => symbol,

            _ => return Err(ParserError
                {
                    location: Some(symbol.location.clone()),
                    kind: ParserErrorKind::ExpectedExpression,
                })
        };

    Ok(Some(new_ast_symbol(symbol.location.clone(), symbol_value)))
}


fn parse_literal_expression(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstExpression>>
{
    let literal = expect_token(buffer, TokenKind::Literal)?;

    let literal_value = match literal.value
        {
            TokenValue::Literal(literal) => literal,

            _ => return Err(ParserError
                {
                    location: Some(literal.location.clone()),
                    kind: ParserErrorKind::ExpectedExpression,
                })
        };

    let literal_value = match literal_value
        {
            TokenLiteral::Integer(value, _) => Value::Integer(value),
            TokenLiteral::Float(value, text) => Value::Float(value, Some(text)),
            TokenLiteral::String(value) => Value::String(value)
        };

    Ok(Some(new_ast_literal(literal.location.clone(), literal_value)))
}


pub fn parse_expression(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstExpression>>
{
    match_one_of(buffer, &[parse_variable_expression,
                           parse_symbol_expression,
                           parse_literal_expression])
}


pub fn parse_exec_expression(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstExpression>>
{
    match_one_of(buffer, &[parse_variable_expression,
                           parse_symbol_expression,
                           parse_literal_expression])
}

```

## statements.rs

```rust

use crate::language::{ ast::{ * },
                       tokenizer::{ TokenBuffer, TokenKind },
                       parser::{ base_utils::{ match_one_of,
                                               expect_token,
                                               try_expect_token,
                                               match_multiple_of },
                                 expressions::{ parse_expression, parse_exec_expression },
                                 results::{ ParseResult, ParserError, ParserErrorKind } } };



fn parse_let_statement(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstStatement>>
{
    // let $var = <expression>

    expect_token(buffer, TokenKind::Let)?;

    let identifier = expect_token(buffer, TokenKind::Identifier)?;

    expect_token(buffer, TokenKind::Assign)?;

    let expression = parse_expression(buffer)?;

    if let Some(expression) = expression
    {
        Ok(new_ast_let_statement(identifier.location.clone(),
                                identifier.token_value_text(),
                                expression))
    }
    else
    {
        Err(ParserError
            {
                location: Some(identifier.location.clone()),
                kind: ParserErrorKind::ExpectedExpression
            })
    }
}


fn parse_execute_statement(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstStatement>>
{
    let exec_expression = parse_exec_expression(buffer)?;

    if let None = &exec_expression
    {
        return Ok(None);
    }

    let exec_expression = exec_expression.unwrap();
    let location = exec_expression.location.clone();

    let parameter_expressions = match_multiple_of(buffer,
                                                  &[parse_expression],
                                                  &[TokenKind::LineBreak,
                                                    TokenKind::StatementBreak])?;

    Ok(new_ast_execute_statement(location,
                                 exec_expression.resolve_as_text()?,
                                 parameter_expressions))
}


fn parse_null_statement(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstStatement>>
{
    if let None = try_expect_token(buffer, TokenKind::LineBreak)?
    {
        if let None = try_expect_token(buffer, TokenKind::StatementBreak)?
        {
            return Ok(None);
        }
    }

    Ok(Some(AstStatement::NullStatement))
}


pub fn parse_statement(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstStatement>>
{
    match_one_of(buffer, &[parse_null_statement,
                           parse_let_statement,
                           parse_execute_statement])
}

```

## mod.rs

```rust

use crate::language::{ ast::AstTopLevel, tokenizer::{ TokenBuffer, Tokenizer } };


mod results;
mod base_utils;
mod expressions;
mod statements;

pub use results::ParserError;
use statements::parse_statement;



pub type ParserResult<T> = results::ParseResult<T>;



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

```

## base_utils.rs

```rust

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

```

## results.rs

```rust

use std::fmt::{ self, Display, Formatter, Debug };

use crate::language::{ text::location::Location,
                       tokenizer::{ TokenKind, TokenizerError },
                       ast::AstError };



pub enum ParserErrorKind
{
    TokenizerError(TokenizerError),
    UnexpectedEOF(TokenKind),
    MatchError(Vec<ParserError>),
    ExpectedToken(TokenKind, Option<TokenKind>),
    ExpectedExpression,
    ExpressionNotString,
    NoProgress,
}


impl Display for ParserErrorKind
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
    {
        match self
        {
            ParserErrorKind::TokenizerError(error) => write!(f, "Tokenizer error: {}.", error),

            ParserErrorKind::UnexpectedEOF(kind) =>
                {
                    write!(f, "Unexpected end of file, expected token: {}.", kind)
                },

            ParserErrorKind::MatchError(errors) => write!(f, "Match error: {:?}.", errors),
            ParserErrorKind::ExpectedToken(expected, found) =>
                {
                    match found
                    {
                        Some(found) => write!(f, "Expected token: {}, but found: {}.", expected, found),
                        None => write!(f, "Expected token: {}, but found end of file.", expected)
                    }
                }

            ParserErrorKind::ExpectedExpression =>
                {
                    write!(f, "Expected expression.")
                },

            ParserErrorKind::ExpressionNotString =>
                {
                    write!(f, "Expression does not resolve to a string.")
                },

            ParserErrorKind::NoProgress =>
                write!(f, "Repeated parser succeeded without consuming a token.")
        }
    }
}


pub struct ParserError
{
    pub location: Option<Location>,
    pub kind: ParserErrorKind
}


pub type ParseResult<T> = Result<T, ParserError>;


impl From<TokenizerError> for ParserError
{
    fn from(error: TokenizerError) -> Self
    {
        ParserError
            {
                location: None,
                kind: ParserErrorKind::TokenizerError(error),
            }
    }
}


impl From<AstError> for ParserError
{
    fn from(error: AstError) -> Self
    {
        match error
        {
            AstError::ExpressionNotString(location) =>
                ParserError
                    {
                        location: Some(location),
                        kind: ParserErrorKind::ExpressionNotString
                    }
        }
    }
}


impl Debug for ParserError
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
    {
        if let Some(location) = &self.location
        {
            write!(f, "Parser error: {}: {}", location, self.kind)
        }
        else
        {
            write!(f, "Parser error: {}", self.kind)
        }
    }
}

```

