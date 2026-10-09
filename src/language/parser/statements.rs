
use crate::language::{ ast::{ * },
                       text::location::Location,
                       data::value::Value,
                       tokenizer::{ TokenBuffer, TokenKind, TokenValue, TokenLiteral },
                       parser::{ base_utils::{ Lookahead,
                                               expect_token,
                                               try_expect_token,
                                               expect_block_list_of },
                                 expressions::{ parse_expression,
                                                parse_exec_expression,
                                                parse_value_expression,
                                                parse_condition_expression,
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
    expect_token(buffer, TokenKind::Let)?;
    let export_flag = if try_expect_token(buffer, TokenKind::Export)?.is_some()
        { AstExportFlag::Exported } else { AstExportFlag::NonExported };
    let identifier = expect_token(buffer, TokenKind::Identifier)?;
    let annotation = if try_expect_token(buffer, TokenKind::TypeDelimiter)?.is_some()
        { Some(super::expressions::parse_type(buffer)?) } else { None };
    let initialized = try_expect_token(buffer, TokenKind::Assign)?.is_some();
    if !initialized && annotation.is_none()
    {
        return Err(ParserError { location: Some(identifier.location),
            kind: ParserErrorKind::ExpectedToken(TokenKind::Assign, None) });
    }
    let expression = if initialized
        {
            parse_value_expression(buffer)?.ok_or_else(|| ParserError
                { location: Some(identifier.location.clone()), kind: ParserErrorKind::ExpectedExpression })?
        }
        else { new_ast_literal(identifier.location.clone(), Value::None, None) };
    expect_statement_end(buffer)?;
    let mut statement = new_ast_let_statement(identifier.location.clone(), export_flag,
        identifier.token_value_text(), expression).unwrap();
    if let AstStatement::LetStatement(item) = &mut statement
    {
        item.annotation = annotation;
        item.default_initialize = !initialized;
    }
    Ok(Some(statement))
}


fn parse_set_statement(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstStatement>>
{
    let mut lookahead = Lookahead::new(buffer);
    let identifier = expect_token(&mut *lookahead.buffer, TokenKind::Identifier)?;
    let indexes = super::expressions::parse_indexes(&mut *lookahead.buffer)?;
    if try_expect_token(&mut *lookahead.buffer, TokenKind::Assign)?.is_none()
    {
        return Ok(None);
    }
    lookahead.commit();
    drop(lookahead);

    let expression = parse_value_expression(buffer)?;

    if let Some(expression) = expression
    {
        expect_statement_end(buffer)?;
        Ok(new_ast_set_statement(identifier.location.clone(),
                                 identifier.token_value_text(),
                                 indexes,
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

    if matches!(expression.kind, AstExpressionKind::Symbol(_) | AstExpressionKind::VariableSplat(_)
        | AstExpressionKind::Splat(_))
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
                            matches!(&exec_expression.kind,
                                AstExpressionKind::Symbol(symbol) if symbol.name.starts_with('~')),
                            exec_expression,
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
    expect_token(buffer, TokenKind::Function)?;
    let name = expect_token(buffer, TokenKind::Symbol)?;
    expect_token(buffer, TokenKind::ParenOpen)?;
    let mut parameters = Vec::new();
    let mut names = std::collections::HashSet::new();
    loop
    {
        while try_expect_token(buffer, TokenKind::LineBreak)?.is_some() {}
        if parameters.is_empty() && try_expect_token(buffer, TokenKind::ParenClose)?.is_some() { break; }
        let parameter = expect_token(buffer, TokenKind::Identifier)?;
        let parameter_name = parameter.token_value_text();
        let mut variadic = try_expect_token(buffer, TokenKind::Splat)?.is_some();
        if !names.insert(parameter_name.clone())
        {
            return Err(ParserError { location: Some(parameter.location),
                kind: ParserErrorKind::DuplicateParameter(parameter_name) });
        }
        let annotation = if try_expect_token(buffer, TokenKind::TypeDelimiter)?.is_some()
            {
                if variadic
                {
                    return Err(ParserError { location: Some(parameter.location),
                        kind: ParserErrorKind::InvalidType("Put '...' after the element type: $rest: Number...".to_string()) });
                }
                let (annotation, typed_variadic) = super::expressions::parse_parameter_type(buffer)?;
                variadic = typed_variadic;
                Some(if variadic { AstType::Array(Box::new(annotation)) } else { annotation })
            }
            else { None };
        parameters.push(AstParameter { location: parameter.location, name: parameter_name,
            annotation, optional: false, variadic, type_id: None });
        while try_expect_token(buffer, TokenKind::LineBreak)?.is_some() {}
        if try_expect_token(buffer, TokenKind::ParenClose)?.is_some() { break; }
        if variadic
        {
            return Err(ParserError { location: Some(parameters.last().unwrap().location.clone()),
                kind: ParserErrorKind::InvalidType("A variadic parameter must be last.".to_string()) });
        }
        expect_token(buffer, TokenKind::Comma)?;
    }
    let return_annotation = if try_expect_token(buffer, TokenKind::TypeDelimiter)?.is_some()
        { Some(super::expressions::parse_type(buffer)?) } else { None };

    while try_expect_token(buffer, TokenKind::LineBreak)?.is_some() {}

    let code = expect_block_list_of(buffer,
                                   TokenKind::BlockOpen,
                                   TokenKind::BlockClose,
                                   None,
                                   &(parse_statement as fn(&mut TokenBuffer<'_, '_>)
                                     -> ParseResult<Option<AstStatement>>))?;

    Ok(new_ast_function_statement(name.location.clone(), name.token_value_text(),
                                  parameters,
                                  return_annotation,
                                  code))
}


fn parse_block(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<AstBlockStatement>
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

    Ok(AstBlockStatement { location, body })
}


pub(super) fn parse_if_expression(buffer: &mut TokenBuffer<'_, '_>,
                                  location: Location) -> ParseResult<AstExpression>
{
    let mut branches = Vec::new();
    let mut else_body = None;
    let mut condition_location = location.clone();

    loop
    {
        let condition = parse_condition_expression(buffer)?.ok_or_else(|| ParserError
            {
                location: Some(condition_location.clone()),
                kind: ParserErrorKind::ExpectedExpression
            })?;
        while try_expect_token(buffer, TokenKind::LineBreak)?.is_some() {}
        let body = parse_block(buffer)?;
        branches.push(AstIfBranch { condition, body });

        // Newlines and comments may separate branches. Preserve them for the
        // enclosing statement parser if no else follows.
        let mut lookahead = Lookahead::new(buffer);
        while try_expect_token(&mut *lookahead.buffer, TokenKind::LineBreak)?.is_some() {}
        if try_expect_token(&mut *lookahead.buffer, TokenKind::Else)?.is_none() { break; }
        while try_expect_token(&mut *lookahead.buffer, TokenKind::LineBreak)?.is_some() {}
        let else_if = try_expect_token(&mut *lookahead.buffer, TokenKind::If)?;
        lookahead.commit();
        drop(lookahead);

        if let Some(keyword) = else_if
        {
            condition_location = keyword.location;
        }
        else
        {
            else_body = Some(parse_block(buffer)?);
            break;
        }
    }

    Ok(AstExpression
        {
            location: location.clone(),
            kind: AstExpressionKind::IfExpression(Box::new(AstIfExpression
                {
                    location,
                    branches,
                    else_body
                })),
            string_flag: None
        })
}


fn parse_for_statement(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstStatement>>
{
    let keyword = expect_token(buffer, TokenKind::For)?;
    let first = expect_token(buffer, TokenKind::Identifier)?;
    let mut bindings = vec![first.token_value_text()];
    if try_expect_token(buffer, TokenKind::Comma)?.is_some()
    {
        let second = expect_token(buffer, TokenKind::Identifier)?;
        let name = second.token_value_text();
        if name == bindings[0]
        {
            return Err(ParserError
                {
                    location: Some(second.location),
                    kind: ParserErrorKind::DuplicateLoopBinding(name)
                });
        }
        bindings.push(name);
    }
    expect_token(buffer, TokenKind::In)?;
    let iterable = parse_condition_expression(buffer)?.ok_or_else(|| ParserError
        {
            location: Some(keyword.location.clone()),
            kind: ParserErrorKind::ExpectedExpression
        })?;
    while try_expect_token(buffer, TokenKind::LineBreak)?.is_some() {}
    let body = parse_block(buffer)?;
    expect_statement_end(buffer)?;
    Ok(Some(AstStatement::ForStatement(Box::new(AstForStatement
        {
            location: keyword.location,
            bindings,
            iterable,
            body
        }))))
}


fn parse_struct_statement(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstStatement>>
{
    expect_token(buffer, TokenKind::Struct)?;
    let name = super::expressions::expect_type_name(buffer)?;
    while try_expect_token(buffer, TokenKind::LineBreak)?.is_some() {}
    expect_token(buffer, TokenKind::BlockOpen)?;
    let mut fields = Vec::new();
    loop
    {
        while try_expect_token(buffer, TokenKind::LineBreak)?.is_some() {}
        if try_expect_token(buffer, TokenKind::BlockClose)?.is_some() { break; }
        let field = expect_token(buffer, TokenKind::Identifier)?;
        let field_name = field.token_value_text()[1..].to_string();
        if !super::expressions::valid_type_name(&field_name)
        {
            return Err(ParserError { location: Some(field.location),
                kind: ParserErrorKind::InvalidType("Invalid struct field name.".to_string()) });
        }
        let optional =
            {
                let mut peek = Lookahead::new(buffer);
                if peek.buffer.next()?.is_some_and(|token| token.kind == TokenKind::Symbol && token.token_value_text() == "optional")
                { peek.commit(); true } else { false }
            };
        let has_type =
            {
                let peek = Lookahead::new(buffer);
                peek.buffer.next()?.is_some_and(|token| !matches!(token.kind,
                    TokenKind::Comma | TokenKind::BlockClose | TokenKind::LineBreak))
            };
        let annotation = if has_type { super::expressions::parse_type(buffer)? }
            else { AstType::Named("any".to_string()) };
        fields.push(AstFieldDeclaration { name: field_name, annotation, optional, location: field.location });
        while try_expect_token(buffer, TokenKind::LineBreak)?.is_some() {}
        if try_expect_token(buffer, TokenKind::Comma)?.is_some() { continue; }
        expect_token(buffer, TokenKind::BlockClose)?;
        break;
    }
    expect_statement_end(buffer)?;
    Ok(Some(AstStatement::StructDeclaration(Box::new(AstStructDeclaration
        { location: name.location.clone(), name: name.token_value_text(), fields }))))
}


fn parse_enum_statement(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstStatement>>
{
    let keyword = expect_token(buffer, TokenKind::Enum)?;
    let name = super::expressions::expect_type_name(buffer)?;
    while try_expect_token(buffer, TokenKind::LineBreak)?.is_some() {}
    expect_token(buffer, TokenKind::BlockOpen)?;
    let mut variants = Vec::new();
    loop
    {
        while try_expect_token(buffer, TokenKind::LineBreak)?.is_some() {}
        if try_expect_token(buffer, TokenKind::BlockClose)?.is_some() { break; }
        let variant = super::expressions::expect_type_name(buffer)?;
        variants.push((variant.token_value_text(), variant.location));
        while try_expect_token(buffer, TokenKind::LineBreak)?.is_some() {}
        if try_expect_token(buffer, TokenKind::Comma)?.is_some() { continue; }
        expect_token(buffer, TokenKind::BlockClose)?;
        break;
    }
    if variants.is_empty()
    {
        return Err(ParserError { location: Some(keyword.location),
            kind: ParserErrorKind::InvalidEnum("An enum requires at least one variant.".to_string()) });
    }
    expect_statement_end(buffer)?;
    Ok(Some(AstStatement::EnumDeclaration(Box::new(AstEnumDeclaration
        { location: name.location.clone(), name: name.token_value_text(), variants }))))
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
    if next_kind == TokenKind::Struct { return parse_struct_statement(buffer); }

    if next_kind == TokenKind::Enum { return parse_enum_statement(buffer); }

    if next_kind == TokenKind::Function
    {
        return parse_function_statement(buffer);
    }

    if next_kind == TokenKind::Return
    {
        return parse_return_statement(buffer);
    }

    if next_kind == TokenKind::For { return parse_for_statement(buffer); }

    if next_kind == TokenKind::Loop
    {
        expect_token(buffer, TokenKind::Loop)?;
        while try_expect_token(buffer, TokenKind::LineBreak)?.is_some() {}
        let body = parse_block(buffer)?;
        expect_statement_end(buffer)?;
        return Ok(Some(AstStatement::LoopStatement(Box::new(body))));
    }

    if matches!(next_kind, TokenKind::Break | TokenKind::Continue)
    {
        let keyword = expect_token(buffer, next_kind)?;
        expect_statement_end(buffer)?;
        return Ok(Some(if next_kind == TokenKind::Break
            { AstStatement::BreakStatement(keyword.location) }
            else { AstStatement::ContinueStatement(keyword.location) }));
    }

    if matches!(next_kind, TokenKind::While | TokenKind::Until)
    {
        let keyword = expect_token(buffer, next_kind)?;
        let condition = parse_condition_expression(buffer)?.ok_or_else(|| ParserError
            {
                location: Some(keyword.location),
                kind: ParserErrorKind::ExpectedExpression
            })?;
        while try_expect_token(buffer, TokenKind::LineBreak)?.is_some() {}
        let body = parse_block(buffer)?;
        expect_statement_end(buffer)?;
        return Ok(Some(AstStatement::ConditionalLoopStatement(Box::new(AstConditionalLoopStatement
            {
                condition,
                body,
                until: next_kind == TokenKind::Until
            }))));
    }

    if next_kind == TokenKind::If
    {
        let expression = parse_expression(buffer)?.ok_or(ParserError
            {
                location: None,
                kind: ParserErrorKind::ExpectedExpression
            })?;
        expect_statement_end(buffer)?;
        return Ok(Some(AstStatement::ExpressionStatement(expression)));
    }
    if next_kind == TokenKind::Else
    {
        return Err(ParserError
            {
                location: Some(expect_token(buffer, TokenKind::Else)?.location),
                kind: ParserErrorKind::UnexpectedElse
            });
    }
    if next_kind == TokenKind::BlockOpen
    {
        return Ok(Some(AstStatement::BlockStatement(Box::new(parse_block(buffer)?))));
    }
    if next_kind == TokenKind::Let { return parse_let_statement(buffer); }
    if next_kind == TokenKind::Alias { return parse_alias_statement(buffer); }
    if next_kind == TokenKind::Identifier
    {
        if let Some(statement) = parse_set_statement(buffer)? { return Ok(Some(statement)); }
    }

    if let Some(statement) = parse_null_statement(buffer)? { return Ok(Some(statement)); }

    // A value mismatch can fall back to command syntax, but malformed expressions
    // must stay errors, including inside branches that will not be executed.
    {
        let mut lookahead = Lookahead::new(buffer);
        if let Some(statement) = parse_value_statement(&mut *lookahead.buffer)?
        {
            lookahead.commit();
            return Ok(Some(statement));
        }
    }
    parse_execute_statement(buffer)
}
