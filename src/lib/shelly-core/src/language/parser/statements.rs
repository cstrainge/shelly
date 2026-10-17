
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
    let name = try_expect_one_of_tokens(input, &[TokenKind::Identifier, TokenKind::Symbol])?;

    if let Some(name) = name
    {
        let parameters = parse_until_break(input, &name.location, EofOk::Yes, parse_expression)?;

        return Ok(Some(Statement::new_call_statement(name.location.clone(),
                                                     name.as_text(),
                                                     parameters)));
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
