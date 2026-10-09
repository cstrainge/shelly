# shelly

A Unix-style shell written in Rust, growing toward a language for working with
commands, structured data, and network services in the same place.

<p align="center">
  <img src="./Shelly.png" alt="Shelly" width="50%">
</p>

Shelly is early work. Development of the shell is already being done in the shell:
running builds, using development tools, and trying new features from its prompt.
The examples below describe the current implementation. The
[language audit](audit/language/REPORT.md) records working features, known defects,
and the results of the repairs following that audit.

## Build and run

Use a Rust toolchain supporting edition 2024. The current audit was run on Linux.

```sh
git clone https://github.com/cstrainge/shelly.git
cd shelly
cargo build --locked
cargo run --locked
```

Shelly supports interactive use, script files, command-line source, and stdin:

```sh
./target/debug/shelly                         # Interactive when attached to a terminal
./target/debug/shelly example.shy one two     # Execute a file
./target/debug/shelly -c 'echo $args...' one two
printf 'echo "hello"\n' | ./target/debug/shelly -s
```

Without a file or `-c`, noninteractive input is read from stdin. `-i` forces the
REPL. `$args` is an array of the supplied arguments, excluding the script filename;
`$args...` passes its elements as separate arguments. Shell options go before the
script filename or script arguments. Use `--help` for all options.

In the REPL, Ctrl+Enter or Shift+Enter inserts a newline for multiline input;
Enter submits the buffer. Definitions and variables persist between submissions.
Leave with `exit` or Ctrl+D. Tab completes a unique name or common prefix; a
second Tab opens the completion menu. Arrow keys navigate an open menu.

Interactive startup loads `~/.shelly_init.shy`. `--rcfile PATH` selects another init
file; `--norc` skips it. `-l` enables login startup, which loads
`/etc/shelly/profile.shy`, then `~/.shelly_profile.shy`, before interactive init.
`--norc` does not disable login profiles. Script, `-c`, and stdin modes skip
interactive init. `-b` suppresses the banner; `-m` requests monochrome output.

## Statements, values, and arithmetic

Commands use space-separated arguments. Newlines and semicolons separate
statements; `#` starts a comment. A backslash followed by a newline continues a
logical source line, including inside arithmetic and return expressions. Put spaces
around binary arithmetic operators; braces may touch the expressions they enclose.

Declare variables with `let`, reference them with `$name` or `${name}`, and assign
an existing variable with `$name = expression`:

```text
let $x = 1024 + 2 * 2
echo $x                     # 1028
$x = ($x - 4) / 2
echo $x                     # 512
```

Variable names cannot contain `=`, including braced names and function parameters.
Keep spaces around assignment `=`; adjacent `==` and `!=` remain comparisons.

Values include signed 64-bit integers, floating-point values, booleans, strings,
arrays, the no-value result displayed as `()`, and external command statuses such
as `ExecResult(0)`. Arrays currently come from `$args` and file globs; array
literals and indexing are not implemented. Without `...`, an array becomes
colon-separated text when passed to a command. `()` is also a literal that
evaluates to `None`, including in assignments and returns.

Use `$args...` or `${args}...` to expand command arguments. A splat cannot be
the executable: `$cmd... 2` is a parse error; use `$cmd 2` to call a stored command.
Bracket-containing names such as `$args[0]` currently refer to a variable with
that exact name, not an array element; this syntax will be revisited with indexing.

Arithmetic supports `+`, `-`, `*`, `/`, and `%`, with normal precedence,
left associativity, and parentheses. **Operations currently convert operands to
integers**: `7 / 2` produces `3`, and `2.9 + 1.9` produces `3`. Numeric strings
convert to integers; a string that cannot be parsed as an integer converts to
zero. Boolean operands convert to `1` or `0`. This is not floating-point arithmetic.
Signed numbers and unary minus work: `-2 + 3` produces `1`, and `-(2 + 3)` produces
`-5`. Division/remainder by zero and integer overflow produce language errors,
including in release builds.

Expressions can stand alone, including inside functions. A top-level expression
is evaluated without automatically printing its value; use `echo` to display it.

## Boolean expressions

`==` and `!=` compare values and produce booleans. Numbers compare numerically,
including integer/float pairs; strings compare their text, ignoring executable
flags. Float source spelling does not affect equality. Arrays compare their
elements in order. Unrelated types are unequal: `"1" == 1` and `true == 1` are
false. `()` equals `()`. Command statuses compare as statuses, not as integers
or booleans.

`!`, `&&`, and `||` convert their operands to booleans:

| Value | Boolean conversion |
| --- | --- |
| `()` | False |
| Boolean | Its existing value |
| Integer or float | False for zero, true otherwise |
| String | False for empty text, exact `"false"`, or text parsing as numeric zero; true otherwise |
| Array or argument expansion | False when empty, true otherwise |
| External command result | True for exit status 0; false for nonzero status or termination by signal |

Logical operators always return a boolean. `&&` skips its right operand when
the left is false; `||` skips it when the left is true. Skipped operands have no
side effects and cannot cause runtime errors, but must still be valid syntax.

Precedence, highest first: parentheses; unary `!` and unary minus; `* / %`;
`+ -`; `== !=`; `&&`; `||`. Binary operators at the same precedence associate
left to right. Boolean operators do not require surrounding spaces, so
`$x!=0` and `!$x` work. Quote operator text when passing it literally.

```text
echo (1 == 2) (2.5 == 2.50)    # false true
echo !"false" !"0"             # true true
let $ready = 2 + 3 == 5 && !false
echo $ready                    # true
echo (false && (1 / 0))         # false; division is skipped
echo ((/usr/bin/false) || (/usr/bin/true))  # true
```

Use parenthesized calls to make command results operands: `(foo 3) && (bar 4)`.
These are value expressions, not shell command chains: `echo true && false`
passes the single value `false` to `echo`. Bare words within boolean expressions
are string operands; `foo == foo` compares text. Function parameters currently
arrive as strings, so `!$parameter` converts that text, while `$parameter == "3"`
compares it without numeric coercion.

Short-circuit compilation emits `ToBoolean`, `JumpIfFalse` (for `&&`) or
`JumpIfTrue` (for `||`), the right operand, `ToBoolean`, and a labeled
`JumpTarget`. Before linking, an optimization pass removes adjacent `PopResult`
and `PushResult` pairs, leaving the value on the stack. It never removes a pair
across a jump target. A second pass removes `CheckResult` only when an earlier
`PushResult` or `CheckResult` proves the result is empty and intervening instructions
preserve that state. Instructions that can set a result or control-flow boundaries
invalidate the proof; entry state is treated as unknown.
A link phase then resolves labels to instruction indexes independently
for each function and top-level code vector, rejecting missing or duplicate
labels. It removes labels from the target markers. The VM sees only numeric
jump destinations and no-op target instructions; jumps preserve the result and
value stack. Conditional expressions use unconditional `Jump` instructions to
skip the remaining branches after a match.

## Strings and paths

Double-quoted strings interpolate `$name` and `${name}`. Single-quoted strings
keep variable references literal. Missing variables are errors. Both quote forms
process backslash escapes, including `\n`, `\r`, `\t`, hexadecimal `\x41`, octal
`\o101`, and decimal `\065` (the last three produce `A`). Use `\$` inside double
quotes to keep a dollar sign literal: `"\$name"` produces `$name`. This also works
in multiline strings.

```text
let $name = 'Shelly'
echo "Hello, $name!"
echo "Building ${name}..."
echo '$name stays literal here'
```

Multiline strings use `"* ... *"` or `'* ... *'`. Leading whitespace before the
first text is skipped, and the first line establishes the indentation removed
from subsequent lines. Extra indentation and embedded newlines are preserved,
including the newline before a closing delimiter on its own line.

```text
let $project = 'Shelly'
echo "*
    Building $project
      Source: src/
      Mode: development
    *"
```

The double-quoted form interpolates variables; the single-quoted form keeps them
literal. Ordinary single-line quotes cannot contain a raw newline.

An unquoted source word beginning with `~` or `~/` expands to the home directory.
Quoted tildes and tildes obtained from variables remain literal. Variables,
interpolated strings, and command arguments retain real filesystem paths, so both
`cd $p` and `cat "$p/file"` work with a stored absolute path. Shelly shortens home
paths to `~/` in its own display formatting, such as the prompt; an external
command like `echo $p` receives and prints the real path.

Unquoted paths can begin with a variable. Its value and the suffix remain one
argument, including spaces in the value:

```text
let $root = '/tmp'
echo $root/project/file.txt
```

Variable-prefixed executable paths such as `$tools/echo` also work. Write
`$a / $b` for division; `$a/file` is a path. Variable-prefixed glob patterns
such as `$root/*.txt` interpolate the prefix before expanding the pattern.
Characters from the variable's value remain literal, including `[` or `*` in a
directory name.

Unquoted `*`, `?`, and bracket patterns such as `[ab]` expand matching paths in
sorted order; `**` supports recursive matching. Hidden entries require an explicit
leading dot, `.` and `..` are excluded, and no matches is an error. Quotes preserve
a glob as text.

```text
let $sources = src/language/*.rs
echo $sources...
```

## Functions, calls, and return values

Functions have named parameters and local variable scopes. Call them like
commands. Arguments are evaluated left to right, then **converted to text** for
the callee; numeric types and executable markers do not survive parameter binding.

```text
fn foo($a)
{
    2048 * $a
}

let $y = foo 3
echo $y                     # 6144
echo (foo 3)                # 6144
echo (foo 3) + 1            # 6145
```

The final expression or command supplies the function's result. A trailing
semicolon or newline does not discard it. `return expression` exits the current
function immediately with that value; bare `return` returns `()`.

```text
fn answer()
{
    return 2048
    echo "unreachable"
}

fn greet($name)
{
    echo "Hello, $name!"
    return
}
```

`return` outside a function is an error. Empty functions, including `fn f() {}`,
and functions ending in a declaration, assignment, alias, or nested function
definition return `()`. Duplicate parameter names are rejected.

Parentheses evaluate one expression and preserve its result. They support nested
calls and arithmetic, but do not contain statement sequences. Missing or extra
closing parentheses are errors, including `echo foo 3)`.

A bare name has different behavior depending on its context:

| Form | Current behavior |
| --- | --- |
| `foo` as a statement | Call `foo` with no arguments; an unknown command errors. |
| `let $x = foo` | Call it if it resolves to a function, builtin, alias, or executable; otherwise store the word as text. |
| `let $x = foo a b` | Call it with arguments; an unknown command errors. |
| `echo foo` | Call `foo` with no arguments if it resolves, then pass its result to `echo`; otherwise pass the word. |
| `echo (foo 3)` | Call `foo` with `3`, then pass its result directly to `echo`. |
| `echo "foo"` | Pass literal text. |

A backtick prefix creates a string marked executable and stores the name without
calling it. There is no closing backtick. The name is resolved when invoked; it
is not a captured function object.

```text
fn answer() { 2048 }
let $call = `answer
let $copy = $call            # Copy the reference without calling it
$call                       # Call it; top-level values are not printed
echo "$call"                # answer
echo $call                  # 2048
echo ($call)                # 2048
echo `answer                # answer
```

A standalone variable, a variable command argument, or a variable/string inside
parentheses is called with no arguments when its value is marked executable.
Ordinary string values stay text. Direct call results and nested groups do not
cause the returned value to be called a second time.

A backtick argument suppresses the automatic call for that argument. Grouping
changes this: ``echo (`answer)`` calls `answer`. A backtick-prefixed name cannot
be the head of a grouped call with arguments: ``echo (`foo 3)`` is an error.
To pass a stored reference's name without calling it, use `"$call"` or
`` `$call ``. The backtick-variable form reads the value and marks its name
executable, so it can also create a reference from a stored ordinary string.

Calls through variables with arguments work as statements, in assignments and
returns, and inside parentheses:

```text
let $call = `foo
let $result = $call 3
echo ($call 4)              # 8192
```

An explicit call with arguments also accepts an ordinary string variable as its
command name. The executable marker controls implicit zero-argument calls; an
explicit call does not require that check.

Function definitions are registered before executing the submitted source, so
forward calls work. The last definition of a name in that source wins even for
earlier calls. Functions can contain helper functions:

```text
fn welcome($name)
{
    fn say_hello()
    {
        echo "Welcome to Shelly, $name!"
    }
    say_hello
}
welcome 'world'
```

Variable lookup searches active block and call scopes; assignment updates the nearest
visible binding. This currently gives variables dynamic caller scope. Nested
function names follow their containing function blocks. `let` creates or replaces
a binding in the current scope. The initializer runs before the binding is
replaced, so `let $x = $x + 1` can read the old value. An initializer error does
not overwrite the binding with an empty value.

## Scoped code blocks

Standalone `{ ... }` blocks create variable scopes and may be nested, both at the
top level and inside functions. `let` creates a binding local to the block;
assignment without `let` updates the nearest visible binding.

```text
let $x = 1
{
    let $x = $x + 1
    { let $x = 3; echo $x }  # 3
    echo $x                 # 2
}
echo $x                     # 1
{ $x = 4 }
echo $x                     # 4
```

The compiler emits `EnterScope` and `ExitScope` around each block. Returns and
runtime errors also unwind any active block scopes. `return` inside a block exits
the enclosing function; it remains an error at the top level. A block at the end
of a function supplies its last expression as the implicit return value, including
through nested blocks. An empty final block, or one ending in a declaration,
supplies `()`.

Blocks are statements; `{ ... }` is not yet an expression for assignments or
command arguments. Blocks scope variables; function definitions retain their
existing hoisting into the enclosing function or top level, and aliases remain
global.

## Conditional expressions

`if condition { ... }`, `else if condition { ... }`, and `else { ... }` form a
chain. Every branch requires a block with its own variable scope. Conditions use
the same boolean conversion as `!`, `&&`, and `||`. They are evaluated in order;
only the first matching branch runs, and later conditions are skipped. Newlines
and comments may separate a condition from its block or one branch from the next.

```text
let $count = 2
let $message = if $count == 0
{
    "empty"
}
else if $count == 1
{
    "one item"
}
else
{
    let $description = "several items"
    $description
}
echo $message               # several items
echo (if true { 10 } else { 20 }) + 1   # 11
```

`if` is an expression: use it in assignments, arguments, arithmetic, returns,
or other conditions. Its value is the selected block's last expression. An empty
block, a block ending in a declaration, or an unmatched chain without `else`
evaluates to `()`. A final `if` expression supplies a function's implicit return
value. `return` inside a branch still exits the enclosing function, unwinding
the branch scope.

Conditions also accept command calls, for example
`if /usr/bin/true { echo "success" }`. Exit status zero is true; nonzero or
signaled results are false. Use parentheses when combining calls with operators:
`if (check 3) && (check 4) { ... }`. The condition is evaluated in the surrounding
scope; branch-local bindings do not escape. Branch blocks follow the function
hoisting and alias rules described above.

All branches must parse, including those skipped at runtime. `else` must belong
to the same chain; a semicolon ends the chain, so put `else` after the closing
brace or on the next line, without a separating semicolon. To pass the literal
word `if` as a command argument, quote it.

## Processes, aliases, and environment

`cd PATH` changes directory. `exit` stops execution with status zero;
`exit 7` stops it with status 7. An explicit exit status must be an integer from
0 to 255. `echo` and the other Unix
commands in these examples are external programs found through `PATH`.

External commands return an `ExecResult` status. Their stdout is inherited by
Shelly; assigning a command result does **not** capture its printed output.

```text
let $status = /usr/bin/false
echo $status                # ExecResult(1)
```

Assignment can retain a failed status as a value. An uncaptured failing command
stops the remaining submitted source; an intermediate failing command also stops
a function. The interactive REPL reports the error and accepts another input.
A failing noninteractive script exits unsuccessfully.

Aliases prepend fixed arguments. Alias arguments are stored literally, without
variable interpolation or automatic calls. Aliases are global even when declared
inside a function. A newline, semicolon, or closing function brace ends an alias
definition.

```text
alias say = echo "prefix"
say 'hello'                 # prefix hello
```

Resolution expands aliases, then checks builtins, functions, and external
programs, in that order. An alias can add defaults to its own command name;
indirect alias cycles produce an error.

Shelly imports the environment. New variables are private unless declared with
`let export`; only exported variables reach child processes.

```text
let export $SHELLY_PROJECT = 'shelly'
```

Useful predefined variables include `$args`, `$pwd`, `$HOSTNAME`, `$HOME`, `$PATH`,
`$shelly` (an executable reference to this binary), `$version`, `$OS`,
`$build_date`, `$build_time`, `$interactive`, `$login`, and `$rc_path`.
`$rc_path` is the configured init path, `<not found>` when missing, or
`<unloaded>` when init loading is disabled.

## Current limitations

The [audit report](audit/language/REPORT.md) records current checks and unresolved
findings. The [temporary Python suite](audit/language/README.md) runs with
`python3 audit/language/run_suite.py` (add `--release` for the optimized build).
It exits nonzero while known mismatches remain. Its cases will migrate to Shelly
under root `test.shy` when the required language support exists.

The language still has deliberate limits:

- Arithmetic converts operands to integers rather than preserving floating-point values.
- Function arguments become text, losing their original types and executable markers.
- Variables use dynamic caller scope; function definitions are hoisted within each input.
- Arrays come from arguments and globs; literal collections and indexing are still planned.
- Command results are statuses, not captured stdout. Shell errors in noninteractive
  execution return a general failure status; only explicit `exit N` selects a specific status.
- Some parser diagnostics still contain verbose lists of attempted alternatives.

Malformed declarations, missing statement separators, duplicate parameters, and
invalid UTF-8 source now produce errors. Arithmetic errors no longer panic the
shell. The audit covers language behavior on Linux; it is not exhaustive testing
of terminal editing or other operating systems.

## Where Shelly is going

The aim is to keep the immediacy of a shell while giving larger scripts a clear
path to structure. A command that is convenient at the prompt should also be
useful inside a function or a longer program.

- **Optional typing.** Add annotations and contracts where they help document
  intent and catch mistakes while keeping small interactive tasks lightweight.
- **Network and JSON support.** Work with remote services and structured values
  directly from commands and functions.
- **Pipelines for text and structured data.** Connect Unix tools with commands
  that consume and produce structs, with conversions at command boundaries.
- **Control flow and collections.** Loops, ordering
  comparisons (`<`, `>`), array literals, and indexing are not yet implemented.

The draft [test.shy](test.shy) sketches how Shelly could test itself by discovering
scripts, looping over them, inspecting results, and reporting failures. It is a
design sketch, not a runnable test suite. Script execution and command results as
values already work; the draft's control flow and richer data operations remain
future work.
