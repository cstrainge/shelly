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
Incomplete-input recovery and semantic classification remain later work; no
new parser or interpolation grammar is introduced here.

The Shelly test suite exercises this API through a hidden inspection bridge in
the interpreter. That bridge serializes lexical facts only and is not part of
the normal module interface. Future REPL and language-server frontends should
consume the Rust source-analysis types directly.

## Parser context

`parse_with_metadata` runs the same grammar as `parse_text` and returns its
strict parse result alongside `SyntaxMetadata`. Facts have exact token spans,
a syntax role, and an expression, command, argument, or type context. Normal
execution leaves the journal disabled. No AST compilation, type checking,
module discovery, command lookup, or interpreter evaluation is involved.

Roles identify declaration names (variables, parameters, functions, aliases,
types, fields, and enum variants), explicit type references, import modules and
selections, keywords, members, literals, operators, and delimiters. Grammar
branches also mark discards and pipe conversion annotations. A command name
is a syntactic call candidate; resolving it to a function, builtin, alias,
type, or external program remains the semantic layer's responsibility. A
pipe's outgoing annotation remains `PipeAnnotation` until registered types
and conversion methods can disambiguate it. Incoming format names are explicit.

The existing grammar supplies these distinctions. For example, `let` is a
keyword in `let $x = 1`, an argument in `echo let`, and a field name in
`Box(let: 1)`. Parenthesized values and collection entries use expression
context inside arguments, and annotations use type context inside signatures.
Literal tokens keep their lexical identity as well as their enclosing context.

The optional journal follows every token-buffer transaction. Reads and role
overrides are appended; rollback truncates both, including nested committed
trials inside a discarded outer alternative. Token cache cleanup does not
clear facts from previous statements. Snapshots apply the latest committed
override per token and return facts in source order. This avoids stale keyword
or declaration coloring from speculative grammar alternatives.

On a strict parse error, the returned facts cover reads that were not rolled
back before the error. They do not claim a successful AST or recover the
remaining input. Tokenization's separate metadata can provide basic lexical
information for unclassified text. Both metadata layers are owned, free of
terminal styling, and suitable for a future REPL or language-server adapter.

`native/syntax_spans.shy` checks the parser through the hidden `__syntax_spans`
bridge. It verifies exact source coverage on successful parses, grammar roles,
streaming parity, strict-error parity, and rollback across failed alternatives.
