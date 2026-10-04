
use crate::language::{ ast::{ * },
                       data::value::Value,
                       tokenizer::{ TokenBuffer, TokenKind, TokenLiteral, TokenValue },
                       parser::{ base_utils::{ expect_token, match_one_of },
                       results::{ ParseResult, ParserError, ParserErrorKind } } };



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

    Ok(Some(new_ast_variable(identifier.location.clone(), identifier_value)))
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

    Ok(Some(new_ast_symbol(symbol.location.clone(), symbol_value)))
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

    let literal_value = match literal_value
        {
            TokenLiteral::Integer(value, _) => Value::Integer(value),
            TokenLiteral::Float(value, text) => Value::Float(value, Some(text)),
            TokenLiteral::String(value) => Value::String(value)
        };

    Ok(Some(new_ast_literal(literal.location.clone(), literal_value)))
}


pub fn parse_expression(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstExpression>>
{
    match_one_of(buffer, &[parse_variable_expression,
                           parse_symbol_expression,
                           parse_literal_expression])
}


pub fn parse_exec_expression(buffer: &mut TokenBuffer<'_, '_>) -> ParseResult<Option<AstExpression>>
{
    match_one_of(buffer, &[parse_variable_expression,
                           parse_symbol_expression,
                           parse_literal_expression])
}
