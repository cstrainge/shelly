
use crate::language::{ ast::{ * },
                       tokenizer::{ TokenBuffer, TokenKind },
                       parser::{ base_utils::{ Lookahead,
                                               match_one_of,
                                               expect_token,
                                               try_expect_token,
                                               match_multiple_of },
                                 expressions::{ parse_expression, parse_exec_expression },
                                 results::{ ParseResult, ParserError, ParserErrorKind } } };



fn parse_let_statement(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstStatement>>
{
    // let $var = <expression>

    expect_token(buffer, TokenKind::Let)?;

    let export_flag = if let Some(_) = try_expect_token(buffer, TokenKind::Export)?
        {
            AstExportFlag::Exported
        }
        else
        {
            AstExportFlag::NonExported
        };

    let identifier = expect_token(buffer, TokenKind::Identifier)?;

    expect_token(buffer, TokenKind::Assign)?;

    let expression = parse_expression(buffer)?;

    if let Some(expression) = expression
    {
        Ok(new_ast_let_statement(identifier.location.clone(),
                                 export_flag,
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


fn parse_set_statement(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstStatement>>
{
    // $var = <expression>

    let identifier = expect_token(buffer, TokenKind::Identifier)?;

    expect_token(buffer, TokenKind::Assign)?;

    let expression = parse_expression(buffer)?;

    if let Some(expression) = expression
    {
        Ok(new_ast_set_statement(identifier.location.clone(),
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
    // EOF is normal between statements. Rewind a real token so the statement rules see it, and
    // leave EOF within required syntax to those rules.

    {
        let lookahead = Lookahead::new(buffer);
        if lookahead.buffer.next()?.is_none()
        {
            return Ok(None);
        }
    }

    // Otherwise attempt to match one of these statement types.
    match_one_of(buffer, &[parse_null_statement,
                           parse_let_statement,
                           parse_set_statement,
                           parse_execute_statement])
}
