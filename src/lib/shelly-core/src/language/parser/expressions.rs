
use super::*;



pub(super) enum CallContext
{
    Statement,
    Grouped
}


pub(super) fn parse_call(input: &mut TokenBuffer<'_, '_>,
                         context: CallContext) -> CoreResult<Option<CallExpression>>
{
    let name = try_expect_one_of_tokens(input, &[TokenKind::Identifier, TokenKind::Symbol])?;

    if let Some(name) = name
    {
        let arguments = match context
            {
                CallContext::Statement =>
                    {
                        parse_until_break(input, &name.location, EofOk::Yes, parse_expression)?
                    },

                CallContext::Grouped =>
                    {
                        parse_until_tokens(&name.location, input, EofOk::No,
                                           parse_expression, &[TokenKind::ParenClose])?
                    }
            };

        return Ok(Some(CallExpression
            {
                location: name.location.clone(),
                identifier: name.as_text(),
                arguments
            }));
    }

    Ok(None)
}


fn parse_call_expression(input: &mut TokenBuffer<'_, '_>) -> CoreResult<Option<Expression>>
{
    if try_expect_token(input, TokenKind::ParenOpen)?.is_none()
    {
        return Ok(None);
    }

    if let Some(call) = parse_call(input, CallContext::Grouped)?
    {
        return Ok(Some(Expression::new_call_expression(call.location,
                                                       call.identifier,
                                                       call.arguments)));
    }

    Ok(None)
}


fn parse_grouped_expression(input: &mut TokenBuffer<'_, '_>) -> CoreResult<Option<Expression>>
{
    let open = try_expect_token(input, TokenKind::ParenOpen)?;

    if let Some(open) = open
    {
        let expression = parse_expression(input)?;

        expect_token(input, TokenKind::ParenClose)?;

        if let None = expression
        {
            Ok(Some(Expression::new_literal_none(open.location)))
        }
        else
        {
            Ok(expression)
        }
    }
    else
    {
        Ok(None)
    }
}


fn parse_literal_expression(input: &mut TokenBuffer<'_, '_>) -> CoreResult<Option<Expression>>
{
    if let Some(token) = try_expect_token(input, TokenKind::Literal)?
    {
        let value = match token.value
            {
                TokenValue::Literal(literal) => literal,
                _                            => return Ok(None)
            };

        return Ok(Some(Expression::new_literal_expression(token.location, value)));
    }

    Ok(None)
}


fn math_binding_power(kind: &TokenKind) -> Option<(u8, u8, MathOperation)>
{
    match kind
    {
        TokenKind::Plus     => Some((1, 2, MathOperation::Addition)),
        TokenKind::Minus    => Some((1, 2, MathOperation::Subtraction)),
        TokenKind::Asterisk => Some((3, 4, MathOperation::Multiplication)),
        TokenKind::Slash    => Some((3, 4, MathOperation::Division)),
        TokenKind::Percent  => Some((3, 4, MathOperation::Modulus)),
        _                   => None
    }
}


fn parse_primary_expression(input: &mut TokenBuffer<'_, '_>) -> CoreResult<Option<Expression>>
{
    match_one_of(input, &[parse_call_expression,
                          parse_grouped_expression,
                          parse_literal_expression])
}


fn parse_expression_with_binding_power(input: &mut TokenBuffer<'_, '_>,
                                       minimum_power: u8) -> CoreResult<Option<Expression>>
{
    let Some(mut left) = parse_primary_expression(input)?
        else
        {
            return Ok(None);
        };

    loop
    {
        let mut lookahead = LookaheadGuard::new(input);

        let Some(operator) = lookahead.buffer.next()?
            else
            {
                break;
            };

        let Some((left_power, right_power, operation)) = math_binding_power(&operator.kind)
            else
            {
                break;
            };

        if left_power < minimum_power
        {
            break;
        }

        lookahead.commit();

        let right = parse_expression_with_binding_power(lookahead.buffer, right_power)?
            .ok_or_else(|| CoreError
                {
                    location: Some(operator.location.clone()),
                    kind: CoreErrorKind::ExpectedExpressionOr(vec![]),
                    message: Some("Expected an expression after the operator.".into())
                })?;

        left = Expression::new_math_operation_expression(operator.location, operation, left, right);
    }

    Ok(Some(left))
}


pub(super) fn parse_expression(input: &mut TokenBuffer<'_, '_>) -> CoreResult<Option<Expression>>
{
    match parse_expression_with_binding_power(input, 0)?
    {
        Some(expression) => Ok(Some(expression)),
        None             => Ok(None) //expected_expression(input)
    }
}
