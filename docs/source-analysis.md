# Source analysis foundation

Language tooling uses `SourceSpan` for half-open UTF-8 byte ranges in the
original input. Diagnostic `Location` remains separate: its line and column
are one-based, and columns include the configured tab stops. Peeking a buffer
does not advance its consumed byte offset. A future language-server adapter
can translate these byte ranges to the client's negotiated position encoding.

Every token has an exact source span. Prefetched operators and numeric member
access own their bytes independently of the preceding word. Strings retain
their raw quotes, escapes, and indentation in their token span; decoded token
values are not used to reconstruct the input.

`Tokenizer::new` keeps strict execution behavior and stores no lexical sidecar.
`Tokenizer::with_metadata` additionally records whitespace, comments, line
continuations, and `StringSource` entries. Significant newlines remain tokens.
The metadata consists of owned language facts, with no terminal styles,
interpreter references, module loading, or execution hooks.

String entries record opening and optional closing delimiters, interpolation
eligibility, decoded text, and source-to-decoded text/escape segments. Removed
multiline indentation is represented by gaps between segments. An escape maps
to its entire raw spelling. `StringSource::source_span` maps a nonempty decoded
UTF-8 range back to the enclosing source range; invalid bounds or character
boundaries return `None`. Cross-line mappings include any intervening removed
indentation. Parts are ordered and contiguous in decoded bytes, but can have
gaps in source bytes.

On a lexical error, strict tokenization still stops and reports its existing
diagnostic. Metadata already understood is retained, including an unfinished
string's decoded prefix and absent closing delimiter. The span of that string
covers the input consumed before failure, including a malformed escape.
Recovery and parser-context classification are later work; no new parser or
interpolation grammar is introduced here.

The Shelly test suite exercises this API through a hidden inspection bridge in
the interpreter. That bridge serializes lexical facts only and is not part of
the normal module interface. Future REPL and language-server frontends should
consume the Rust source-analysis types directly.
