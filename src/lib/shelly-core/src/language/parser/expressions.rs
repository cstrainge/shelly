
use super::*;


fn parse_literal_expression(_input: &mut TokenBuffer<'_, '_>) -> CoreResult<Option<Expression>>
{
    // Implementation for parsing literal expressions goes here.
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



fn expected_expression(input: &mut TokenBuffer<'_, '_>) -> CoreResult<Option<Expression>>
{
    let token = input.next()?;

    Err(CoreError
        {
            location: if let Some(token) = &token { Some(token.location.clone()) } else { None },
            kind: CoreErrorKind::ExpectedExpression,
            message: None
        })
}


pub(super) fn parse_expression(input: &mut TokenBuffer<'_, '_>) -> CoreResult<Option<Expression>>
{
    match_one_of(input, &[parse_grouped_expression,
                          parse_literal_expression,
                          expected_expression])
}
