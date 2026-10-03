
use std::fmt::{ self, Display, Formatter };
use super::{ ast::{ * }, tokenizer::{ Token, TokenKind, Tokenizer, TokenizerError } };



pub enum ParserError
{
    TokenizerError(TokenizerError),
    UnexpectedEof,
    UnexpectedToken(Token),
}


impl Display for ParserError
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
    {
        match self
        {
            ParserError::TokenizerError(error) => write!(f, "Tokenizer error: {}", error),
            ParserError::UnexpectedEof => write!(f, "Unexpected end of file."),
            ParserError::UnexpectedToken(token) => write!(f, "Unexpected token: {}.", token),
        }
    }
}


pub type ParserResult<T> = Result<T, ParserError>;


impl From<TokenizerError> for ParserError
{
    fn from(error: TokenizerError) -> Self
    {
        ParserError::TokenizerError(error)
    }
}


fn expect_token<'a>(tokenizer: &mut Tokenizer<'a>,
                    expected: TokenKind) -> ParserResult<(bool, Token)>
{
    if let Some(token) = tokenizer.next_token()?
    {
        if token.kind == expected
        {
            return Ok((true, token));
        }

        return Ok((false, token));
    }

    Err(ParserError::UnexpectedEof)
}


fn try_expect_token<'a>(tokenizer: &mut Tokenizer<'a>,
                        expected: TokenKind) -> ParserResult<Option<(bool, Token)>>
{
    match expect_token(tokenizer, expected)
    {
        Ok(result) => Ok(Some(result)),
        Err(ParserError::UnexpectedEof) => Ok(None),
        Err(error) => Err(error),
    }
}


fn expect_one_of_token<'a>(tokenizer: &mut Tokenizer<'a>,
                          expected: &[TokenKind]) -> ParserResult<(bool, Token)>
{
    if let Some(token) = tokenizer.next_token()?
    {
        if expected.contains(&token.kind)
        {
            return Ok((true, token));
        }

        return Ok((false, token));
    }

    Err(ParserError::UnexpectedEof)
}


fn parse_toplevel_statement<'a>(tokenizer: &mut Tokenizer<'a>) -> ParserResult<Option<AstStatement>>
{
    let command = try_expect_token(tokenizer, TokenKind::Symbol)?;

    if let None = command
    {
        return Ok(None);
    }

    let mut params: Vec<Token> = Vec::new();

    let (found, command) = command.unwrap();

    if !found
    {
        return Err(ParserError::UnexpectedToken(command));
    }

    loop
    {
        let (found, token) =

        match expect_one_of_token(tokenizer,
                                  &[TokenKind::Symbol,
                                  TokenKind::StatementBreak,
                                  TokenKind::LineBreak])
        {
            Ok(result) => result,
            Err(ParserError::UnexpectedEof) => break,
            Err(error) => return Err(error)
        };

        if !found
        {
            return Err(ParserError::UnexpectedToken(token));
        }

        match token.kind
        {
            TokenKind::Symbol => params.push(token),
            TokenKind::StatementBreak | TokenKind::LineBreak => break,
            _ => {}
        }
    }

    Ok(Some(AstStatement::ExecuteStatement(Box::new(
        AstExecuteStatement
        {
            location: command.location.clone(),
            executable_name: command.token_value_text(),
            arguments: params.iter().map(|token|
                {
                    let text = token.token_value_text();

                    AstExpression::Symbol(AstSymbol
                        {
                            location: token.location.clone(),
                            name: text
                        })
                }).collect()
        }))))
}


pub fn parse_text<'a>(tokenizer: &mut Tokenizer<'a>) -> ParserResult<AstTopLevel>
{
    let mut statements = Vec::new();

    while let Some(statement) = parse_toplevel_statement(tokenizer)?
    {
        statements.push(statement);
    }

    Ok(statements)
}
