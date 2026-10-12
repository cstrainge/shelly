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
The editor analysis described below adds recovery. The semantic layer can
then classify known symbols and references inside interpolated strings.

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

## Incomplete and invalid input

`analyze_editor` returns an `EditorAnalysis` containing tokens, lexical and
syntax metadata, diagnostics, statement regions, and an optional source suffix
that exceeded the size limit. It owns its result and has no interpreter or
terminal dependency. It never returns a recovered AST to the compiler.

The editor lexer uses the execution tokenizer's rules. At EOF it can return a
partial literal, variable, or member with an `Incomplete` diagnostic. Bad words
become editor-only `Invalid` tokens. A broken single-line string ends before
its physical newline, leaving that newline available to the parser. Invalid
escapes record their original byte spans and use a replacement character only
in decoded metadata; the original input and all token spans remain unchanged.
String indentation and escape mappings use the same machinery as strict input.

Statement recovery uses the existing grammar and token transactions. Required
syntax failures save their furthest understood prefix before rollback. A failed
statement rewinds, retains that prefix, and synchronizes at a statement boundary
or an enclosing closing brace. An EOF failure retains understood block bodies.
Successful alternatives discard their failure candidates. Malformed constructs
can still leave some tokens without grammar facts; their lexical information
remains available for basic styling. Recovery does not resolve names or infer
whether following text was intended as part of a broken construct.

Diagnostics distinguish incomplete syntax, invalid syntax, and analysis limits.
Their half-open byte spans can be empty for a missing token at EOF. Statement
regions retain complete, incomplete, invalid, or limited status and can nest.
Recovery placeholders exist only inside analysis; execution parsing and its
existing errors remain strict.

Analysis bounds source parsing to 65,536 bytes at a UTF-8 boundary, grammar
nesting to 64 levels, and parser reads (including lookahead) to 40,000 steps.
When a parser limit is reached, lexical analysis still covers the bounded input.
An oversized suffix is returned as an original source span for plain rendering.
Permanent stream read errors stop lexical recovery. These limits bound work for
large pastes and deeply nested editing buffers without inserting source text,
loading modules, querying the environment, or running any user code.

The editor tests walk every prefix of representative declarations and pipelines,
compare complete input with strict parsing, verify source reconstruction on
malformed text, and assert that later commands retain their argument context.
The syntax and source-span tests continue to validate strict mode separately.


## Read-only semantic information

`Interpreter::symbol_snapshot` copies currently visible names, nominal type
identities, field types, method signatures, module exports, aliases, and native
functions into an owned `SymbolSnapshot`. Both the snapshot and semantic analysis
result are `Send + Sync`; the snapshot
contains no interpreter, `Rc`, value, executable body, live receiver, or getter.
Hidden registrations stay hidden. Qualified names follow loaded module exports;
private extensions from other modules remain unavailable.

Snapshot creation does not read special-variable getters or call type inference.
Even iterator signature lookup uses the registered signature, avoiding the normal
lookup path that can intern specialized `next_item` result types. Unknown binding
types remain unknown rather than reading a referenced value through a getter.

`analyze_semantics(source, snapshot)` returns editor metadata plus sorted semantic
facts for variables, types, functions, methods, builtins, aliases, known external
commands, fields, enum variants, and module namespaces. Semantic spans refer to
original UTF-8 bytes and supplement grammar facts; they do not change the input.
Unknown names keep their syntax roles without a speculative semantic error.

The existing editor parser supplies a private recovered tree for a read-only
walk. Functions, parameters, local types, fields, aliases, and bindings in that
tree are prospective: they are never registered. Scope boundaries and declaration
order control local references. Annotations and simple understood initializers
can identify receiver types, including typed collections and iterator elements.
Unknown unions, dependent types, and computed results stay conservative. This
is not compilation, flow analysis, or validation of a program's types. Registered
type descriptors are shared through an immutable `Arc`; each editing pass adds
its own local descriptors instead of cloning every registered field and method.

String references use the same pure name scanner as execution. Decoded spans map
back through `StringSource` to their original spelling, preserving Unicode,
escapes, and multiline indentation. Escaped dollars and noninterpolated strings
are excluded. Incomplete braced references can still receive variable styling.

Analysis never evaluates conditions, calls functions or conversion methods,
expands aliases, loads modules, reads files, invokes programs, or changes the
interpreter. Imports can consult exports already present in the snapshot, but
cannot discover an unloaded module. The parser's source and work limits still
apply, and the semantic walk caps expression depth at 64 for long operator
chains. Indexed span and region lookups avoid repeatedly scanning whole buffers.

Before each REPL prompt, completion builds an executable-name inventory from
PATH and shares it in the symbol snapshot. Completion reuses that inventory
while editing, and future highlighting can consume it without probing PATH on
keystrokes. Configuration and visible symbols are refreshed after each command.
This phase exposes analysis only; Reedline styling and color configuration are
separate integration work. A future language server can reuse these descriptors
and translate their byte spans to client positions.

`native/semantic_analysis.shy` uses the hidden `__semantic_source` inspection
bridge to verify semantic spans, visibility, provisional scopes, imports,
interpolation, incomplete input, and analysis without execution or type interning.
