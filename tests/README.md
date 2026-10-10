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

## Process case records

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

`native/` checks string methods, process results/settings, exact binary I/O,
redirection, live exports, terminal identity/lifecycle, UTF-8 boundaries, deadlines,
process-group cleanup, and Ctrl+C. `harness/` tests schema/result validation,
catalog/matrix completeness, miniature suites exercising the actual runner, and Ctrl+C against a frozen suite.
These tests deliberately run failing children and assert that they fail correctly.
`native/iterator_protocol.shy` and `repl/iterator_protocol.shy` check user and native
iteration, private state, method versions, destructuring, unit termination, fixed-length
array annotations, and recovery after iterator errors.
`native/visibility.shy` and `repl/visibility.shy` check native and scripted exports,
private bindings, re-exports, callable identity, diagnostics, and REPL recovery.
`repl/prelude_reload*.shy` checks reload identity, scope isolation, cached dependencies,
and recovery after failed reloads. The native and REPL `prelude_startup.shy` tests
check explicit profile reloads and availability in later startup scripts.

Every driver receives a fresh copy of `fixtures/`, a separate HOME and TMPDIR,
and a replacement environment. Child cases get `fixtures/home` as HOME. Fixtures
include executable Shelly helpers; `bin/shelly` is linked to the binary under test.
A driver has a 30-second watchdog and ordinary nested cases have a 3-second
watchdog. PTY reads use bounded waits. Temporary directories are removed after
normal success or failure. Forced termination can leave directories behind;
subsequent runs create independent directories and do not delete abandoned ones.

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
