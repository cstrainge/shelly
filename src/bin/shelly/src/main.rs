
use shelly_core::language::{ ast::print_ast,
                             text::simple_buffer::SimpleBuffer,
                             tokenizer::{Tokenizer, TokenBuffer },
                             parser::parse };



fn tokenize_sample(name: &str, source_text: &str)
{
    let mut buffer = SimpleBuffer::new(name, &source_text, None);
    let mut tokenizer = Tokenizer::new(&mut buffer);

    loop
    {
        let next = tokenizer.next();

        if let Err(error) = next
        {
            eprintln!("Error: {}", error);
            break;
        }

        if let Ok(Some(token)) = next
        {
            println!("{}", token);
        }
        else
        {
            break;
        }
    }
}


fn parse_sample(name: &str, source_text: &str)
{
    let mut buffer = SimpleBuffer::new(name, &source_text, None);
    let mut tokenizer = Tokenizer::new(&mut buffer);
    let mut token_buffer = TokenBuffer::new(&mut tokenizer);

    match parse(&mut token_buffer)
    {
        Ok(ast)    => print_ast(&ast),
        Err(error) => eprintln!("Parse failed: {}", error)
    }
}



fn main()
{
    const FILE_NAME: &str = "test_code.shy";
    const SOURCE_TEXT: &str = include_str!("../../../../test_code.shy");

    tokenize_sample(FILE_NAME, SOURCE_TEXT);

    println!("");

    parse_sample(FILE_NAME, SOURCE_TEXT);
}
