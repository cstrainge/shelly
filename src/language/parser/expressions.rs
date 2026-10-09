
use crate::language::{ ast::{ * },
                       data::value::Value,
                       tokenizer::{ TokenBuffer, TokenKind, TokenLiteral, TokenValue, StringFlag },
                       parser::{ base_utils::{ expect_token,
                                               match_one_of,
                                               Lookahead,
                                               try_expect_token,
                                               try_expect_one_of_tokens },
                       results::{ ParseResult, ParserError, ParserErrorKind } } };



pub(super) fn expect_type_name(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<crate::language::tokenizer::Token>
{
    let token = expect_token(buffer, TokenKind::Symbol)?;
    let name = token.token_value_text();
    if !valid_type_name(&name)
    {
        return Err(ParserError { location: Some(token.location),
            kind: ParserErrorKind::InvalidEnum("Expected an identifier containing letters, digits, or underscores.".to_string()) });
    }
    Ok(token)
}

fn valid_type_name(name: &str) -> bool
{
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || c == '_')
}


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


/**
 * Parse one arithmetic operand. A mismatch consumes nothing. Parentheses accept
 * any value expression, including a command whose result is used as the operand.
 */
fn parse_math_primary(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstExpression>>
{
    let mut lookahead = Lookahead::new(buffer);
    let Some(token) = lookahead.buffer.next()? else { return Ok(None); };

    let expression = match token.kind
        {
            TokenKind::Symbol =>
                {
                    if try_expect_token(&mut *lookahead.buffer, TokenKind::Scope)?.is_none() { return Ok(None); }
                    let name = token.token_value_text();
                    if !valid_type_name(&name)
                    {
                        return Err(ParserError { location: Some(token.location),
                            kind: ParserErrorKind::InvalidEnum("Invalid enum type name.".to_string()) });
                    }
                    let variant = expect_type_name(&mut *lookahead.buffer)?;
                    let peek = Lookahead::new(&mut *lookahead.buffer);
                    if let Some(next) = peek.buffer.next()?
                    {
                        if next.kind == TokenKind::Scope
                        {
                            return Err(ParserError { location: Some(next.location),
                                kind: ParserErrorKind::InvalidEnum("Enum references require exactly Type::Variant.".to_string()) });
                        }
                        if next.kind == TokenKind::ParenOpen
                            && next.location.line == variant.location.line
                            && next.location.column == variant.location.column + variant.token_value_text().chars().count()
                        {
                            return Err(ParserError { location: Some(next.location),
                                kind: ParserErrorKind::InvalidEnum("Unit enum variants do not take constructor arguments.".to_string()) });
                        }
                    }
                    AstExpression { location: token.location,
                        kind: AstExpressionKind::EnumVariant(name, variant.token_value_text()), string_flag: None }
                },

            TokenKind::SquareOpen => AstExpression
                {
                    location: token.location,
                    kind: parse_collection(&mut *lookahead.buffer)?,
                    string_flag: None
                },

            TokenKind::If => super::statements::parse_if_expression(
                &mut *lookahead.buffer, token.location)?,

            TokenKind::Literal =>
                {
                    let mut string_flag = None;

                    let value = match token.value
                        {
                            TokenValue::Literal(TokenLiteral::Integer(value, _)) =>
                                {
                                    Value::Integer(value)
                                },

                            TokenValue::Literal(TokenLiteral::Float(value, text)) =>
                                {
                                    Value::Float(value, Some(text))
                                },

                            TokenValue::Literal(TokenLiteral::Boolean(value)) => Value::Boolean(value),

                            TokenValue::Literal(TokenLiteral::String(value, flag, escaped_dollars)) =>
                                {
                                    string_flag = Some(match flag
                                        {
                                            StringFlag::Interpolated =>
                                                AstStringFlag::Interpolated(escaped_dollars),

                                            StringFlag::NonInterpolated =>
                                                AstStringFlag::NonInterpolated,
                                        });

                                    Value::from_string(value)
                                },

                            _ => return Ok(None)
                        };

                    new_ast_literal(token.location, value, string_flag)
                },

            TokenKind::Not =>
                {
                    let operand = match parse_math_primary(&mut *lookahead.buffer)?
                        {
                            Some(operand) => operand,
                            None => parse_scalar_expression(&mut *lookahead.buffer)?
                                .ok_or_else(|| ParserError
                                    {
                                        location: Some(token.location.clone()),
                                        kind: ParserErrorKind::ExpectedExpression
                                    })?
                        };
                    AstExpression
                        {
                            location: token.location,
                            kind: AstExpressionKind::BooleanNot(Box::new(operand)),
                            string_flag: None
                        }
                },

            TokenKind::Minus =>
                {
                    // A standalone '-' remains a command argument. With an operand it is unary
                    // negation, expressed as subtraction so normal checked arithmetic applies.
                    let Some(operand) = parse_math_primary(&mut *lookahead.buffer)? else
                    {
                        return Ok(None);
                    };
                    AstExpression
                        {
                            location: token.location.clone(),
                            kind: AstExpressionKind::MathExpression(AstMathOperator::Subtract,
                                Box::new(new_ast_literal(token.location, Value::Integer(0), None)),
                                Box::new(operand)),
                            string_flag: None
                        }
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
                    if try_expect_token(&mut *lookahead.buffer, TokenKind::ParenClose)?.is_some()
                    {
                        let expression = parse_postfix(&mut *lookahead.buffer,
                            new_ast_literal(token.location, Value::None, None))?;
                        lookahead.commit();
                        return Ok(Some(expression));
                    }

                    let expression = parse_value_expression(&mut *lookahead.buffer)?
                        .ok_or_else(|| ParserError
                            {
                                location: Some(token.location.clone()),
                                kind: ParserErrorKind::ExpectedExpression
                            })?;
                    expect_token(&mut *lookahead.buffer, TokenKind::ParenClose)?;
                    AstExpression
                        {
                            location: token.location,
                            kind: AstExpressionKind::Grouped(Box::new(expression)),
                            string_flag: None
                        }
                },

            _ => return Ok(None)
        };

    let expression = parse_postfix(&mut *lookahead.buffer, expression)?;
    lookahead.commit();
    Ok(Some(expression))
}


fn skip_array_newlines(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<()>
{
    while try_expect_token(buffer, TokenKind::LineBreak)?.is_some() {}
    Ok(())
}


pub fn parse_indexes(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Vec<AstExpression>>
{
    let mut indexes = Vec::new();
    while let Some(open) = try_expect_token(buffer, TokenKind::IndexOpen)?
    {
        skip_array_newlines(buffer)?;
        let index = parse_value_expression(buffer)?.ok_or_else(|| ParserError
            {
                location: Some(open.location),
                kind: ParserErrorKind::ExpectedExpression
            })?;
        skip_array_newlines(buffer)?;
        expect_token(buffer, TokenKind::SquareClose)?;
        indexes.push(index);
    }
    Ok(indexes)
}


fn parse_postfix(buffer: &mut TokenBuffer<'_, '_>,
                 mut expression: AstExpression) -> ParseResult<AstExpression>
{
    for index in parse_indexes(buffer)?
    {
        expression = AstExpression
            {
                location: expression.location.clone(),
                kind: AstExpressionKind::Index(Box::new(expression), Box::new(index)),
                string_flag: None
            };
    }
    if let Some(splat) = try_expect_token(buffer, TokenKind::Splat)?
    {
        expression = AstExpression
            {
                location: splat.location,
                kind: AstExpressionKind::Splat(Box::new(expression)),
                string_flag: None
            };
    }
    Ok(expression)
}


fn parse_collection_value(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<AstExpression>
{
    parse_value_before_block(buffer, false, true)?.ok_or_else(|| ParserError
        {
            location: None,
            kind: ParserErrorKind::ExpectedExpression
        })
}


fn parse_collection(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<AstExpressionKind>
{
    skip_array_newlines(buffer)?;
    if try_expect_token(buffer, TokenKind::SquareClose)?.is_some()
    {
        return Ok(AstExpressionKind::Array(Vec::new()));
    }
    // [] remains an array; [:] denotes an empty map.
    if try_expect_token(buffer, TokenKind::TypeDelimiter)?.is_some()
    {
        skip_array_newlines(buffer)?;
        expect_token(buffer, TokenKind::SquareClose)?;
        return Ok(AstExpressionKind::HashMap(Vec::new()));
    }

    let first = parse_collection_value(buffer)?;
    skip_array_newlines(buffer)?;
    let is_map = try_expect_token(buffer, TokenKind::TypeDelimiter)?.is_some();
    let mut elements = Vec::new();
    let mut pairs = Vec::new();
    let mut item = first;
    loop
    {
        if is_map
        {
            skip_array_newlines(buffer)?;
            pairs.push((item, parse_collection_value(buffer)?));
        }
        else
        {
            elements.push(item);
        }
        skip_array_newlines(buffer)?;
        if try_expect_token(buffer, TokenKind::SquareClose)?.is_some() { break; }
        expect_token(buffer, TokenKind::Comma)?;
        skip_array_newlines(buffer)?;
        if try_expect_token(buffer, TokenKind::SquareClose)?.is_some() { break; }
        item = parse_collection_value(buffer)?;
        if is_map
        {
            skip_array_newlines(buffer)?;
            expect_token(buffer, TokenKind::TypeDelimiter)?;
        }
    }
    Ok(if is_map { AstExpressionKind::HashMap(pairs) } else { AstExpressionKind::Array(elements) })
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
                kind: AstExpressionKind::MathExpression(operator, Box::new(left), Box::new(right)),
                string_flag: None
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
    if starts_with_group || matches!(&expression.kind,
        AstExpressionKind::EnumVariant(_, _) | AstExpressionKind::MathExpression(_, _, _) | AstExpressionKind::BooleanNot(_)
        | AstExpressionKind::IfExpression(_) | AstExpressionKind::Array(_)
        | AstExpressionKind::HashMap(_)
        | AstExpressionKind::Index(_, _) | AstExpressionKind::Splat(_))
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
        Ok(Some(parse_postfix(buffer,
            new_ast_variable(identifier.location.clone(), identifier_value))?))
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

    // Does the symbol have any $s in it?
    let string_flag = if symbol_value.contains('$')
        {
            Some(AstStringFlag::Interpolated(Vec::new()))
        }
        else
        {
            Some(AstStringFlag::NonInterpolated)
        };

    Ok(Some(new_ast_symbol(symbol.location.clone(), symbol_value, string_flag)))
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

    let mut string_flag = None;

    let literal_value = match literal_value
        {
            TokenLiteral::Integer(value, _) => Value::Integer(value),
            TokenLiteral::Float(value, text) => Value::Float(value, Some(text)),
            TokenLiteral::Boolean(value) => Value::Boolean(value),
            TokenLiteral::String(value, flag, escaped_dollars) =>
                {
                    string_flag = match flag
                        {
                            StringFlag::Interpolated => Some(AstStringFlag::Interpolated(escaped_dollars)),
                            StringFlag::NonInterpolated => Some(AstStringFlag::NonInterpolated),
                        };
                    Value::from_string(value)
                }
        };

    Ok(Some(new_ast_literal(literal.location.clone(), literal_value, string_flag)))
}


fn parse_lonely_glob_expression(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstExpression>>
{
    let glob_operator = try_expect_token(buffer, TokenKind::Glob)?;

    if let Some(glob_operator) = glob_operator
    {
        let location = glob_operator.location.clone();
        return Ok(Some(new_ast_symbol(location, "*".to_string(), None)));
    }

    let star = expect_token(buffer, TokenKind::Asterisk)?;

    Ok(Some(new_ast_symbol(star.location.clone(), "*".to_string(), None)))
}


fn parse_operator_to_symbol(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstExpression>>
{
    let allowed_operators = [ TokenKind::Let,
                              TokenKind::Export,
                              TokenKind::Alias,
                              TokenKind::Sub,
                              TokenKind::If,
                              TokenKind::Else,
                              TokenKind::While,
                              TokenKind::Until,
                              TokenKind::For,
                              TokenKind::In,
                              TokenKind::Break,
                              TokenKind::Continue,
                              TokenKind::Loop,
                              TokenKind::Match,
                              TokenKind::Return,
                              TokenKind::Function,
                              TokenKind::Struct,
                              TokenKind::Enum,
                              TokenKind::Import,
                              TokenKind::TypeDelimiter,
                              TokenKind::Assign,
                              TokenKind::Minus,
                              TokenKind::Plus,
                              TokenKind::Slash,
                              TokenKind::Percent,
                              TokenKind::Scope,
                              TokenKind::ErrorSource ];

    // Attempt to match one of the allowed operators
    if let Some(operator) = try_expect_one_of_tokens(buffer, &allowed_operators)?
    {
        let location = operator.location.clone();

        return Ok(Some(new_ast_symbol(location, operator.token_value_text(), None)));
    }

    Ok(None)
}


fn parse_scalar_expression(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstExpression>>
{
    // An executable reference currently stores only its name. Use a literal so that value
    // parsing does not turn the name into a call or apply symbol expansion.
    if let Some(escape) = try_expect_token(buffer, TokenKind::ExecEscape)?
    {
        let executable = parse_exec_expression(buffer)?.ok_or_else(|| ParserError
            {
                location: Some(escape.location.clone()),
                kind: ParserErrorKind::ExpectedExpression
            })?;

        if matches!(executable.kind, AstExpressionKind::Variable(_) | AstExpressionKind::Index(_, _))
        {
            return Ok(Some(AstExpression
                {
                    location: escape.location,
                    kind: AstExpressionKind::ExecutableReference(Box::new(executable)),
                    string_flag: None
                }));
        }

        return Ok(Some(new_ast_literal(escape.location,
                                       Value::from_executable_string(executable.resolve_as_text()?),
                                       None)));
    }

    // The math rule itself decides when to fall back. Once it reports malformed
    // arithmetic, do not retry it as separate literal/variable/glob arguments.
    if let Some(expression) = parse_math_expression(buffer)?
    {
        return Ok(Some(expression));
    }

    match_one_of(buffer, &[parse_variable_expression,
                           parse_symbol_expression,
                           parse_literal_expression,
                           parse_lonely_glob_expression,
                           parse_operator_to_symbol])
}


fn parse_boolean_tail(buffer: &mut TokenBuffer<'_, '_>,
                      mut left: AstExpression,
                      min_precedence: u8) -> ParseResult<AstExpression>
{
    loop
    {
        let mut lookahead = Lookahead::new(buffer);
        let Some(token) = lookahead.buffer.next()? else { break; };
        let (operator, precedence) = match token.kind
            {
                TokenKind::Or => (AstBooleanOperator::Or, 1),
                TokenKind::And => (AstBooleanOperator::And, 2),
                TokenKind::Equal => (AstBooleanOperator::Equal, 3),
                TokenKind::NotEqual => (AstBooleanOperator::NotEqual, 3),
                _ => break
            };
        if precedence < min_precedence { break; }

        let right = parse_range_expression(&mut *lookahead.buffer)?
            .ok_or_else(|| ParserError
                {
                    location: Some(token.location.clone()),
                    kind: ParserErrorKind::ExpectedExpression
                })?;
        let right = parse_boolean_tail(&mut *lookahead.buffer, right, precedence + 1)?;
        left = AstExpression
            {
                location: token.location,
                kind: AstExpressionKind::BooleanExpression(operator, Box::new(left), Box::new(right)),
                string_flag: None
            };
        lookahead.commit();
    }
    Ok(left)
}


pub fn parse_expression(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstExpression>>
{
    let Some(left) = parse_range_expression(buffer)? else { return Ok(None); };
    Ok(Some(parse_boolean_tail(buffer, left, 0)?))
}


// Arithmetic binds inside each bound; comparisons and logical operators combine
// complete ranges. A second range operator must be explicitly parenthesized.
fn parse_range_expression(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstExpression>>
{
    let leading = try_expect_one_of_tokens(buffer, &[TokenKind::Range, TokenKind::RangeInclusive])?;
    let start = if leading.is_none() { parse_scalar_expression(buffer)? } else { None };
    // A bare command name is not a range bound: `echo ..5` and `cd ..`
    // must leave the range for argument parsing.
    if start.as_ref().is_some_and(|value| matches!(value.kind, AstExpressionKind::Symbol(_)))
    {
        return Ok(start);
    }
    let operator = match leading
        {
            Some(operator) => Some(operator),
            None => try_expect_one_of_tokens(buffer, &[TokenKind::Range, TokenKind::RangeInclusive])?
        };
    let Some(operator) = operator else { return Ok(start); };
    let inclusive = operator.kind == TokenKind::RangeInclusive;
    let at_end =
        {
            let lookahead = Lookahead::new(buffer);
            lookahead.buffer.next()?.is_none_or(|token| matches!(token.kind,
                TokenKind::LineBreak | TokenKind::StatementBreak | TokenKind::BlockOpen
                | TokenKind::BlockClose | TokenKind::ParenClose | TokenKind::SquareClose
                | TokenKind::Comma | TokenKind::TypeDelimiter | TokenKind::Equal
                | TokenKind::NotEqual | TokenKind::And | TokenKind::Or))
        };
    let end = if at_end { None } else { parse_scalar_expression(buffer)? };
    if (inclusive && end.is_none())
        || try_expect_one_of_tokens(buffer, &[TokenKind::Range, TokenKind::RangeInclusive])?.is_some()
    {
        return Err(ParserError { location: Some(operator.location), kind: ParserErrorKind::InvalidRange });
    }
    if !at_end && end.is_none()
    {
        return Err(ParserError { location: Some(operator.location), kind: ParserErrorKind::ExpectedExpression });
    }
    Ok(Some(AstExpression
        {
            location: operator.location,
            kind: AstExpressionKind::Range(start.map(Box::new), end.map(Box::new), inclusive),
            string_flag: None
        }))
}


/**
 * Parse a value with the command-call rules shared by assignments, returns, and parentheses.
 */
pub fn parse_value_expression(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstExpression>>
{
    parse_value_before_block(buffer, false, false)
}


pub fn parse_condition_expression(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstExpression>>
{
    parse_value_before_block(buffer, true, false)
}


fn parse_value_before_block(buffer: &mut TokenBuffer<'_, '_>,
                            stop_at_block: bool,
                            stop_at_colon: bool) -> ParseResult<Option<AstExpression>>
{
    let Some(expression) = parse_expression(buffer)? else { return Ok(None); };

    if matches!(&expression.kind, AstExpressionKind::Symbol(_) | AstExpressionKind::Variable(_)
        | AstExpressionKind::Index(_, _))
    {
        reject_index_assignment(buffer, &expression)?;
        let arguments = parse_arguments_before_block(buffer, stop_at_block, stop_at_colon)?;
        let location = expression.location.clone();

        if !arguments.is_empty()
        {
            return Ok(Some(AstExpression
                {
                    location: location.clone(),
                    kind: AstExpressionKind::Execute(Box::new(AstExecuteStatement
                        {
                            location,
                            expand_path: matches!(&expression.kind,
                                AstExpressionKind::Symbol(symbol) if symbol.name.starts_with('~')),
                            executable: expression,
                            arguments
                        })),
                    string_flag: None
                }));
        }

        // Globs remain collection values. Only a lone command word is ambiguous.
        if matches!(&expression.kind, AstExpressionKind::Symbol(symbol) if !symbol.is_glob())
        {
            return Ok(Some(AstExpression
                {
                    location,
                    kind: AstExpressionKind::TryExecute(Box::new(expression)),
                    string_flag: None
                }));
        }
    }

    Ok(Some(expression))
}


pub fn parse_command_arguments(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Vec<AstExpression>>
{
    parse_arguments_before_block(buffer, false, false)
}


fn parse_arguments_before_block(buffer: &mut TokenBuffer<'_, '_>,
                                stop_at_block: bool,
                                stop_at_colon: bool) -> ParseResult<Vec<AstExpression>>
{
    let mut parameter_expressions = Vec::new();

    loop
    {
        let next =
            {
                let mut lookahead = Lookahead::new(buffer);
                let Some(token) = lookahead.buffer.next()? else { break; };

                match token.kind
                {
                    TokenKind::BlockOpen if stop_at_block => break,
                    TokenKind::TypeDelimiter if stop_at_colon => break,

                    TokenKind::LineBreak | TokenKind::StatementBreak | TokenKind::BlockClose
                    | TokenKind::ParenClose | TokenKind::SquareClose | TokenKind::Comma => break,

                    TokenKind::LineContinue =>
                        {
                            expect_token(lookahead.buffer, TokenKind::LineBreak)?;
                            lookahead.commit();
                            continue;
                        },

                    _ => token
                }
            };

        // The lookahead rewound this token so the full expression can consume it.
        let expression = parse_expression(buffer)?.ok_or_else(|| ParserError
            {
                location: Some(next.location),
                kind: ParserErrorKind::ExpectedExpression
            })?;

        parameter_expressions.push(expression);
    }

    Ok(parameter_expressions)
}


pub fn parse_exec_expression(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstExpression>>
{
    {
        let mut lookahead = Lookahead::new(buffer);
        if let Some(expression) = parse_math_expression(&mut *lookahead.buffer)?
            && matches!(expression.kind, AstExpressionKind::Index(_, _) | AstExpressionKind::EnumVariant(_, _))
        {
            reject_index_assignment(&mut *lookahead.buffer, &expression)?;
            lookahead.commit();
            return Ok(Some(expression));
        }
    }
    let expression = match_one_of(buffer, &[parse_variable_expression,
                           parse_symbol_expression,
                           parse_literal_expression,
                           parse_operator_to_symbol])?;

    if let Some(expression) = &expression
        && matches!(expression.kind, AstExpressionKind::VariableSplat(_) | AstExpressionKind::Splat(_))
    {
        return Err(ParserError
            {
                location: Some(expression.location.clone()),
                kind: ParserErrorKind::SplatExecutable
            });
    }

    Ok(expression)
}


// Valid indexed assignments are consumed by the statement parser. Do not turn
// an invalid target or an assignment inside an expression into a command call.
fn reject_index_assignment(buffer: &mut TokenBuffer<'_, '_>,
                            expression: &AstExpression) -> ParseResult<()>
{
    if matches!(expression.kind, AstExpressionKind::Index(_, _))
        && let Some(assign) = try_expect_token(buffer, TokenKind::Assign)?
    {
        return Err(ParserError
            {
                location: Some(assign.location),
                kind: ParserErrorKind::InvalidAssignmentTarget
            });
    }
    Ok(())
}
