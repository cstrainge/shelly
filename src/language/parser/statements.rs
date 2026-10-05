
use crate::language::{ ast::{ * },
                       tokenizer::{ TokenBuffer, TokenKind },
                       parser::{ base_utils::{ Lookahead,
                                               match_one_of,
                                               expect_token,
                                               try_expect_token,
                                               match_multiple_of,
                                               expect_block_list_of },
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


fn parse_function_statement(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstStatement>>
{
    fn parse_arg(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstExpression>>
    {
        let token = try_expect_token(buffer, TokenKind::Identifier)?;

        if let Some(token) = token
        {
            Ok(Some(new_ast_symbol(token.location.clone(), token.token_value_text(), None)))
        }
        else
        {
            Ok(None)
        }
    }

    expect_token(buffer, TokenKind::Function)?;

    let name = expect_token(buffer, TokenKind::Symbol)?;

    let parameters = expect_block_list_of(buffer,
                                          TokenKind::ParenOpen,
                                          TokenKind::ParenClose,
                                          Some(TokenKind::Comma),
                                          &(parse_arg as fn(&mut TokenBuffer<'_, '_>)
                                            -> ParseResult<Option<AstExpression>>))?;

    // Convert the list of AstExpression into a list of strings representing the parameter names.
    let parameter_names: Vec<String> = parameters.into_iter()
                                                .map(|expr|
                                                    {
                                                        expr.resolve_as_text().unwrap()
                                                    })
                                                .collect();

    while try_expect_token(buffer, TokenKind::LineBreak)?.is_some() {}

    let code = expect_block_list_of(buffer,
                                   TokenKind::BlockOpen,
                                   TokenKind::BlockClose,
                                   None,
                                   &(parse_statement as fn(&mut TokenBuffer<'_, '_>)
                                     -> ParseResult<Option<AstStatement>>))?;

    Ok(new_ast_function_statement(name.location.clone(),
                                  name.token_value_text(),
                                  parameter_names,
                                  code))
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
                           parse_execute_statement,
                           parse_function_statement])
}
