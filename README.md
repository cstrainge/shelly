# shelly

A Unix-style shell written in Rust, growing toward a language for working with
commands, structured data, and network services in the same place.

<p align="center">
  <img src="./Shelly.png" alt="Shelly" width="50%">
</p>

Shelly is early work. The language and its implementation are still taking shape,
but development of the shell is already being done in the shell: running builds,
using development tools, and trying new features from Shelly's own prompt.

## Running Shelly

Use a Rust toolchain that supports edition 2024, on Linux or WSL:

```sh
git clone https://github.com/cstrainge/shelly.git
cd shelly
cargo run --locked
```

The binary currently launches the interactive shell; it does not yet accept a
script file to execute. Use Ctrl+Enter or Shift+Enter to insert a newline for
multiline input. Leave with `exit` or Ctrl+D.

## The language today

Shelly has an interactive prompt, runs external programs, and includes `cd` and
`exit` built-ins. Commands use familiar shell syntax, with arguments separated by
spaces. Newlines and semicolons separate statements, and `#` starts a comment.

Variables are declared with `let` and referenced with `$`. Values include strings,
integers, floating-point numbers, booleans, and arrays. Variables can be reassigned;
arithmetic currently uses integer conversion and supports `+`, `-`, `*`, `/`, `%`,
precedence, and parentheses. Use spaces around arithmetic operators.

```text
let $x = 1024 + 2 * 2
echo $x                     # 1028
$x = ($x - 4) / 2
echo $x                     # 512
```

Double-quoted strings interpolate `$name` and `${name}`. Single-quoted strings
keep those references literal. Missing variables produce an error.

```text
let $name = 'Shelly'
echo "Hello, $name!"
echo "Building ${name}..."
echo '$name stays literal here'
```

Multiline strings use `"* ... *"` or `'* ... *'`. Leading whitespace before the
first text is skipped, and that first line establishes the indentation removed
from subsequent lines. Extra indentation is preserved, so the source can stay
neatly indented without adding that indentation to the output.

```text
let $project = 'Shelly'
echo "*
    Building $project
      Source: src/
      Mode: development
    *"
```

Prints:

```text
Building Shelly
  Source: src/
  Mode: development
```

The double-quoted form interpolates variables; the single-quoted form keeps them
literal:

```text
echo '*
    Variables such as $project and ${project}
    stay literal in this string.
    *'
```

Newlines inside the string are retained, including the one before a closing
delimiter on its own line.

Unquoted paths can start with a variable. The expanded value and the path suffix
stay together as one argument, including when the variable contains spaces.

```text
let $root = '/tmp'
echo $root/project/file.txt
# Prints: /tmp/project/file.txt
```

This also works in assignments, function arguments, and executable paths such as
`$tools/echo`. Write `$a / $b` for division; `$a/file` is a path.

File globs expand into arguments. Hidden entries require an explicit leading dot,
`.` and `..` are excluded, and a pattern with no matches is an error. A glob can
also be stored in a variable and expanded later with `...`:

```text
let $sources = src/language/*.rs
echo $sources...
```

Functions have named parameters and local variable scopes. Call them like other
commands; arguments currently arrive as text. Function return values and early
return are not supported yet.

```text
fn greet($name) {
    echo "Hello, $name!"
}

greet 'world'
```

Functions can also contain helper functions. A nested function can use the outer
function's variables when called from it:

```text
fn welcome($name) {
    fn say_hello() {
        echo "Welcome to Shelly, $name!"
    }

    say_hello
}

welcome 'world'
# Prints: Welcome to Shelly, world!
```

Shelly imports the environment when it starts. Use `let export` to make a new
variable available to child processes:

```text
let export $SHELLY_PROJECT = 'shelly'
```

## Where Shelly is going

The aim is to keep the immediacy of a shell while giving larger scripts a clear
path to structure. A command that is convenient to type at the prompt should also
be useful inside a function or a longer program.

- **Optional typing.** Start with values and simple commands, then add type
  annotations and contracts where they help document intent and catch mistakes.
  Small interactive tasks should stay lightweight.
- **Full network and JSON support.** Working with remote services, making
  requests, and reading or producing JSON should be natural parts of the language.
  Network responses should be available as structured values that commands and
  functions can work with directly.
- **Pipelines that connect text and structs.** Pipelines should automatically
  convert between text and structured values as data crosses command boundaries.
  Existing Unix tools should participate alongside commands that consume and
  produce structs. Formats and types should guide conversion, with clear errors
  when the data cannot be converted.

These are design goals, not implemented features yet. Optional type annotations,
structs, native network and JSON facilities, and pipelines are still ahead. The
current shell provides the foundation and a place to try the language as it grows.

The draft test runner in [test.shy](test.shy) makes that direction concrete: Shelly
should be able to test itself with scripts written in its own language. It sketches
collecting and sorting test files, counting them, looping over them, checking
execution results with `if` / `else`, and reporting successes and failures.

For example, this excerpt shows the intended flow from file discovery to execution
and a readable result:

```text
let $must_succeed = sort ./tests/must_succeed/*.shy
let $success_count = 0

for $file in $must_succeed
{
    let $result = $file

    if $result
    {
        $success_count = $success_count + 1
    }
    else
    {
        echo "$file failed."
    }
}
```

This is a design sketch, not a runnable example in the current shell. Command
results used as values, script execution, loops, and conditionals are part of the
intended language. The full draft also brings together comparisons, boolean logic,
multiline status messages, and an exit status for the test run. It is the kind of
everyday automation that should remain easy to read as Shelly gains richer data
and types.
