
mod language;
mod runtime;

use language::parser::parse_text;
use language::tokenizer::Tokenizer;
use language::text::buffer::SimpleBuffer;



fn main()
{
    //let source = "let x = 1 + 2\nfoo a b c";

    let source = include_str!("../sample.shy");

    let mut buffer = SimpleBuffer::new("<inline>", source, None);
    let mut tokenizer = Tokenizer::new(&mut buffer);

    let _ast = parse_text(&mut tokenizer);
}
