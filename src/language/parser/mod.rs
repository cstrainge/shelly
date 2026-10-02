
use super::{ ast::AstTopLevel, tokenizer::Tokenizer };



pub fn parse_text<'a>(tokenizer: &mut Tokenizer<'a>) -> AstTopLevel
{
    while let Ok(Some(token)) = tokenizer.next_token()
    {
        println!("{}", token);
    }

    Vec::new()
}
