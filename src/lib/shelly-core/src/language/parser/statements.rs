
use super::*;



fn parse_expression_statement(input: &mut TokenBuffer<'_, '_>) -> CoreResult<Option<Statement>>
{
    let result = parse_expression(input)?;

    if let Some(expression) = result
    {
        return Ok(Some(Statement::new_expression_statement(expression)));
    }

    Ok(None)
}


fn parse_block_statement(_input: &mut TokenBuffer<'_, '_>) -> CoreResult<Option<Statement>>
{
    Ok(None)
}


fn parse_call_statement(input: &mut TokenBuffer<'_, '_>) -> CoreResult<Option<Statement>>
{
    if let Some(call) = parse_call(input, CallContext::Statement)?
    {
        return Ok(Some(Statement::new_call_statement(call.location,
                                                     call.identifier,
                                                     call.arguments)));
    }

    Ok(None)
}


fn parse_null_statement(input: &mut TokenBuffer<'_, '_>) -> CoreResult<Option<Statement>>
{
    let result = try_expect_one_of_tokens(input, &[TokenKind::StatementBreak,
                                                   TokenKind::LineBreak])?;

    if let Some(_) = result
    {
        return Ok(Some(Statement::new_null_statement()));
    }

    Ok(None)
}


pub fn parse_statement(input: &mut TokenBuffer<'_, '_>) -> CoreResult<Option<Statement>>
{
    match_one_of(input, &[parse_expression_statement,
                          parse_block_statement,
                          parse_call_statement,
                          parse_null_statement])
}
