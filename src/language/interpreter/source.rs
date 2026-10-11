
use std::io::{ BufReader, Cursor };

use crate::language::{ data::{ map_key::MapKey, value::Value },
                       interpreter::{ ErrorWhat, Interpreter, InterpreterError, InterpreterResult },
                       parser::{ analyze_editor, EditorAnalysis, parse_text, parse_with_metadata,
                                 source::SyntaxMetadata },
                       text::{ buffer::{ Buffer, SimpleBuffer }, location::Location,
                               read_buffer::ReadBuffer, span::SourceSpan },
                       tokenizer::Tokenizer };


fn record<const N: usize>(fields: [(&str, Value); N]) -> Value
{
    Value::from_hash_map(fields.into_iter()
        .map(|(name, value)| (MapKey::String(name.into()), value)).collect())
}


fn span_value(span: SourceSpan, source: &str) -> Value
{
    record([
            ("start", Value::Integer(span.start as i64)),
            ("end", Value::Integer(span.end as i64)),
            ("raw", span.slice(source).map_or(Value::None, |text| Value::from_string(text.into()))),
        ])
}


// An opt-in, hidden test bridge. The analysis layer itself has no runtime dependencies.
fn inspect(source: &str, buffer: &mut dyn Buffer, metadata: bool) -> Value
{
    buffer.peek_next();
    buffer.peek_next();
    let peek_offset = buffer.byte_offset();
    let mut tokenizer = if metadata { Tokenizer::with_metadata(buffer) }
        else { Tokenizer::new(buffer) };
    let mut tokens = Vec::new();
    let mut error = Value::None;
    loop
    {
        match tokenizer.next_token()
        {
            Ok(Some(token)) => tokens.push(record([
                    ("span", span_value(token.span, source)),
                    ("kind", Value::from_string(format!("{:?}", token.kind))),
                    ("text", Value::from_string(token.token_value_text())),
                    ("line", Value::Integer(token.location.line as i64)),
                    ("column", Value::Integer(token.location.column as i64)),
                ])),
            Ok(None) => break,
            Err(failure) =>
                {
                    error = record([
                            ("message", Value::from_string(failure.message)),
                            ("line", Value::Integer(failure.location.line as i64)),
                            ("column", Value::Integer(failure.location.column as i64)),
                        ]);
                    break;
                }
        }
    }
    let mut trivia = Vec::new();
    let mut strings = Vec::new();
    if let Some(metadata) = tokenizer.metadata()
    {
        for item in &metadata.trivia
        {
            trivia.push(record([
                    ("kind", Value::from_string(format!("{:?}", item.kind))),
                    ("span", span_value(item.span, source)),
                ]));
        }
        for string in &metadata.strings
        {
            let parts = string.parts.iter().map(|part| record([
                    ("kind", Value::from_string(format!("{:?}", part.kind))),
                    ("source", span_value(part.source, source)),
                    ("decoded", span_value(part.decoded, &string.decoded)),
                ])).collect();
            let mapped = string.decoded.char_indices().map(|(start, character)|
                string.source_span(SourceSpan::new(start, start + character.len_utf8()))
                    .map_or(Value::None, |span| span_value(span, source))).collect();
            strings.push(record([
                    ("span", span_value(string.span, source)),
                    ("opening", span_value(string.opening, source)),
                    ("closing", string.closing.map_or(Value::None,
                        |span| span_value(span, source))),
                    ("interpolated", Value::Boolean(string.interpolated)),
                    ("decoded", Value::from_string(string.decoded.clone())),
                    ("parts", Value::from_array(parts)),
                    ("mapped", Value::from_array(mapped)),
                    ("whole", string.source_span(SourceSpan::new(0, string.decoded.len()))
                        .map_or(Value::None, |span| span_value(span, source))),
                ]));
        }
    }
    record([
            ("tokens", Value::from_array(tokens)),
            ("trivia", Value::from_array(trivia)),
            ("strings", Value::from_array(strings)),
            ("error", error),
            ("consumed", Value::Integer(tokenizer.byte_offset() as i64)),
            ("peek_offset", Value::Integer(peek_offset as i64)),
        ])
}


fn inspect_syntax(source: &str, buffer: &mut dyn Buffer, metadata: bool) -> Value
{
    let mut tokenizer = Tokenizer::new(buffer);
    let (result, syntax) = if metadata { parse_with_metadata(&mut tokenizer) }
        else { (parse_text(&mut tokenizer), SyntaxMetadata::default()) };
    let (statements, error) = match result
    {
        Ok(ast) => (Value::Integer(ast.len() as i64), Value::None),
        Err(error) => (Value::None, Value::from_string(error.to_string())),
    };
    let facts = syntax.facts.into_iter().map(|fact| record([
            ("span", span_value(fact.span, source)),
            ("role", Value::from_string(format!("{:?}", fact.role))),
            ("context", Value::from_string(format!("{:?}", fact.context))),
        ])).collect();
    record([
            ("facts", Value::from_array(facts)),
            ("statements", statements),
            ("error", error),
        ])
}


impl Interpreter
{
    pub(super) fn handle_editor_source(&mut self, location: &Location, args: &[Value])
        -> InterpreterResult<()>
    {
        let [Value::String(source, _)] = args else
        {
            return Err(InterpreterError { location: location.clone(),
                what: ErrorWhat::ArgumentMismatch("__editor_source expects one String".into()) });
        };
        let analysis: EditorAnalysis = analyze_editor(source);
        let tokens = analysis.tokens.iter().map(|token| record([
                ("span", span_value(token.span, source)),
                ("kind", Value::from_string(format!("{:?}", token.kind))),
            ])).collect();
        let facts = analysis.syntax.facts.iter().map(|fact| record([
                ("span", span_value(fact.span, source)),
                ("role", Value::from_string(format!("{:?}", fact.role))),
                ("context", Value::from_string(format!("{:?}", fact.context))),
            ])).collect();
        let diagnostics = analysis.diagnostics.iter().map(|issue| record([
                ("span", span_value(issue.span, source)),
                ("kind", Value::from_string(format!("{:?}", issue.kind))),
                ("phase", Value::from_string(format!("{:?}", issue.phase))),
                ("message", Value::from_string(issue.message.clone())),
            ])).collect();
        let regions = analysis.regions.iter().map(|region| record([
                ("span", span_value(region.span, source)),
                ("status", region.status.map_or(Value::None,
                    |status| Value::from_string(format!("{:?}", status)))),
            ])).collect();
        let trivia = analysis.lexical.trivia.iter().map(|item| record([
                ("span", span_value(item.span, source)),
                ("kind", Value::from_string(format!("{:?}", item.kind))),
            ])).collect();
        let strings = analysis.lexical.strings.iter().map(|string| record([
                ("opening", span_value(string.opening, source)),
                ("interpolated", Value::Boolean(string.interpolated)),
                ("span", span_value(string.span, source)),
                ("decoded", Value::from_string(string.decoded.clone())),
                ("closing", string.closing.map_or(Value::None, |span| span_value(span, source))),
                ("parts", Value::from_array(string.parts.iter().map(|part| record([
                        ("source", span_value(part.source, source)),
                        ("decoded", span_value(part.decoded, &string.decoded)),
                        ("kind", Value::from_string(format!("{:?}", part.kind))),
                    ])).collect())),
            ])).collect();
        self.last_result = Some(record([
                ("tokens", Value::from_array(tokens)), ("facts", Value::from_array(facts)),
                ("diagnostics", Value::from_array(diagnostics)),
                ("regions", Value::from_array(regions)), ("trivia", Value::from_array(trivia)),
                ("strings", Value::from_array(strings)),
                ("unparsed", analysis.unparsed.map_or(Value::None,
                    |span| span_value(span, source))),
            ]));
        Ok(())
    }

    pub(super) fn handle_syntax_spans(&mut self, location: &Location, args: &[Value])
        -> InterpreterResult<()>
    {
        self.handle_source_inspection(location, args, "__syntax_spans", inspect_syntax)
    }

    pub(super) fn handle_source_spans(&mut self, location: &Location, args: &[Value])
        -> InterpreterResult<()>
    {
        self.handle_source_inspection(location, args, "__source_spans", inspect)
    }

    fn handle_source_inspection(&mut self, location: &Location, args: &[Value], name: &str,
                                inspect: fn(&str, &mut dyn Buffer, bool) -> Value)
        -> InterpreterResult<()>
    {
        let [Value::String(source, _), Value::Boolean(stream), Value::Boolean(metadata)] = args else
        {
            return Err(InterpreterError { location: location.clone(),
                what: ErrorWhat::ArgumentMismatch(
                    format!("{} expects a String and two Booleans", name)) });
        };
        let result = if *stream
            {
                let mut reader = BufReader::with_capacity(1, Cursor::new(source.as_bytes()));
                let mut buffer = ReadBuffer::new("<analysis>", &mut reader, None);
                inspect(source, &mut buffer, *metadata)
            }
            else
            {
                let mut buffer = SimpleBuffer::new("<analysis>", source, None);
                inspect(source, &mut buffer, *metadata)
            };
        self.last_result = Some(result);
        Ok(())
    }
}
