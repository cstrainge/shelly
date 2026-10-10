# Shelly

A Unix-style shell written in Rust, growing toward a language for working with
commands, structured data, and network services in the same place.

<p align="center">
  <img src="./Shelly.png" alt="Shelly" width="50%">
</p>

Shelly is early work. Development of the shell is already being done in the shell:
running builds, using development tools, and trying new features from its prompt.
The examples below describe the current implementation.

## Build and run

Shelly runs on Linux (including WSL) and macOS. Build with a Rust toolchain
supporting edition 2024.

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
Ctrl+Enter and Shift+Enter require a terminal that reports those key combinations.

Interactive startup loads `~/.shelly_init.shy`. `--rcfile PATH` selects another init
file; `--norc` skips it. `-l` enables login startup, which loads
`/etc/shelly/profile.shy`, then `~/.shelly_profile.shy`, before interactive init.
`--norc` does not disable login profiles. Script, `-c`, and stdin modes skip
interactive init. `-b` suppresses the banner; `-m` requests monochrome output.

Define `fn prompt() { ... }` in the init file to customize the prompt. Shelly uses
its printed stdout as the prompt text and falls back to the default prompt if
the call fails.

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

Use `let _ = expression` to evaluate an expression once and discard its value
without creating a variable. This also consumes a command's exit status, just as
assigning it to a variable does; evaluation errors still propagate. Printed output
is unaffected. The discard requires an initializer and has no type annotation or
`export` modifier. `let $_ = expression` remains an ordinary named binding.

An optional `: Type` annotation constrains a variable's initializer and subsequent
writes. Type names are case-sensitive: use `Number`, not `number`.

```text
let $count: Number                 # Defaults to 0
let $label: String = "items"
let $values: [Number] = [1, 2.5]
let $lookup: [String: Number]
let $maybe: optional Number        # Defaults to ()
$count = 3.5
$lookup["count"] = $count
```

Annotated declarations may omit `= expression`. Defaults are:

| Type | Default |
| --- | --- |
| `Number`, `Integer`, `Float` | Zero of the appropriate numeric type |
| `Boolean` | `false` |
| `String` | Empty string |
| `[T]`, `Array` | `[]` |
| `[K: V]`, `HashMap` | `[:]` |
| `optional T`, `None`, `any` | `()` |
| `Range` | `0..0` |
| `ExecResult` | `ExecResult(0)` |
| `ArgumentExpansion` | Empty argument expansion |

Structs and enums require an initializer unless wrapped in `optional`; empty
containers of those types need no initializer. `let $x` without a type or an
initializer remains an error. Annotations use the same types and nested container
constraints as struct fields. `Number` accepts integers and floats, but does not
coerce strings or booleans. Failed initializers and writes preserve the previous
binding, including its constraint. Indexed and field writes validate the updated
value before committing it.

Unannotated bindings remain dynamically typed. A new `let` declaration may replace
an existing binding and its annotation; inner declarations shadow outer bindings.
An `Integer` is promoted to `Float` wherever a Float is required: declarations,
assignments, parameters (including variadic parameters), return values, struct
fields, and typed collection elements or values. Promotion applies recursively
through optional types and nested collections. Strings and booleans are not
implicitly promoted. `Number` preserves whether its value is an Integer or Float.
Other typed bindings preserve the assigned value's type, including
`ArgumentExpansion`; untyped assignments retain their expansion-to-array conversion.
A Float never implicitly narrows to Integer; use `Integer(...)` explicitly.
That checked conversion rejects fractional and out-of-range values.

```text
let $x: Float = 7
echo ($x / 2)              # 3.5
$x = 9
echo ($x / 2)              # 4.5
let $n: Integer = Integer(8.0)
echo ($n / 2)              # 4
```

Values include signed 64-bit integers, floating-point values, booleans, strings,
arrays, hash maps, ranges, enums, structs, the no-value result displayed as `()`, and external
command statuses such as `ExecResult(0)`. Arrays come from literals, `$args`, and
file globs. Without `...`, an array becomes colon-separated text when passed to
a command. `()` is also a literal that evaluates to `None`, including in assignments
and returns.

Use `$args...` or `${args}...` to expand command arguments. A splat cannot be
the executable: `$cmd... 2` is a parse error; use `$cmd 2` to call a stored command.

Arithmetic supports `+`, `-`, `*`, `/`, and `%`, with normal precedence,
left associativity, and parentheses. Integer operands produce Integer results:
`7 / 2` produces `3`, truncating toward zero. If either operand is a Float, the
other numeric operand is promoted and the result remains a Float: `7.0 / 2`,
`7 / 2.0`, and `7.0 / 2.0` all produce `3.5`. This applies to all five operators;
`2.5 + 1` produces `3.5`, and `7.5 % 2` produces `1.5`.

When both operands are Strings, `+` concatenates them: `"Hello, " + "world!"`
produces `"Hello, world!"`, and `"7" + "2"` produces `"72"`. The result is plain
string data, even when an operand carries an executable marker.

All other arithmetic requires numeric operands. Strings (including numeric text),
booleans, arrays, maps, ranges, structs, enums, command statuses, and `()` produce
errors. Use an explicit numeric conversion when needed, such as `Integer("7") + 2`.
Signed numbers and unary minus work: `-2 + 3` produces `1`, and `-(2.5 + 3)`
produces `-5.5`. Division/remainder by zero, integer overflow, and non-finite
floating-point operands or results produce language errors, including in release builds.

Expressions can stand alone, including inside functions. A top-level expression
is evaluated without automatically printing its value; use `echo` to display it.

## Arrays

Create an empty array with `[]` or use comma-separated expressions:

```text
let $a = [10, 20 + 2, "three", [4, 5]]
echo $a[1]                   # 22
$a[1] = 99
$a[3][0] = 40
echo $a[3]...                # 40 5
echo [7, 8][0]               # 7
```

Elements evaluate left to right and retain their types. Nested arrays remain
nested. Newlines, comments, and a trailing comma are allowed between elements.
Each element accepts the same value expressions as an initializer, including
calls and conditionals: `[foo 3, if true { 1 } else { 2 }]`.

Indexes are zero-based integers. `$a[$i + 1]`, `${a}[0]`, `(make_array)[0]`,
and chained `$a[1][2]` reads work. The opening index bracket must touch its
value: `$a[0]` is an element, while `$a [0]` supplies a separate array argument.
Negative, out-of-range, or non-integer indexes (including `"0"` and `1.0`)
produce errors for arrays. Only arrays and maps support indexing. Array writes
replace existing elements; they do not append or grow an array. Numeric function
arguments retain their types; use `Integer($index)` to convert an explicitly textual
index, such as one read from `$args`.

Indexed assignment must start with a variable, as in `$a[0] = value` or
`$a[0][1] = value`. It updates the nearest visible binding. Index expressions
evaluate once, left to right, followed by the right-hand expression; the complete
path is checked before replacing the element. Evaluation side effects remain if
a later check fails. Assigning or reading an array copies its value: changing
`$b` after `let $b = $a` does not change `$a`.
Internally, arrays and argument expansions share reference-counted storage;
indexed writes copy shared arrays only along the modified path.

Use `...` to spread an array into arguments or another literal:

```text
let $a = [2, 3]
let $b = [1, $a..., 4]        # [1, 2, 3, 4]
echo $b...                    # 1 2 3 4
```

Indexed executable strings follow the same call rules as executable variables:
`$commands[0] 3` calls with an argument; `$commands[0]` invokes a marked executable
in statement or command-argument position. Prefix the access with a backtick to
pass its name without invoking it. Indexing is expression syntax; quoted string
interpolation still supports variable names rather than arbitrary expressions.

An array cannot be a command. A standalone `$a` or `$a[0]` whose value is an array
and is discarded reports `Cannot execute an array as a command`; explicit calls
such as `$a "argument"` reject arrays too. Arrays remain valid as function return
values, conditional results, and arguments (`echo $a` or `echo $a...`).

A leading `[` starts an array or map literal. For bracket globs use a path prefix,
such as `./[ab].txt` or `fixtures/[ab].txt`; quote brackets to pass literal text.

## Builtin type methods

Methods use member access and ordinary shell-style arguments. Arrays and argument
expansions (including glob results) provide these methods:

| Method | Result |
| --- | --- |
| `$items.sort` | A new array sorted in ascending order; the receiver is unchanged |
| `$items.zip $other` | An array of two-element arrays, stopping at the shorter input |
| `$items.count` | The number of elements as an `Integer` |

```text
let $items = [3, 1, 2]
let $sorted = $items.sort
echo $sorted...                         # 1 2 3
echo $items.count                       # 3
let $pairs = $sorted.zip ["a", "b"]
echo $pairs[0][0] $pairs[0][1]           # 1 a
echo ($items.zip [4, 5]).count           # 2
let $files = (./src/*.rs).sort
```

Methods with no arguments run when accessed, including in assignments and chained
expressions such as `$items.sort.count`. Calls with arguments use spaces;
parenthesize a nested call as in `echo ($items.zip $other)`. Empty parentheses
are still a `None` argument, so use `.sort`, not `.sort()`.

Sorting is stable: equal values retain their relative order. Numbers sort
numerically, including mixed integers and floats without rounding large integers
before comparison. Strings sort lexicographically by their text, booleans put
`false` first, and variants of one enum sort by `.index`. Mixed categories,
different enum types, nonfinite numbers, and structured elements are rejected.
Empty arrays are valid. Sorting strings does not execute them.

Zip preserves element types and value semantics, including nested collections.
Its argument must be an array; explicitly expanded arguments still follow normal
command expansion rules. Use `Array($expansion)` to pass an argument expansion as
one array. For example, `[1, 2].zip ["a", "b"]` produces
`[[1, "a"], [2, "b"]]`. Iterate over those arrays with a destructuring pattern:

```text
for ($number, $letter) in $items.zip ["a", "b"]
{
    echo $number $letter
}
```

Use `for $pair in ...` to keep each two-element array as a single value.

Backtick access retains a callable method bound to its live receiver:

```text
let $sort = `$items.sort
$items[0] = 9
echo ($sort)...                         # 1 2 9, using the updated receiver
```

Methods are registered per type in the type registry. Adding a builtin method
does not require a new parser rule or a separate opcode for each method. Struct
fields and enum `.index` retain their existing behavior.

## User-defined type methods

Use `fn TypeName::method(...)` to extend any visible type, including builtin types,
structs, and enums. The method does not have to be declared alongside the type.
An implicit, typed `$self` parameter receives the value before the explicit
parameters. Do not declare `$self` in the parameter list.

```text
struct Item { quantity: Integer }

fn Item::add($amount: Integer): Item
{
    $self.quantity = $self.quantity + $amount
    $self
}

let $item = Item(quantity: 2)
let $updated = $item.add 3
echo $item.quantity $updated.quantity   # 5 5

fn Array::first($fallback: optional any): any
{
    if $self.count == 0 { $fallback } else { $self[0] }
}

echo [4, 5].first                       # 4
echo ([].first "empty")                 # empty

fn Integer::sum($rest: Integer...): Integer
{
    for $value in $rest { $self = $self + $value }
    $self
}

let $start = 10
echo ($start.sum 1 2 3)                 # 16
```

Methods use the same parameter annotations, trailing optional parameters, final
variadic parameter, return annotations, explicit `return`, and implicit final
result as ordinary functions. Calls use the same dot syntax as builtin methods.
`$self` is a mutable alias to the receiver. Assigning `$self` or updating its
members changes the caller's variable immediately; returning or reassigning the
result is not required. This also works through nested fields and array or map
indexes, such as `$items[0].add 3`. Receiver indexes are evaluated once.

Ordinary value copies and explicit function parameters remain independent.
Methods on literals, constructor results, or other temporary values mutate only
that temporary. Returned values are ordinary values, so chaining a method onto
a returned value operates on that result. Existing type constraints still apply
to mutations, including constraints on containing fields and collections.
Successful mutations remain visible if a later statement in the method fails.

Extensions on `Number` apply to both integers and floats; extensions on `any`
provide a fallback for all values. Methods on the concrete type take precedence,
followed by `Number` for numeric receivers, then `any`. A user method may replace
a builtin method on that type. Struct fields and enum `.index` take precedence
over fallback methods, and declaring a method directly on a type with the same
name as one of its data members is an error.

Methods follow ordinary function scoping and forward-declaration rules. Different
types may use the same method name, and method names do not occupy the namespace
of bare function calls. Already compiled calls retain their visible method
versions when a method is redefined. Backtick references such as
``let $add = `$item.add`` keep a live reference to the receiver binding and pin
the method version; `$add 3` updates `$item`. Reassigning the same binding is
visible through the reference, while declaring a new variable with `let` creates
a separate binding. Captured bindings remain alive after their scope exits.
References to collection elements retain their evaluated index or key; an invalid
path or incompatible receiver type produces an error when called. Redefining a
type creates a distinct type, so old values retain their original methods.

## Hash maps

Use `[key: value, key: value]` to create a map and `[:]` for an empty map.
`[]` remains an empty array. Keys and values are expressions, evaluated left to
right, key before value. Newlines and a trailing comma are allowed; array elements
and map pairs cannot be mixed in one literal. Quote literal string keys to avoid
the usual bare-word command lookup rules.

```text
let $myHash = ["key": 42, "items": [1, 2]]
let $x = $myHash["key"]          # 42
echo $myHash["missing"]          # ()
$myHash["new"] = 7              # Insert
$myHash["key"] = 99             # Replace
$myHash["items"][0] = 8         # Nested write
```

Any value can be a key, including arrays and maps:

```text
let $lookup = [[1, 2]: "array key", ["a": 1]: "map key"]
echo $lookup[[1.0, 2]]           # array key
echo $lookup[["a": 1.0]]         # map key
```

Keys compare by value. `1` and `1.0` identify the same key, while `"1"` is distinct.
String execution flags and float source spelling do not affect key identity;
array order matters and map entry order does not. Collection keys are immutable
snapshots: modifying the original array or map leaves its stored key unchanged.
NaN keys are canonicalized to one key so they can be looked up reliably. Duplicate
keys keep the last value, while all key and value expressions still execute.

Maps use reference-counted storage and copy-on-write, like arrays. Indexed writes
insert or replace the final key; missing intermediate containers cause an error.
A missing read returns `()` without inserting an entry. Maps compare by their
entries and convert to false only when empty. Arithmetic on maps is an error.
Text conversion produces a bracketed list of key/value pairs in a
stable order, with quoted strings and bracketed nested collections. Passing a map
as an argument uses that text; `...` treats a map as one value and does not iterate
its entries. A map cannot be executed as a command.

Inside a collection literal, `:` separates map keys and values. Use parentheses
for a nested call that takes an unquoted colon argument, such as
`["result": (command :)]`.

## Ranges

Ranges hold integer bounds without allocating their elements:

| Syntax | Meaning |
| --- | --- |
| `1..5` | Start included, end excluded |
| `1..=5` | Both ends included |
| `1..` | Start specified, end omitted |
| `..5` or `..=5` | Start omitted, end excluded or included |
| `..` | Both bounds omitted |

Either bound can be a variable or an expression. Bounds evaluate once, left to
right, when the range is created; later variable assignments do not change it.

```text
let $start = 1
let $end = 4
let $r = $start..$end
echo $r                       # 1..4
echo $r...                    # 1 2 3
echo ($start..=$end)...        # 1 2 3 4
let $inner = ($start + 1)..($end - 1)
let $items = [0, $r..., 4]     # [0, 1, 2, 3, 4]
```

`...` expands a bounded range in ascending steps of one, materializing its
elements as arguments or array elements. Reversed ranges expand to no values;
`3..3` is empty and `3..=3` contains one value. Expansion is reusable and does
not consume the range. Parenthesize a literal before expanding it: `(1..4)...`.
Without expansion, a command receives the range's text, such as `1..4`.

Bounds must be integers; floats, numeric strings, and other types produce an
error. Use `Integer($start)` to explicitly convert a numeric string; integer function
arguments already retain their type. Inclusive ranges require an end bound, and chained
ranges such as `1..2..3` are rejected. Omitted bounds remain unspecified;
expanding such a range is an error. Range indexing and array slicing are not
implemented yet.

Ranges compare by their bounds and inclusivity: `1..3` differs from `1..=2`
even though they expand to the same elements. They can be map keys and function
return values, but cannot be commands. Boolean conversion is false for an empty
bounded range and true otherwise. Explicit numeric conversion of a range and
arithmetic on a range are errors.

Arithmetic binds more tightly than range operators, which bind more tightly than
comparisons. Spaces around `..` and `..=` are optional. Parenthesize open ranges
when followed by other arguments, as in `echo (1..) (..5)`. Ordinary words retain
embedded dots (`file..name`); quote text that would otherwise parse as a range.
Paths such as `./file`, `../file`, and `cd ..` continue to work.

## Enums

Enums define named alternatives. The current implementation supports unit variants:

```text
enum Color
{
    Red,
    Green,
    Blue,
}

let $color = Color::Green
echo $color                       # Color::Green
echo ($color == Color::Green)      # true
let $labels = [Color::Red: "stop", Color::Green: "go"]
echo $labels[$color]               # go
let $index: Integer = $color.index # 1
echo Color::Blue.index             # 2

fn is_green($value) { $value == Color::Green }
echo (is_green $color)             # true
```

Commas separate variants; trailing commas and newlines are allowed. Names contain
letters, digits, or underscores and cannot start with a digit or use a reserved
keyword. Empty enums, duplicate variants, and duplicate type declarations in the
same scope and submission are errors. Unit variants have no constructor arguments:
use `Color::Red`, not `Color::Red()`. Payload variants are not implemented yet.
Unit variants can be used as values in `match` arms.

Enum names are lexical: a declaration is available throughout its containing block,
including earlier expressions and function definitions. Inner declarations can
shadow outer types. The name does not escape its block, but returned or assigned
values retain their definition. Repeated calls and loop iterations reuse the same
compiled declaration's identity.

Enums compare by declaration identity and variant. A variant differs from its
printed string and from identically named variants of other declarations. Enums
can be array elements, map keys, function arguments, and return values. They always
convert to true, even if a variant is named `False` or `Error`. Arithmetic,
indexing, iteration, and using an enum as a command are errors. Expansion with
`...` passes one value; enum text such as `Color::Green` is display output, not
serialized source.

Every enum value has a read-only `.index` member: an `Integer` starting at zero
in declaration order. It works on variables, literal variants, and enum values
inside collections or struct fields. The index belongs to the value's original
declaration, so redeclaring an enum does not change existing values' indexes.
Use `.index` for numeric access; `Integer($color)` does not convert an enum directly.

The shared type registry persists across REPL submissions. Redeclaring a type in
a later submission creates a new identity; existing values and previously compiled
functions retain the old definition. Checking resolves enum names and variants in
the entire submitted AST, including unused functions and skipped branches, before
bytecode generation. A parsing, checking, or compilation failure does not publish
new types or functions. A runtime failure occurs after declarations are committed.

## Structs

Structs declare bare field names without `$`. A type annotation follows a colon:
`name: Type`. Omitting the annotation makes the field accept any value:

```text
enum Status { Ready, Busy }

struct Item
{
    quantity: Number,
    state: Status,
    children: [Item],
    labels: [String: String],
    next: optional Item,
    payload: any,
}

let $item = Item(
    quantity: 2,
    state: Status::Ready,
    children: [],
    labels: ["name": "widget"],
    payload: (),
)
echo $item.quantity $item.labels["name"]    # 2 widget
echo $item.next                            # ()
$item.quantity = 3.5
$item.labels["name"] = "updated"
```

Commas separate declarations and constructor arguments; newlines, comments, and
trailing commas are allowed. Declaration names and constructor labels omit `$`.
The opening parenthesis can touch the type name, be separated by spaces (`Item (quantity: 42, ...)`),
or follow it on a new line, including across blank lines and comments:

```text
let $item = Item
    (
        quantity: 42,
        state: Status::Ready,
        children: [],
        labels: [:],
        payload: (),
    )
```

The newline form requires named fields. Constructors with no supplied fields use
`MyType()` or `MyType ()`, with the opening parenthesis on the same line as the
type name. The type registry distinguishes `MyType ()` from a function call:
declared type names take precedence; otherwise `foo ()` passes `None` to `foo`.
A following ordinary grouped expression, including `()`, remains a separate
statement across a newline. Semicolons always end the statement. Shell-function
calls retain space-separated arguments, including `foo (expression)` on one line.

| Declaration | Meaning |
| --- | --- |
| `field` or `field: any` | Required field accepting any value, including `()` |
| `field: optional` or `field: optional any` | Any value; defaults to `()` when omitted |
| `field: Number` | Integer or float; strings and booleans do not qualify |
| `field: MyEnum` or `field: MyStruct` | A value of that specific type declaration |
| `field: [T]` | Array whose elements satisfy `T` |
| `field: [K: V]` | Map whose keys satisfy `K` and values satisfy `V` |
| `field: optional T` | Either `T` or `()`; defaults to `()` when omitted |

The existing builtin names also work, including `Integer`, `Float`, `String`,
`Boolean`, `None`, `Range`, `Array`, `HashMap`, and `ExecResult`. Annotations check
values without converting them. `Array` and `HashMap` accept those containers
without constraining their contents; `any` accepts every value.

Container annotations compose: `[String: [Item]]`, `[MyEnum: optional Item]`,
`[optional Number]`, and `optional [String: any]`. An array of optional elements
still requires an array; an optional array may itself be `()`. Empty arrays and
maps satisfy their respective element constraints. Map-key constraints follow
key equality: `1` and `1.0` denote the same key. This applies recursively to
collection keys; struct keys retain a typed snapshot of their original fields.

Every nonoptional field must be supplied, including untyped fields. Optional
fields may be omitted or supplied explicitly. Unknown and duplicate fields are
errors. Supplied expressions evaluate once, left to right in source order; fields
are stored and displayed in declaration order. Empty structs are allowed:
`struct Empty {}` and `Empty()`.

Read and update members with `$item.field`, including mixed paths such as
`$item.groups["name"][0].field`. An assignment must start with a variable.
Fields cannot be added or removed, and unknown members are errors. Accessing a
field through `()` is an error; an optional field containing a struct permits
normal member access. User-defined methods use `fn TypeName::method(...)` declarations;
optional-chaining syntax is not implemented.

Member syntax preserves word interpolation: `$item.field` accesses a field,
`${name}.txt` concatenates text, and `(${item}).field` accesses a field using a
braced variable. String interpolation still accepts variable names rather than
member expressions. Literal paths such as `file.txt` retain their meaning.

Structs have value semantics and share reference-counted storage. Assignment,
function calls, and collection insertion preserve their type; writes copy shared
values along the modified path. A method's implicit `$self` instead aliases its
receiver binding, so its mutations update that binding. All nested writes must satisfy the containing
field's constraints. An invalid update leaves the target unchanged, although
side effects from evaluating its indexes and right-hand expression remain.

Structs and enums share the same lexical type namespace, forward-reference rules,
and REPL identity rules. A scope cannot declare an enum and a struct with the
same name. An inner declaration may shadow an outer type. Type names `any` and
`optional` are reserved. Redeclaration in a later REPL submission creates a new
identity; existing values, field annotations, and compiled functions retain the
old definitions.

Self and mutual references are supported through optional fields or containers.
Cycles consisting entirely of required struct fields are rejected because they
cannot form a finite, fully initialized value. For example, `next: optional Item`
and `children: [Item]` are valid; a required `next: Item` inside `Item` is not.

Equality compares declaration identity and all field values. Structs can be map
keys, using immutable snapshots and normal value equality. They always convert
to true, including empty structs, and expansion with `...` passes one value.
Arithmetic, execution, iteration, and bracket-indexing a struct are errors; use
member access for its fields. Text such as `Item(quantity: 2, ...)` is display
output, not a serialization format.

The AST checker rejects invalid declarations, constructor shapes, known value
mismatches, and known invalid member accesses before executing the submitted
source. Dynamic values are checked during construction and updates. A failed
check or compilation does not publish new types or functions; runtime failures
occur after declarations have been committed.

## Explicit type conversions

Use `Type(value)` to request conversion. All builtin types support this syntax;
annotations still check values without converting them. `Integer` and `Float`
are concrete numeric types; `Number` accepts either (`Integer | Float`).

| Target | Accepted input and behavior |
| --- | --- |
| `Integer` | Integers, integral floats, decimal integer text, booleans, numeric exit statuses |
| `Float` | Numbers, numeric text, booleans, and numeric exit statuses |
| `Number` | Preserves numbers; converts numeric text, booleans, and numeric exit statuses |
| `Boolean` | Uses the truth rules below |
| `String` | Display text for any value, with executable flags removed |
| `Array` | Arrays, argument expansions, and bounded ranges |
| `ArgumentExpansion` | Arrays, argument expansions, and bounded ranges |
| `HashMap` | Existing hash maps |
| `Range` | Existing ranges, including unbounded ranges |
| `ExecResult` | Existing statuses, integer-valued numbers, decimal integer text, or booleans |
| `None` | Evaluates its input, then produces `()` |
| `any` | Preserves the input value and its concrete type |

Numeric conversions map booleans to 0 or 1 and numeric command statuses to their
exit code. `ExecResult` requires a code in 0..=255; it maps `true` to status 0 and
`false` to status 1. `Number` parses text as an integer when it fits, otherwise as
a finite float; booleans and numeric command statuses become integers.

Numeric text may have surrounding whitespace. `Integer("2.0")` is rejected;
`Integer(Float("2.0"))` succeeds. Fractional float-to-integer conversions, overflow,
invalid text, and nonfinite numeric conversions are errors. Floating-point
conversion has normal `f64` precision limits. A status caused by a signal has no
numeric code, so it cannot convert to an integer or float.

Collection conversions preserve elements without converting them. Arrays and
argument expansions share their reference-counted storage; writes retain value
semantics. Bounded ranges materialize their integer elements. Other collection
conversions, including arrays of pairs to maps, are errors.

```text
let $count: Integer = Integer("42")
let $ratio: Float = Float("2.5")
let $items: Array = Array(1..4)
echo ArgumentExpansion($items)      # 1 2 3
let $status: ExecResult = ExecResult(false)
echo Boolean($status) Integer($status)  # false 1
```

Conversions require one expression. Spaces before `(`, newlines inside the
parentheses, and a trailing comma are allowed; a newline before `(` starts a
separate statement. Use `None(())`, for example, rather than an empty `None()`.
Type names take precedence over function names in this syntax. A struct or enum
declaration with a builtin name shadows that conversion. Resolved conversion
targets remain fixed in previously compiled functions. User-defined conversions
are not implemented yet.

## Boolean expressions

`==` and `!=` compare values and produce booleans. Numbers compare numerically,
including integer/float pairs; strings compare their text, ignoring executable
flags. Float source spelling does not affect equality. Arrays compare their
elements in order. Unrelated types are unequal: `"1" == 1` and `true == 1` are
false. `()` equals `()`. Command statuses compare as statuses, not as integers
or booleans.

`Boolean(value)` explicitly converts a value to a boolean. `!`, `&&`, and `||`
use the same conversion rules:

| Value | Boolean conversion |
| --- | --- |
| `()` | False |
| Boolean | Its existing value |
| Integer or float | False for zero, true otherwise |
| String | False for empty text, exact `"false"`, or text parsing as numeric zero; true otherwise |
| Array, hash map, or argument expansion | False when empty, true otherwise |
| Range | False for an empty bounded range; true otherwise |
| Enum or struct | Always true, including empty structs |
| External command result | True for exit status 0; false for nonzero status or termination by signal |

```text
let $result = /bin/true
let $succeeded: Boolean = Boolean($result)  # true
echo Boolean(0) Boolean("false") Boolean([1])  # false false true
```

Boolean annotations check values without converting them: `let $flag: Boolean = 1`
is an error. `!!value` remains a shorthand for boolean conversion.
`Boolean(value)` requires one expression; use `Boolean(())` to convert `None`.

Logical operators always return a boolean. `&&` skips its right operand when
the left is false; `||` skips it when the left is true. Skipped operands have no
side effects and cannot cause runtime errors, but must still be valid syntax.

Newlines, blank lines, and comments may appear before or after `&&`, `||`, `==`,
and `!=`. A newline before an operator continues the expression; otherwise it
still ends the statement. Precedence and short-circuit behavior are unchanged:

```text
if    ($expected != "")
   && ($actual != $expected)
{
    echo "Output differs"
}
```

Precedence, highest first: parentheses and indexing; unary `!` and unary minus;
`* / %`; `+ -`; `.. ..=`; `== !=`; `&&`; `||`. Range operators cannot be chained;
other binary operators at the same precedence associate left to right. Boolean
operators do not require surrounding spaces, so `$x!=0` and `!$x` work. Quote
operator text when passing it literally.

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
are string operands; `foo == foo` compares text. Function parameters preserve
their argument types: passing `3` gives an integer, while passing `"3"` gives
a string. Equality does not coerce one into the other.

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

Reading variables and interpolating strings shortens paths under the current
`$HOME` to `~` or `~/...`, including `$pwd` and strings stored in arrays and map
values. Map keys retain their original values.
Only complete home-directory prefixes match; similarly named sibling directories
stay unchanged. Stored values are not rewritten by reading them.

Shelly expands leading `~` or `~/` at filesystem boundaries: `cd`, executable
lookup, glob variable prefixes, path settings, and external-command arguments.
This also applies to quoted or variable-derived arguments. Thus `cd $p` and
`cat "$p/file"` work with shortened paths, and `echo $pwd` prints an absolute
path. Embedded text such as `echo "cwd: ${pwd}"` retains the shortened path,
as does a custom prompt using `${pwd}` after its label or color codes. `~someone`
is not expanded. This external-command expansion does not apply at a shell
function call boundary; reading the function parameters follows the same path
shortening rules as other variable reads.

Unquoted paths can begin with a variable. Its value and the suffix remain one
argument, including spaces in the value:

```text
let $root = '/tmp'
echo $root/project/file.txt
let $name = 'report'
echo ${name}suffix           # reportsuffix
echo ${name}.txt             # report.txt
```

Braces mark the end of a variable name within a word: `${name}suffix` is one
argument, even when the variable's value contains spaces. `${name} suffix`
remains two arguments. `${items}[0]` and `${items}...` retain their indexing and
expansion meanings.

Variable-prefixed executable paths such as `$tools/echo` also work. Write
`$a / $b` for division; `$a/file` is a path. Variable-prefixed glob patterns
such as `$root/*.txt` interpolate the prefix before expanding the pattern.
Characters from the variable's value remain literal, including `[` or `*` in a
directory name.

Unquoted `*`, `?`, and bracket patterns in paths such as `./[ab]` expand matching paths in
sorted order; `**` supports recursive matching. Hidden entries require an explicit
leading dot, `.` and `..` are excluded, and no matches is an error. Quotes preserve
a glob as text.

```text
let $sources = src/language/*.rs
echo $sources...
```

## Functions, calls, and return values

Functions have named parameters and local variable scopes. Call them like
commands. Arguments are evaluated left to right and **retain their types** when
passed to Shelly functions, including enums, arrays, maps, and executable markers.
External commands and builtins receive text. Alias defaults and command-line
`$args` remain strings. Arrays passed to a function remain one array argument
unless expanded with `...`.

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

Parameters and return values can also be annotated:

```text
struct Item { value: Number }

fn make_item($value: Number): Item
{
    Item(value: $value)
}

let $item: Item = make_item 42
echo $item.value
```

Parameter types are checked before the function body runs and remain constraints
on assignments to those parameters. The return annotation applies to both explicit
and implicit returns. Bare `return` and fallthrough returning `()` require a type
that accepts `None`, such as `optional Number` or `any`. Unannotated parameters and
returns remain unrestricted. Integer arguments widen to Float where required;
other implicit conversions are rejected.

Trailing parameters annotated `optional T` may be omitted from right to left:

```text
fn describe($value: Number, $label: optional String, $limit: optional Number)
{
    echo $value $label $limit
}

describe 7                       # 7 () ()
describe 7 "items"               # 7 items ()
describe 7 "items" 10            # 7 items 10
```

Required parameters cannot follow optional ones. Use `optional any` for an
omittable parameter accepting any value. Explicit `()` occupies its argument
position, so `describe 7 () 10` skips the label while supplying the limit.
The compiler emits parameter-binding instructions containing `()` defaults;
these apply equally to direct calls, aliases, executable variables, and calls
whose arguments are expanded with `...`.

The final parameter may collect extra arguments into an array. `$rest...` accepts
elements of any type; `$rest: Number...` binds a `[Number]`:

```text
fn collect($rest...): Array
{
    return $rest
}

fn sum($values: Number...): Number
{
    let $total: Number
    for $value in $values { $total = $total + $value }
    return $total
}

echo (collect "hello" 3 true)...  # hello 3 true
echo (sum 1 2 3)                 # 6
echo (sum)                       # 0
```

A variadic parameter receives `[]` when there are no remaining arguments. It may
follow required and optional parameters; fixed parameters consume their positions
first, and all remaining arguments go into the array. Use `()` explicitly to skip
an optional position before supplying variadic arguments. The suffix follows the
element type: `$rows: [Number]...` receives `[[Number]]`, and
`$values: optional Number...` receives `[optional Number]`.

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
and functions ending in a declaration, assignment, alias, nested function
definition, or completed loop return `()`. Duplicate parameter names are rejected.

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

A backtick prefix creates a string marked executable without calling it. There is
no closing backtick. For a known Shelly function, the reference retains that
function's version, including its parameter and return constraints. Redefining
the name does not change a previously stored reference. Builtins, external commands,
and names without a known Shelly function remain name-based references.

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
forward calls work. Once a function is known, compiled calls and references retain
that version, including across later redefinitions in the same submission.
Forward references with no known version resolve through that submission's
completed function namespace. Later submissions cannot change that namespace.
Redefinitions do not retarget existing direct, recursive, or sibling calls;
newly compiled code sees the new definitions. Stored backtick
references also retain their original versions when copied, passed to functions,
returned, or stored in array elements, map values, and struct fields. A bound
function reference is not redirected by an alias added later.

Functions can contain helper functions:

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

Returns and runtime errors unwind any active block scopes. `return` inside a
block exits the enclosing function; it remains an error at the top level. A block at the end
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

## Match expressions

`match` evaluates a subject once and tries arm expressions from top to bottom.
The first matching arm runs; its block supplies the result:

```text
let $description = match $value
    {
        0            => { "zero" }
        1..10        => { "one through nine" }
        $expected    => { "the expected value" }
        $valid_range => { "inside the configured range" }
        $a..$b       => { "inside the other configured range" }
        _            => { "something else" }
    }
```

Non-range arm values use the same equality as `==`, including literal expressions,
arrays, maps, structs, and enum values. Variables supply their current values;
they do not introduce bindings. Arm expressions are evaluated only when reached,
and later expressions and bodies are skipped after a match. Function calls and
arithmetic can supply the subject, an arm value, or range bounds.

A range-valued arm tests **integer membership**. `a..b` excludes `b`, `a..=b`
includes it, and omitted bounds are unbounded. Reversed or empty ranges match
nothing. Non-integer subjects do not match range arms; other arms can handle them.
A variable holding a range has exactly the same behavior as a written range.

A bare `_` is an optional fallback and must be last; `"_"` is an ordinary string
pattern. Every arm requires a block. Commas after arm blocks are optional. An empty
arm block returns `()`. An empty arm list is a syntax error. If no arm matches and
there is no fallback, execution raises `Match error: No arm matched the value.`

Arm blocks have the same variable scopes, function hoisting, and alias rules as
other blocks. `return` exits the enclosing function; `break` and `continue` target
the enclosing loop. All arms are parsed and type-checked, even when not selected.
Match expressions also work in assignments, returns, collections, and grouped
command arguments.

## Loops

All loops require a block and are statements. Body results are discarded; a
function or conditional branch ending in a completed loop returns `()`.
Each iteration creates a fresh scope for bindings and body-local variables.
These can shadow outer variables; assignment still updates the nearest visible
binding. Body-local bindings do not escape. Loops may nest in any combination.

`return` exits the enclosing function, and runtime errors stop execution. Both
clean up active loop scopes and iterators. Function definitions and aliases inside
loops follow the hoisting and global-alias rules for blocks.

### For

Use one binding for array elements or range integers, and two bindings for map
keys and values. The expression after `in` can be a literal, a variable, an
indexed value, a conditional, or a function call:

```text
for $index in 1..4
{
    echo $index              # 1, then 2, then 3
}

let $items = ["red", "green", "blue"]
for $value in $items { echo $value }
for $value in [10, 20] { echo $value }

let $settings = ["width": 80, "height": 24]
for $key, $value in $settings { echo $key $value }
for $key, $value in ["answer": 42] { echo $key $value }
```

A parenthesized binding list destructures each array element. This works with
the arrays returned by `.zip` and with any array of arrays:

```text
for ($test_file, $output_file) in $tests.zip $outputs
{
    echo $test_file $output_file
}

for ($x, $y, $z) in [[1, 2, 3], [4, 5, 6]] { echo $x $y $z }
```

Each element must be an array with exactly as many elements as the pattern has
bindings. The check happens before binding any values or entering the loop body;
a mismatch reports the loop's source location. Patterns are flat lists of one
or more distinct variables; trailing commas, newlines, and comments are allowed.
`for ($value,) in [[1], [2]]` unwraps each one-element array. An empty iterable
skips the body without attempting to destructure an element.

The iterable is evaluated once, before any loop bindings are created. Array
elements retain their types and are visited in array order. Maps visit each
key/value pair once in unspecified order; keys use the map's canonical value
representation. Iteration uses a snapshot: assigning to the source collection or
mutating it during the loop does not change the remaining iterations. Changing
a collection held by a loop binding also leaves the original element unchanged.

Range iteration is lazy and requires both integer bounds. `..` excludes the end,
`..=` includes it, and reversed ranges are empty. Empty arrays, maps, and ranges
skip the body. Other iterable types, unparenthesized pairs of bindings for
arrays/ranges, one binding for maps, and duplicate binding names produce errors.
Map key/value iteration continues to use the unparenthesized `$key, $value` form.

### Unbounded loop

`loop { ... }` repeats its required block without a condition or iterable:

```text
let $count = 0
loop
{
    $count = $count + 1
    if $count == 2 { continue }
    echo $count
    if $count == 3 { break }
}
# Prints 1, then 3
```

An empty `loop {}` runs indefinitely.

### While and until

`while condition { ... }` repeats while the condition converts to true.
`until condition { ... }` repeats while it converts to false. Both check the
condition before the first iteration and before every subsequent iteration:

```text
let $n = 0
while $n != 3
{
    echo $n                  # 0, 1, 2
    $n = $n + 1
}
until $n == 0
{
    echo $n                  # 3, 2, 1
    $n = $n - 1
}
```

Conditions accept the same expressions and command calls as `if`, with the same
boolean conversion and short-circuit rules. A command's zero exit status is true;
nonzero status is false. `while false { ... }` and `until true { ... }` skip their
bodies, although those bodies must still contain valid syntax. A block is always
required, and newlines/comments may separate the condition from its opening brace.

The condition runs in the surrounding scope, before the body scope is created.
`continue` rechecks it; `break` exits without evaluating it again.

### Break and continue

`break` leaves the innermost active loop; `continue` skips the remainder of its
current iteration. In `for`, it advances to the next element; in `loop`, it
restarts the body; in `while` and `until`, it rechecks the condition. Neither
accepts a value or a loop label:

```text
for $i in 0..5
{
    if $i == 1 { continue }
    if $i == 3 { break }
    echo $i                  # 0, then 2
}
```

Both statements unwind the current iteration's scopes, including nested blocks,
and discard abandoned expression temporaries. Executing either without an active
loop in the current function or top-level execution produces an error. A called
function cannot control its caller's loop. Skipped branches still require valid
syntax, but do not execute their loop-control statements.

## Processes, aliases, and environment

`cd PATH` changes directory and returns `ExecResult(0)` on success or
`ExecResult(1)` on failure, so it can be used directly as a condition.
`exit` stops execution with status zero;
`exit 7` stops it with status 7. An explicit exit status must be an integer from
0 to 255. `echo` and the other Unix commands in these examples are external
programs found through `PATH`.

External commands return an `ExecResult` status. Their stdout is inherited by
Shelly unless redirected; assigning a command result does **not** capture its printed output.

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
`$shelly` (an executable reference to this binary), `$version`, `$os` (also `$OS`),
`$build_date`, `$build_time`, `$interactive`, `$login`, and `$rc_path`.
`$os` identifies the host operating system, for example `"macos"` or `"linux"`,
and can be used in functions to select platform-specific commands.
`$rc_path` is the configured init path, `<not found>` when missing, or
`<unloaded>` when init loading is disabled.

### File and variable redirection

`->` redirects stdout, `~->` redirects stderr, and `~+->` sends both streams to
the same destination. Data flows from left to right. These operators can be
combined on one command:

```text
let $output: String
let $errors: String
input.txt -> $output
sh -c 'echo output; echo error >&2' -> $output ~-> $errors
let $status = sh -c 'echo failed >&2; exit 1' ~+-> $output
echo $status                         # ExecResult(1)
```

A bare variable on the right of an output operator receives text; declare it
first, with a type that accepts `String`. Captures preserve all UTF-8 text,
including trailing newlines. Invalid UTF-8 produces an error. File-to-file and
process-to-file transfers preserve arbitrary bytes. Capturing output does not
change the command's return value or its normal failure handling.

Other destinations are file paths, created or truncated when opened. Quote a
variable to use its value as a filename instead of capturing into the variable:

```text
echo hello -> output.txt
let $log = 'command.log'
sh -c 'echo output; echo error >&2' ~+-> "$log"
```

A file can also supply text directly to a variable. A variable used as this
source holds the filename, matching `test.shy`:

```text
let $contents: String
input.txt -> $contents
let $path = 'input.txt'
$path -> $contents
```

A bare source word that resolves to a command runs that command; otherwise it
names a file. Quote the source path to force file access when its name matches a
command. Redirections also apply to commands called inside Shelly functions and
to builtin diagnostics. Streams are restored on completion, errors, and returns.
Each stream may be redirected once per expression. Left-facing redirection
operators are not supported. Input from files or variables into a process will
use `|` pipelines, which are not implemented yet. Append redirection is also
not implemented.

## Supervised processes and terminals

`run_process` accepts an argument array and an options map. It returns a result
map rather than raising a language error for a nonzero exit, signal, timeout, or
launch failure. Inspect the result explicitly:

```text
let $result = run_process ["/usr/bin/cat"] [
    "stdin_file": "input.bin", "stdout_file": "output.bin",
    "stderr_file": "errors.txt", "timeout_ms": 3000,
]
echo $result["exit_code"] $result["signal"] $result["timed_out"] $result["error"]
```

Options are `cwd`, `env`, `stdin_file`, `stdout_file`, `stderr_file`, and
`timeout_ms`. Arguments, paths, and environment names/values must be strings.
An omitted `env` inherits Shelly's exported variables; an explicit map replaces
the environment, including `[:]` to clear it. File paths are relative to the
calling shell's directory, independently of the child's `cwd`. Output files are
truncated; use distinct files for separate streams. Omitted streams inherit the
current streams, including Shelly's `->`, `~->`, and `~+->` redirections. Explicit
file options override that inheritance. File I/O preserves arbitrary bytes.

Results always contain `exit_code` (integer or `()`), `signal` (integer or `()`),
`timed_out` (boolean), and `error` (launch/I/O error text or `()`). Invalid API
arguments are language errors. Deadlines are nonnegative integer milliseconds;
omitting `timeout_ms` waits indefinitely. On timeout Shelly sends SIGTERM to the
child's process group, waits 100 ms, then sends SIGKILL and reaps the child. It
also stops remaining group members after the direct child exits normally.
Children that deliberately create a different process group escape this group
cleanup. This API currently requires Unix.

The native terminal API creates a child with a controlling PTY:

```text
let $terminal: Terminal = open_terminal ["/usr/bin/cat"] ["rows": 40, "columns": 140]
terminal_write $terminal "hello\n"
let $reply = terminal_read $terminal 1000
echo $reply["text"] $reply["eof"] $reply["timed_out"]
let $status = terminal_close $terminal
```

`open_terminal` accepts `cwd`, `env`, `rows`, and `columns`. Dimensions must be
integers from 1 to 65535; defaults are 40 by 140. `Terminal` is an opaque shared
handle with identity equality; assigning it shares the same terminal. Reads
return UTF-8 text, EOF/timeout flags, and the process-result fields above. A read
timeout does not kill the process or imply EOF. Reads may return partial output;
UTF-8 sequences split between reads are retained, while invalid or truncated
UTF-8 raises an error. Writes accept strings and have a five-second deadline if
the PTY stops accepting input. Closing releases the PTY, stops the process group,
and reaps the child; repeated closes return the same status. Dropping the last
handle also cleans up. Writes and reads after explicit close are errors.

Strings provide `$text.chars`, `$text.contains $part`, `$text.starts_with $prefix`,
`$text.ends_with $suffix`, `$text.replace $old $new`, `$text.split $separator`,
`$text.trim`, `$text.trim_start`, and `$text.trim_end`. They return new values and
never mutate the receiver. `chars` returns Unicode scalar strings; `split`
retains empty fields, including leading/trailing ones. Trimming uses Unicode
whitespace and is always explicit: captures still preserve trailing newlines.

## Current limitations and known issues

- Arithmetic converts operands to integers rather than preserving floating-point values.
- Variables use dynamic caller scope; function definitions are hoisted within each input.
- Command results are statuses, not captured stdout. Shell errors in noninteractive
  execution return a general failure status; only explicit `exit N` selects a specific status.
- Pipelines, append redirection, ordering comparisons (`<`, `>`, `<=`, `>=`), array slicing,
  and array append syntax are not implemented.
- Iterating or expanding a range requires both bounds. `break` cannot carry a value
  or target a named loop. Standalone blocks are statements, not general expressions.
- Some parser diagnostics contain verbose lists of attempted alternatives.

## Implementation and development

`src/main.rs` selects the execution mode. `src/runtime/repl.rs` implements the
Reedline editor, completion, and prompt. The active tokenizer, parser, AST,
compiler, values, and interpreter live under `src/language/`. Source is tokenized
and parsed into an AST, checked against a staged type registry, compiled to
bytecode, optimized, linked, then executed. The initial checking pass registers
lexically scoped enum and struct identities before resolving field annotations.
It checks constructors, required-field cycles, annotations, known incompatible
initializers, assignments and returns, and known member accesses. Runtime checks
cover dynamic values, function arguments, return paths, and nested writes; general
type inference remains future work. Builtin types, container
constraints, enums, and structs share stable `TypeId`s independent of name visibility.

Two optimization passes run before linking: adjacent `PopResult`/`PushResult`
pairs are removed, and redundant `CheckResult` instructions are dropped only
when the compiler can prove the result is already empty. The proof is conservative
across calls and control-flow boundaries.

Jumps and `EnterLoop` initially refer to labels. Linking resolves them to numeric
instruction indexes independently for each function and the top-level code,
rejecting missing or duplicate labels. `JumpTarget` instructions remain as landing
points, without their labels; the interpreter sees only numeric destinations.

Blocks use `EnterScope` and `ExitScope`. `EnterLoop` pushes the continue/break
addresses and saved execution depths; `ExitLoop` pops that frame. `Break` and
`Continue` unwind scopes and temporary values before jumping. For loops also use
iterator instructions; while and until use `ToBoolean` and conditional jumps.
Loop and iterator stacks belong to the current execution frame and are cleaned
up on returns and errors.

Build with `cargo build --locked` and check behavior through command-line source,
scripts, or the REPL. `cargo clippy --locked --all-targets` runs the Rust lints.

[test.shy](test.shy) runs the Shelly test suite from the repository root:

```sh
./target/debug/shelly -m test.shy
./target/debug/shelly -m test.shy native
./target/debug/shelly -m test.shy C001
```

The suite includes 4,105 process cases, 89 stateful REPL scenarios, prompt/path
checks, watchdog probes, native API tests, and harness failure controls. All
orchestration and assertions run in Shelly; no Python, pexpect, or other shell is
needed. Standard Unix utilities still provide file operations and byte/regex
comparisons. See [tests/README.md](tests/README.md) for the case format and
[tests/MIGRATION.md](tests/MIGRATION.md) for coverage mapping.

Each case gets a private temporary fixture, isolated HOME/TMPDIR, an explicit
child environment, and native process deadlines. The runner continues after
failures and exits 1 if any test fails or the selection is empty. Normal
completion removes temporary fixtures; interrupted runs can leave directories
under `/tmp/shelly-suite.*`.

**Keep validation releases separate from an installed/default-shell binary:**

```sh
cargo build --locked --release --target-dir target/test-validation
./target/test-validation/release/shelly -m test.shy
```

This does not replace `target/release/shelly`.

## Direction

The aim is to keep the immediacy of a shell while giving larger scripts a clear
path to structure. Future work includes broader type inference and contracts,
network and JSON support, and pipelines for text and structured data.
