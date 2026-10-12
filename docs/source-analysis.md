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

Executable names discovered by explicit completion requests are cached and copied
into the next symbol snapshot. Prompt refresh and highlighting never walk PATH
or inspect every executable. Completion retains its normal filesystem discovery,
checking executability only for names matching the requested prefix. The cache
is cleared when PATH or cwd changes. Uncached command names retain their grammar
role; source analysis does not need to prove that an executable exists to style
its command position. Configuration and visible symbols refresh after each
command. A future language server can reuse these descriptors and translate
their byte spans to client positions.

`native/semantic_analysis.shy` uses the hidden `__semantic_source` inspection
bridge to verify semantic spans, visibility, provisional scopes, imports,
interpolation, incomplete input, and analysis without execution or type interning.


## Frontend classifications and Reedline rendering

`highlight_spans` combines lexical facts, grammar roles, string escapes, semantic
references, and invalid diagnostics into disjoint `HighlightSpan` values. Each
span has a frontend-independent `HighlightKind`. A sweep over fact boundaries
handles overlaps without allocating a style entry for every input byte. Original
whitespace is retained; the suffix beyond the editor budget remains plain.
Incomplete diagnostics do not turn normal unfinished input into error styling.

`ShellyHighlighter` implements Reedline's `Highlighter` using a frozen
`SymbolSnapshot` and `SyntaxSettings`. It emits `StyledText` slices of the original
buffer; decoded strings are never used for rendering. A mutex-protected cache
reuses classifications when only the cursor or a menu changes. Changed input
uses the existing bounded editor analysis. No interpreter reference, environment
query, filesystem lookup, module loader, or execution hook enters this path.

Before editing, the REPL refreshes the snapshot and reads the configuration's
stored bindings directly, without getters. `$repl_syntax` and the
`$syntax_color_*` variables are typed defaults in each scope. Foreground color
strings are parsed into `Color` values; raw configuration never reaches terminal
output. Empty/default colors disable a category, malformed sequences fall back
to defaults, and colors adapt to the established terminal mode. Monochrome and
disabled highlighting return plain text without running source analysis.

`native/highlighting.shy` checks the actual highlighter through the hidden
`__highlight_source` bridge, including byte reconstruction, cache stability,
all color modes, configuration validation, incomplete input, and source budgets.
`repl/syntax_highlighting.shy` checks raw PTY styling, live configuration and
symbol refresh, editing without evaluation, Unicode cursor movement, multiline
submission, and monochrome output. `repl/prompt_refresh.shy` guards prompt latency
with a long PATH. The existing interactive suite continues to
cover completion, history, terminal capabilities, cancellation, and recovery.


## Language server

`src/server/` is a peer of the language and runtime modules. `shelly --language-server`
dispatches there before shell initialization, with stdin/stdout reserved for framed
JSON-RPC. Native registration is separated from startup so the server can obtain
an owned baseline `SymbolSnapshot` without executing the prelude, profiles, or RC.
Document analysis has no interpreter and never follows imports or reads source files.

The initial server supports initialization, full document synchronization,
`textDocument/semanticTokens/full`, published syntax diagnostics, shutdown, and exit.
Each document version caches its token data and diagnostics; older updates are
ignored. The negotiated position encoding is UTF-16. Source boundaries are converted
in a single pass, and tokens are split at line endings so Unicode and CRLF retain
their original positions. The token legend maps all understood lexical, grammar,
and semantic categories, including custom categories for commands and arguments.
No colors are sent and no TextMate grammar participates.

Whole-file analysis uses `analyze_semantics_with_limits` with a 1 MiB source limit
and 640,000 parser reads. REPL limits remain unchanged, and both frontends retain
the same 64-level nesting bound. Invalid syntax is an error, incomplete input a
hint, and exhausted budgets a warning. Unknown names stay conservative. Workspace
and standard-library indexing, completion, and navigation remain separate work.
The server currently processes messages serially; cancellation notifications do
not interrupt an in-progress bounded analysis.

`native/language_server.shy` tests the real wire protocol, native baseline, startup
isolation, document lifecycle, UTF-16 spans, stale versions, malformed frames,
diagnostic clearing, and whole-file budgets. The thin TypeScript client lives in
`extensions/vscode/`, with the internal name `shelly-shell`.
