
use crate::language::{ ast::{ * },
                       data::value::Value,
                       tokenizer::{ TokenBuffer, TokenKind, TokenLiteral, TokenValue, StringFlag },
                       parser::{ base_utils::{ expect_token,
                                               match_one_of,
                                               Lookahead,
                                               try_expect_token,
                                               try_expect_one_of_tokens },
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
                        lookahead.commit();
                        return Ok(Some(new_ast_literal(token.location, Value::None, None)));
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
        AstExpressionKind::MathExpression(_, _, _) | AstExpressionKind::BooleanNot(_)
        | AstExpressionKind::IfExpression(_))
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

        if matches!(executable.kind, AstExpressionKind::Variable(_))
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

        let right = parse_scalar_expression(&mut *lookahead.buffer)?
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
    let Some(left) = parse_scalar_expression(buffer)? else { return Ok(None); };
    Ok(Some(parse_boolean_tail(buffer, left, 0)?))
}


/**
 * Parse a value with the command-call rules shared by assignments, returns, and parentheses.
 */
pub fn parse_value_expression(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstExpression>>
{
    parse_value_before_block(buffer, false)
}


pub fn parse_condition_expression(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstExpression>>
{
    parse_value_before_block(buffer, true)
}


fn parse_value_before_block(buffer: &mut TokenBuffer<'_, '_>,
                            stop_at_block: bool) -> ParseResult<Option<AstExpression>>
{
    let Some(expression) = parse_expression(buffer)? else { return Ok(None); };

    if matches!(&expression.kind, AstExpressionKind::Symbol(_) | AstExpressionKind::Variable(_))
    {
        let arguments = parse_arguments_before_block(buffer, stop_at_block)?;
        let location = expression.location.clone();

        if !arguments.is_empty()
        {
            return Ok(Some(AstExpression
                {
                    location: location.clone(),
                    kind: AstExpressionKind::Execute(Box::new(AstExecuteStatement
                        {
                            location,
                            executable_name: expression.resolve_as_text()?,
                            expand_path: matches!(&expression.kind,
                                AstExpressionKind::Symbol(symbol) if symbol.name.starts_with('~')),
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
    parse_arguments_before_block(buffer, false)
}


fn parse_arguments_before_block(buffer: &mut TokenBuffer<'_, '_>,
                                stop_at_block: bool) -> ParseResult<Vec<AstExpression>>
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

                    TokenKind::LineBreak | TokenKind::StatementBreak | TokenKind::BlockClose
                    | TokenKind::ParenClose => break,

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
    match_one_of(buffer, &[parse_variable_expression,
                           parse_symbol_expression,
                           parse_literal_expression,
                           parse_operator_to_symbol])
}
