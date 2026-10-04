
use crate::language::{ ast::{ * },
                       data::value::Value,
                       tokenizer::{ TokenBuffer, TokenKind, TokenLiteral, TokenValue },
                       parser::{ base_utils::{ expect_token, match_one_of, Lookahead, try_expect_token },
                       results::{ ParseResult, ParserError, ParserErrorKind } } };



fn math_operator(kind: TokenKind) -> Option<(AstMathOperator, u8)>
{
    match kind
    {
        TokenKind::Plus     => Some((AstMathOperator::Add,      1)),
        TokenKind::Minus    => Some((AstMathOperator::Subtract, 1)),
        TokenKind::Asterisk => Some((AstMathOperator::Multiply, 2)),
        TokenKind::Slash    => Some((AstMathOperator::Divide,   2)),
        TokenKind::Percent  => Some((AstMathOperator::Modulo,   2)),
        _                   => None
    }
}


fn parse_math_binary_expression(buffer: &mut TokenBuffer<'_, '_>,
                                min_precedence: u8) -> ParseResult<AstExpression>
{
    let location =
        {
            let peek = Lookahead::new(buffer);
            peek.buffer.next()?.map(|token| token.location)
        };

    let left = parse_math_primary(buffer)?.ok_or(ParserError
        {
            location,
            kind: ParserErrorKind::ExpectedExpression
        })?;

    parse_math_binary_tail(buffer, left, min_precedence, false)
}


/**
 * Parse one arithmetic operand. A mismatch consumes nothing. Parentheses only
 * group math here; general command grouping is left to a future grammar rule.
 */
fn parse_math_primary(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstExpression>>
{
    let mut lookahead = Lookahead::new(buffer);
    let Some(token) = lookahead.buffer.next()? else { return Ok(None); };

    let expression = match token.kind
        {
            TokenKind::Literal =>
                {
                    let value = match token.value
                        {
                            TokenValue::Literal(TokenLiteral::Integer(value, _)) =>
                                {
                                    Value::Integer(value)
                                },

                            TokenValue::Literal(TokenLiteral::Float(value, text)) =>
                                {
                                    Value::Float(value, Some(text))
                                }

                            _ => return Ok(None)
                        };

                    new_ast_literal(token.location, value)
                },

            TokenKind::Identifier =>
                {
                    let TokenValue::Identifier(name) = token.value else { return Ok(None); };

                    // Argument expansion is not a scalar arithmetic operand.
                    if try_expect_token(&mut *lookahead.buffer, TokenKind::Splat)?.is_some()
                    {
                        return Ok(None);
                    }

                    new_ast_variable(token.location, name)
                },

            TokenKind::ParenOpen =>
                {
                    let expression = parse_math_binary_expression(&mut *lookahead.buffer, 0)?;
                    expect_token(&mut *lookahead.buffer, TokenKind::ParenClose)?;
                    expression
                },

            _ => return Ok(None)
        };

    lookahead.commit();
    Ok(Some(expression))
}


/**
 * Extend a left operand using precedence climbing. The higher right-hand
 * threshold makes equal-precedence operators left-associative.
 */
fn parse_math_binary_tail(buffer: &mut TokenBuffer<'_, '_>,
                          mut left: AstExpression,
                          min_precedence: u8,
                          allow_glob_fallback: bool) -> ParseResult<AstExpression>
{
    let mut matched_operator = false;

    loop
    {
        // Keep the operator speculative until its right operand has parsed.
        // EOF, statement terminators, closing parentheses, **, and other
        // non-operators remain unread for the enclosing parser.
        let mut lookahead = Lookahead::new(buffer);
        let Some(token) = lookahead.buffer.next()? else { break; };

        if matches!(token.kind, TokenKind::LineBreak | TokenKind::StatementBreak | TokenKind::ParenClose)
        {
            break;
        }

        let Some((operator, precedence)) = math_operator(token.kind) else { break; };

        if precedence < min_precedence
        {
            break;
        }

        let right = match parse_math_primary(&mut *lookahead.buffer)?
            {
                Some(right) => right,
                None =>
                    {
                        if    allow_glob_fallback
                           && !matched_operator
                           && matches!(operator, AstMathOperator::Multiply)
                        {
                            break;
                        }

                        return Err(ParserError
                            {
                                location: Some(token.location),
                                kind: ParserErrorKind::ExpectedExpression
                            });
                    }
            };

        let right = parse_math_binary_tail(&mut *lookahead.buffer, right, precedence + 1, false)?;

        left = AstExpression
            {
                location: left.location.clone(),
                kind: AstExpressionKind::MathExpression(operator, Box::new(left), Box::new(right))
            };

        lookahead.commit();
        matched_operator = true;
    }

    Ok(left)
}


fn parse_math_expression(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstExpression>>
{
    let mut lookahead = Lookahead::new(buffer);
    let starts_with_group =
        {
            let peek = Lookahead::new(&mut *lookahead.buffer);
            matches!(peek.buffer.next()?, Some(token) if token.kind == TokenKind::ParenOpen)
        };

    let Some(left) = parse_math_primary(&mut *lookahead.buffer)? else { return Ok(None); };
    let expression = parse_math_binary_tail(&mut *lookahead.buffer, left, 0, !starts_with_group)?;

    // A lone number/variable belongs to the existing expression rules. Explicit
    // parentheses also accept a single operand, e.g. (42) or ($count).
    if starts_with_group || matches!(&expression.kind, AstExpressionKind::MathExpression(_, _, _))
    {
        lookahead.commit();
        return Ok(Some(expression));
    }

    Ok(None)
}


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

    // Check for the splat operator, `...`, which may follow the variable.
    let splat_operator = try_expect_token(buffer, TokenKind::Splat)?;

    if let Some(_) = splat_operator
    {
        Ok(Some(new_ast_variable_splat(identifier.location.clone(), identifier_value)))
    }
    else
    {
        Ok(Some(new_ast_variable(identifier.location.clone(), identifier_value)))
    }
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


fn parse_lonely_glob_expression(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstExpression>>
{
    let glob_operator = try_expect_token(buffer, TokenKind::Glob)?;

    if let Some(glob_operator) = glob_operator
    {
        let location = glob_operator.location.clone();
        return Ok(Some(new_ast_symbol(location, "*".to_string())));
    }

    let star = expect_token(buffer, TokenKind::Asterisk)?;

    Ok(Some(new_ast_symbol(star.location.clone(), "*".to_string())))
}


pub fn parse_expression(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstExpression>>
{
    // The math rule itself decides when to fall back. Once it reports malformed
    // arithmetic, do not retry it as separate literal/variable/glob arguments.
    if let Some(expression) = parse_math_expression(buffer)?
    {
        return Ok(Some(expression));
    }

    match_one_of(buffer, &[parse_variable_expression,
                           parse_symbol_expression,
                           parse_literal_expression,
                           parse_lonely_glob_expression])
}


pub fn parse_exec_expression(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstExpression>>
{
    match_one_of(buffer, &[parse_variable_expression,
                           parse_symbol_expression,
                           parse_literal_expression])
}
