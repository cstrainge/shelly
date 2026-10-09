
use crate::language::{ ast::{ * },
                       data::value::Value,
                       tokenizer::{ TokenBuffer, TokenKind, TokenValue, TokenLiteral },
                       parser::{ base_utils::{ Lookahead,
                                               match_one_of,
                                               expect_token,
                                               try_expect_token,
                                               expect_block_list_of },
                                 expressions::{ parse_expression,
                                                parse_exec_expression,
                                                parse_value_expression,
                                                parse_command_arguments },
                                 results::{ ParseResult, ParserError, ParserErrorKind } } };


fn expect_statement_end(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<()>
{
    let lookahead = Lookahead::new(buffer);
    if let Some(token) = lookahead.buffer.next()?
       && !matches!(token.kind, TokenKind::LineBreak | TokenKind::StatementBreak | TokenKind::BlockClose)
    {
        return Err(ParserError
            {
                location: Some(token.location),
                kind: ParserErrorKind::ExpectedToken(TokenKind::StatementBreak, Some(token.kind))
            });
    }
    Ok(())
}



fn parse_return_statement(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstStatement>>
{
    let keyword = expect_token(buffer, TokenKind::Return)?;
    let has_expression =
        {
            let lookahead = Lookahead::new(buffer);
            lookahead.buffer.next()?.is_some_and(|token|
                !matches!(token.kind, TokenKind::LineBreak | TokenKind::StatementBreak | TokenKind::BlockClose))
        };

    let expression = if has_expression
        {
            Some(parse_value_expression(buffer)?.ok_or_else(|| ParserError
                {
                    location: Some(keyword.location.clone()),
                    kind: ParserErrorKind::ExpectedExpression
                })?)
        }
        else
        {
            None
        };

    expect_statement_end(buffer)?;
    Ok(Some(AstStatement::ReturnStatement(Box::new(AstReturnStatement
        {
            location: keyword.location,
            expression
        }))))
}


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

    let expression = parse_value_expression(buffer)?;

    if let Some(expression) = expression
    {
        expect_statement_end(buffer)?;
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

    let expression = parse_value_expression(buffer)?;

    if let Some(expression) = expression
    {
        expect_statement_end(buffer)?;
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


fn parse_alias_statement(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstStatement>>
{
    expect_token(buffer, TokenKind::Alias)?;

    let alias = expect_token(buffer, TokenKind::Symbol)?;

    expect_token(buffer, TokenKind::Assign)?;

    let target = expect_token(buffer, TokenKind::Symbol)?;

    let mut arguments = Vec::new();

    loop
    {
        let mut lookahead = Lookahead::new(buffer);
        let Some(token) = lookahead.buffer.next()? else { break; };
        if token.kind == TokenKind::BlockClose { break; }
        lookahead.commit();
        drop(lookahead);

        match token.kind
        {
            TokenKind::LineBreak | TokenKind::StatementBreak => break,

            TokenKind::LineContinue =>
                {
                    expect_token(buffer, TokenKind::LineBreak)?;
                    continue;
                },

            _ => {}
        }

        // Alias arguments are stored values, not expressions to evaluate here.
        let value = match &token.value
            {
                TokenValue::Literal(TokenLiteral::Integer(value, _)) => Value::Integer(*value),
                TokenValue::Literal(TokenLiteral::Float(value, text)) =>
                    Value::Float(*value, Some(text.clone())),
                TokenValue::Literal(TokenLiteral::Boolean(value)) => Value::Boolean(*value),
                _ => Value::from_string(token.token_value_text())
            };

        arguments.push(AstAliasArgument
            {
                expand_path: token.kind != TokenKind::Literal
                    && token.token_value_text().starts_with('~'),
                value
            });
    }

    Ok(new_ast_alias_statement(alias.location.clone(),
                              alias.token_value_text(),
                              target.token_value_text(),
                              arguments))
}


fn parse_value_statement(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstStatement>>
{
    let Some(expression) = parse_expression(buffer)? else { return Ok(None); };

    if matches!(expression.kind, AstExpressionKind::Symbol(_) | AstExpressionKind::VariableSplat(_))
    {
        return Ok(None);
    }

    let lookahead = Lookahead::new(buffer);
    if let Some(token) = lookahead.buffer.next()?
    {
        if !matches!(token.kind, TokenKind::LineBreak | TokenKind::StatementBreak | TokenKind::BlockClose)
        {
            return Ok(None);
        }
    }

    Ok(Some(AstStatement::ExpressionStatement(expression)))
}


fn parse_execute_statement(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstStatement>>
{
    let Some(exec_expression) = parse_exec_expression(buffer)? else { return Ok(None); };
    let location = exec_expression.location.clone();
    let parameter_expressions = parse_command_arguments(buffer)?;

    Ok(new_ast_execute_statement(location,
                            exec_expression.resolve_as_text()?,
                            matches!(&exec_expression.kind,
                                AstExpressionKind::Symbol(symbol) if symbol.name.starts_with('~')),
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

    let mut names = std::collections::HashSet::new();
    for parameter in &parameters
    {
        let name = parameter.resolve_as_text()?;
        if !names.insert(name.clone())
        {
            return Err(ParserError
                {
                    location: Some(parameter.location.clone()),
                    kind: ParserErrorKind::DuplicateParameter(name)
                });
        }
    }

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

    Ok(new_ast_function_statement(name.token_value_text(),
                                  parameter_names,
                                  code))
}


fn parse_block_statement(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstStatement>>
{
    let location =
        {
            let peek = Lookahead::new(buffer);
            expect_token(peek.buffer, TokenKind::BlockOpen)?.location
        };
    let body = expect_block_list_of(buffer,
                                    TokenKind::BlockOpen,
                                    TokenKind::BlockClose,
                                    None,
                                    &(parse_statement as fn(&mut TokenBuffer<'_, '_>)
                                      -> ParseResult<Option<AstStatement>>))?;

    Ok(Some(AstStatement::BlockStatement(Box::new(AstBlockStatement { location, body }))))
}


pub fn parse_statement(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstStatement>>
{
    // EOF is normal between statements. Rewind a real token so the statement rules see it, and
    // leave EOF within required syntax to those rules.

    let next_kind =
    {
        let lookahead = Lookahead::new(buffer);
        let Some(token) = lookahead.buffer.next()? else { return Ok(None); };
        token.kind
    };

    // A function's parameter list can also look like a parenthesized command argument. Once fn
    // starts a declaration, preserve its errors (including incomplete input) instead of falling
    // back to parsing it as a command.
    if next_kind == TokenKind::Function
    {
        return parse_function_statement(buffer);
    }

    if next_kind == TokenKind::Return
    {
        return parse_return_statement(buffer);
    }

    if next_kind == TokenKind::BlockOpen { return parse_block_statement(buffer); }
    if next_kind == TokenKind::Let { return parse_let_statement(buffer); }
    if next_kind == TokenKind::Alias { return parse_alias_statement(buffer); }
    if next_kind == TokenKind::Identifier
    {
        let assignment =
            {
                let lookahead = Lookahead::new(buffer);
                lookahead.buffer.next()?;
                matches!(lookahead.buffer.next()?, Some(token) if token.kind == TokenKind::Assign)
            };
        if assignment { return parse_set_statement(buffer); }
    }

    match_one_of(buffer, &[parse_null_statement,
                           parse_value_statement,
                           parse_execute_statement])
}
