# Shelly tests

Run from the repository root with the binary being tested:

```sh
./target/debug/shelly -m test.shy
./target/debug/shelly -m test.shy process
./target/debug/shelly -m test.shy repl
./target/debug/shelly -m test.shy native
./target/debug/shelly -m test.shy harness
./target/debug/shelly -m test.shy REDIRECTION-
```

An optional argument selects a group or a substring of a test filename. Empty
selections fail. Full/process runs validate `INVENTORY.tsv` against the files on
disk, rejecting duplicate IDs, missing/orphan files, and invalid paths. Focused
selections skip that global check. The harness group includes the catalog audit.

`test.shy` concatenates the appropriate `support/*.shy` library with each selected
test into a disposable driver. Drivers and the programs they test use the same
`$shelly` binary that launched the runner. Every driver must exit zero to pass;
`cases/negative` describes expected rejection of the nested program, not failure
of the assertion driver.

`native/source_spans.shy` checks the shared tokenizer's UTF-8 byte spans and
optional lexical metadata. Its hidden `__source_spans` inspection bridge must
be enabled explicitly with `visible`; it tokenizes text without evaluating it.
The checks compare string and one-byte streaming buffers, reconstruct original
input from token/trivia spans, and verify string escapes and indentation maps.
`native/syntax_spans.shy` uses the hidden `__syntax_spans` bridge to check parser
roles and contexts, nested lookahead rollback, and strict parsing parity.
`native/editor_analysis.shy` uses the hidden `__editor_source` bridge to check
partial literals and statements, recovery boundaries, source preservation,
strict rejection, and analysis limits.
`native/semantic_analysis.shy` checks read-only symbol snapshots, provisional
local scopes, receiver methods, imported exports, and string references through
the hidden `__semantic_source` bridge.
`native/highlighting.shy` checks source-preserving rendering, category colors,
color-mode adaptation, settings, and incomplete input. `repl/syntax_highlighting.shy`
checks actual Reedline styling, configuration refresh, and Unicode/multiline edits.
`repl/prompt_refresh.shy` uses a deliberately long PATH to catch executable scans
that block startup or prompt refresh.
`native/language_server.shy` drives the real LSP stdio mode and checks framing,
startup isolation, document versions, semantic tokens, Unicode positions,
diagnostics, whole-file analysis limits, and delimiter exclusion inside strings
and comments. The VS Code extension's `test.shy` runs Shelly assertions against
Node editor fixtures; run `npm test` from `extensions/vscode` after building the
debug binary and installing the extension's development dependencies.

`native/nested_imports.shy` checks nested module paths, `init.shy` packages,
search precedence, ambiguous file/directory layouts, public re-exports, shared identities,
directory preludes, reloads, rejected direct entry-point imports, and source spans.
`repl/nested_imports.shy` checks package state across separate submissions.
`repl/module_search.shy` checks startup-script local lookup, source-relative nested
imports, colon-separated search paths, and ordinary/standard lookup separation.
`native/bundled_std.shy` checks bundled imports through their public module names,
without depending on whether a module uses a file or directory layout.

`native/struct_protocols.shy` checks named factory arguments, return validation,
private storage and constructors, child-module access, computed properties,
raw/dynamic/interpolated member access, enum defaults, and pipeline construction.
`repl/struct_protocols.shy` checks setter rollback, nested writeback, compiler
contract rollback, and recovery after constructor recursion across submissions.
Existing struct fixtures mark their publicly constructed storage with `pub`.

## Process case records

`native/colon_arguments.shy` verifies attached colons in command and alias words,
argument counts, source metadata, and preservation of type and pipeline delimiters.

Every process case calls `check` with an explicit record:

```text
check [
    "id": "EXAMPLE", "name": "prints hello", "source": "echo hello",
    "mode": "code", "args": [], "stdout": "hello\n", "status": 0,
    "valid": true, "error": "", "stderr_contains": [],
    "stderr_excludes": [], "stdout_excludes": [],
]
```

Add its ID/group/name/path to `INVENTORY.tsv` and update the expected process
case count in `harness/inventory.shy`. Names in that TSV escape literal
newlines/tabs as `\n`/`\t`; source and expected output retain their exact bytes in
Shelly string literals. IDs use letters, digits, underscores, and hyphens.

Modes preserve the original CLI boundary: `code` uses `-c`, `file` executes
`input.shy`, `stdin` uses `-s`, `invalid_file` writes deliberately invalid bytes,
and `bad_tab` probes invalid CLI options. `args` is an array of strings.
`<FIXTURE>` in source expands to the case directory; that path is normalized back
to `<FIXTURE>` in output before comparison. Invalid-file source markers
`<INVALID>` and `<TRUNCATED>` produce bytes FF and C3 respectively.

Stdout is always exact, including an empty expectation. Negative records require
status 1 and a known diagnostic regex (the whitelist is in `support/process.shy`).
Required/forbidden stderr details and forbidden stdout details are also checked.
Positive stderr is empty unless the record explicitly requires diagnostic text.
Panics, stack overflows, signals, launch failures, and timeouts never count as a
language rejection. Cases intentionally returning a nonzero success status state
that status explicitly. Binary assertions use output files and `cmp`, avoiding
UTF-8 string conversion.

## Interactive and harness checks

`repl/R*.shy` supplies sequences of source, expectation kind, and expected text to
`check_repl`. It opens a real PTY, answers terminal capability queries, submits
bracketed paste, strips terminal control sequences, and checks state across
submissions. Diagnostic matching examines the actual diagnostic suffix, so
repainted source cannot supply a missing error. `prompts.shy` covers custom
prompts and home-shortened paths; `repl/harness.shy` tests the matcher itself.
`native/prompt.shy` and `repl/prompt_variable.shy` check the built-in `Prompt` type,
string interpolation and returned prompt strings, callable contracts, live closures,
imported callbacks, stdout isolation, temporary widget bindings, empty prompts,
default initialization/fallback, and recovery across submissions.

`native/readline*.shy` checks the scripted Reedline wrapper with real PTYs: prompt functions,
Unicode and byte spans, blank/EOF/interrupt events, both editing modes, completion, validation,
callback budgets and errors, isolated persistent history, stdout routing and managed pipelines.
`repl/readline.shy` checks a custom REPL running within a shell submission and returning its
terminal to the built-in editor.

`native/` checks string methods, process results/settings, exact binary I/O,
redirection, live exports, terminal identity/lifecycle, UTF-8 boundaries, deadlines,
process-group cleanup, and Ctrl+C. `harness/` tests schema/result validation,
catalog/matrix completeness, miniature suites exercising the actual runner, and Ctrl+C against a frozen suite.
`repl/interrupt.shy` checks Ctrl+C during commands, redirection, pipelines, supervised
processes, and scripted loops, preserving captured signal statuses and accepting
subsequent commands. Noninteractive Ctrl+C termination remains covered by the native
terminal and frozen-suite checks.
`repl/history*.shy` checks restart and multiline restoration, exact source preservation,
early persistence, interruption and explicit exit, shared sessions and navigation,
typed startup settings, limits, private file permissions, disabled persistence,
malformed/invalid UTF-8 stores, and recovery when storage fails during a session.
These tests deliberately run failing children and assert that they fail correctly.
`native/array_reverse.shy` and `repl/array_reverse.shy` check nonmutating reversal,
empty and mixed arrays, nested copies, named arrays, callable elements, argument
expansions, property references, retained method versions, and argument rejection.
`native/dynamic_members.shy` and `repl/dynamic_members.shy` check braced member
interpolation, dynamic String accessor names, nested typed writes, method references,
closure captures, read-only properties, and REPL state after rejected accesses.
`native/method_result_types.shy` and `repl/method_result_types.shy` check declared
method results, forward definitions, nullable and numeric unions, widening,
typed writes, imported extensions, lexical overrides, retained versions, and
atomic rejection of incompatible return declarations.
`native/collection_properties.shy` and `repl/collection_properties.shy` check array
and hash type metadata, explicit and inferred types, empty and mixed collections,
nested projections, named types, live receivers, reflection, and read-only access.
`native/receiver_annotations.shy` and `repl/receiver_annotations.shy` check
receiver-dependent collection method signatures, separate hash key/value checks,
numeric promotion, unions and named types, variadic arguments, composed annotations,
compile-time rejection before execution, runtime fallback, imported methods,
stored references, callable prototypes, and return checks after receiver mutation.
`native/properties.shy` and `repl/properties.shy` check member snapshots, type
metadata, scalar and collection values, enum indexes, callback fields, builtin
and user methods, public module extensions, live receivers, retained method
versions, iterator references, named types, collisions, and invalid arguments.
`native/outer_command.shy` and `repl/outer_command.shy` check `::name` external
lookup across function, prelude, alias, and native shadows; grouped calls, literal
arguments, saved and named-type references, redirections, pipeline data/status, missing binaries,
malformed syntax, and REPL recovery.
`native/prelude_symbols.shy` and `repl/prelude_symbols.shy` cover implicit public
functions, live variables, aliases, native visibility, re-exported namespaces,
private symbols, ordinary module imports, reload replacement/removal, binding
conflicts, saved references, and retained module generations. The native and
bundled-library tests also run the prelude's variadic `sum` through row pipelines.
`native/banner_sixel.shy` checks the optional Python/Pillow banner renderer's cell
bounding boxes and pixel dimensions, control bytes, text layout, output files, and invalid inputs.
It skips when Python or Pillow is unavailable; the harness orchestration remains in Shelly.
`repl/banner_sixel.shy` checks embedded banner selection through raw PTY responses,
unsupported and malformed attributes, timeouts, fragmented/late replies, startup
input, init overrides, terminal mode restoration, and monochrome/banner/script gating.
`repl/banner_sixel_sizing.shy` checks cell-size reports, fitted dimensions, small
terminal bounds, invalid dimensions, fallback sizing, and input during the cell-size query.
`repl/banner_iterm2.shy` checks native PNG precedence over sixel, iTerm2 identity,
unsupported/multiplexed fallbacks, fragmented/late/missing replies, startup input,
init overrides, interpolation, terminal restoration, and monochrome/banner/script gating.
`repl/banner_iterm2_sizing.shy` checks the same cell-size fitting and fallback cases
for native PNG without requiring sixel support. Rust unit tests decode the PNG
payload to verify transparency, partial alpha, pixel dimensions, shared layout,
and bounded multipart transmission for large images.
`native/iterator_protocol.shy` and `repl/iterator_protocol.shy` check user and native
iteration, private state, method versions, destructuring, unit termination, fixed-length
array annotations, and recovery after iterator errors.
`native/for_discard.shy` and `repl/for_discard.shy` check `_` loop bindings,
partial and repeated destructuring discards, unchanged outer bindings, typed
retained items, closures, native and custom iterators, loop control, and diagnostics.
The native and REPL `collection_iteration_types.shy` tests check declared and
inferred element types, typed loop bindings, mixed/empty collections, mutations,
native overrides, and inference across REPL submissions.
The native and REPL `named_types.shy` tests cover distinct type identities,
wrapping and explicit conversions, union coercion, inherited operations, typed
iteration, module/prelude imports, redeclarations, and atomic failed writes.
The native and REPL `conversion_protocol.shy` tests cover implied typedef factories,
explicit overrides, retained references and module exports, plus target-side factories,
exact source identities, typed array/hash naming, empty declared collections,
lowercase naming, signature and return contracts,
fallback rules, module privacy and re-exports, prelude access, frozen function
versions, and recovery after conversion errors.
The native and REPL `function_types.shy` tests cover function prototypes, `any`,
call-time contracts, higher-order functions, live methods, imports, versioned
references, signature mismatches, and failed-call recovery.
The native and REPL `widgets.shy` tests check the predefined `WidgetFn` type and
typed `$widgets` array, callback captures, module initialization, incompatible
signatures, return contracts, and recovery after rejected assignments.
`repl/widget_prompt.shy` checks ordered widget rendering, empty and Unicode
results, live captures, stdout isolation, list mutations, imported prompts,
temporary variable cleanup, and recovery after widget or prompt errors.
`repl/widget_colors.shy` checks raw widget colors in truecolor, 256-color, basic,
and monochrome terminals, live `$widget_color` overrides, rejected assignments,
foreground resets, and empty widget suppression.
`repl/last_cmd_time.shy` checks previous-command duration, failed commands, blank
input, prompt/widget timing isolation, and imported widget access.
The native and REPL `workspace_widgets.shy` tests check Python and Node workspace
detection, parent directories, paths with spaces, version-source precedence,
matching and differing versions, missing Node, and environment/PATH changes after
import. Native checks also exercise `package.json` parsing when Node is installed;
REPL checks verify recovery and environment cleanup after a failed widget.
The native and REPL `anonymous_functions.shy` tests cover function literals,
live lexical captures, returned and nested closures, shared and independent
bindings, caller shadowing, live receivers, typed collections, module interfaces,
callable identity, signature errors, and recovery across REPL submissions.
The native and REPL `type_guards.shy` tests check runtime type values, exact
identities, imports and redeclarations, positive/negative branch narrowing,
short-circuit guards, typed collections, mutation invalidation, metadata errors,
and recovery after failed checks.
`native/visibility.shy` and `repl/visibility.shy` check native and scripted exports,
private bindings, re-exports, callable identity, diagnostics, and REPL recovery.
The native and REPL `module_privacy.shy` tests check private-by-default declarations,
`pub` interfaces, methods and iterators, aliases, private imports and public re-exports,
prelude isolation, `pub visible`, invalid modifiers, and failed-submission recovery.
The native and REPL `module_extensions.shy` tests check extension activation for
ordinary and selective imports, public re-export chains, conversion factories,
live receiver methods, iterators, standard-library imports, local overrides,
private isolation, duplicate/conflicting imports, explicit factories overriding
implied defaults in either import order, retained references and rollback.
The native and REPL `parent_imports.shy` tests cover structural `super` namespaces,
selected private types, ancestor and sibling imports, parent re-exports, shared
type identity, explicit conversion factories, direct child scripts, directory
preludes, private function isolation, and recovery after failed child imports.
`repl/prelude_reload*.shy` checks reload identity, scope isolation, cached dependencies,
and recovery after failed reloads. The native and REPL `prelude_startup.shy` tests
check explicit profile reloads and availability in later startup scripts.
The native and REPL `bundled_std.shy` tests check library access from a copied
executable, bundled diagnostics and exports, reloads and retained references,
and explicit paths disabling the bundled fallback. Bundled implementation checks
cover the `bundled` search entry and `$SHELLY_STD_FOUND` across reloads, failures,
and retained module scopes. Default fallback checks run when the corresponding
system modules are absent, since system copies take precedence and may expose
different interfaces.

Every driver receives a fresh copy of `fixtures/`, a separate HOME and TMPDIR,
and a replacement environment. Child cases get `fixtures/home` as HOME. Fixtures
include executable Shelly helpers; `bin/shelly` is linked to the binary under test.
A driver has a 30-second watchdog and ordinary nested cases have a 3-second
watchdog. PTY reads use bounded waits. Temporary directories are removed after
normal success or failure. Forced termination can leave directories behind;
subsequent runs create independent directories and do not delete abandoned ones.

`native/pipelines.shy` and `repl/pipelines.shy` check structured stages, iterator
sources with private state and frozen methods, final array collection, automatic
input, typed sinks, per-row streaming, frozen conversion factories, JSON and
CSV/TSV/SSV records, typed headers, byte preservation, backpressure, early exits,
aggregate status, redirections, diagnostics, and recovery across submissions.

`native/type_name.shy` and `repl/type_name.shy` cover represented type names,
collection metadata, reflection, String type checking, and read-only recovery.

`native/collection_constructors.shy` and `repl/collection_constructors.shy` cover
typed empty array/hash expressions, grouped type objects, metadata retention,
mutation constraints, numeric promotion, pipelines, and recovery after rejected writes.

The suite targets Linux/WSL and macOS.
It uses `mktemp`, `find`, `cp`, `ln`, `mkdir`, `rm`, `test`, `printf`, `cat`, `cmp`,
`sleep`, `printenv`, and `kill`. Individual language cases also exercise ordinary
Unix commands. `$os` identifies the host operating system for platform-specific
test helpers; `[when $os == "macos"]` declarations select the macOS helpers and
temporary-path normalization when the runner and drivers are compiled.
Cleanup and interrupted-runner checks execute on both platforms:
Linux checks `/proc`, while macOS uses `ps` to verify that children were reaped.
`native/process_groups.shy` checks descendants that ignore SIGTERM and terminal
hangup, and `harness/diagnostics.shy` checks portable diagnostic regex matching.
The native and REPL platform checks cover `$os` in scripts, stdin, command-line
source, functions, and interactive sessions, including conflicting environment values.
`/bin/sh` supplies process-ID and descendant fixtures; assertions and orchestration
remain Shelly code. There is no dependency on Python, pexpect, or an external
timeout helper. Sandboxed hosts must permit process inspection and PTY creation
to run the complete suite; denied inspection is a failure, not a passing skip.

`native/language_intelligence.shy` exercises LSP hovers and completions, module
visibility, properties and raw storage, named constructors, imported source edits,
external command signatures, Markdown layout, and UTF-16 cursor positions.

`native/call_stacks.shy` checks ordered runtime traces, recursive and anonymous
calls, module source locations, argument and return errors, property accessors,
constructors, native failures, and pipeline conversion context.
`repl/call_stacks.shy` checks recovery across separate interactive submissions.
