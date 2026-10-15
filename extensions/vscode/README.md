# Shelly Shell for VS Code

The extension's internal name is `shelly-shell`, matching the Rust package.
It recognizes `.shy` files and Shelly shebangs, starts `shelly --language-server`,
and provides semantic highlighting, syntax diagnostics, hovers, and completions.
The server uses the same tokenizer, parser, and semantic classifications as the
REPL. Your VS Code theme chooses the colors.

Bracket matching, coloring, guides, and auto-closing run inside VS Code. A small
extension grammar identifies comments, quoted strings, and multiline strings so
brackets in that text are excluded. Interpolation braces stay inside their
string. This works while typing, before the server responds, and with semantic
highlighting disabled or the server unavailable.

Use VS Code's `editor.bracketPairColorization.enabled` and
`editor.guides.bracketPairs` settings to control bracket display. These replace
the extension's former `shelly-shell.bracketColorization` setting.

Requires **Shelly 0.5.4 or later** and VS Code 1.140 or later.
The extension does not bundle Shelly; an older server executable cannot provide
these language features.
On Windows, run it in a WSL workspace with Shelly installed inside WSL. Linux
and macOS use their native Shelly executable.

## Build and install locally

Build Shelly from the repository root with `cargo build --locked`. Then:

```sh
cd extensions/vscode
npm ci
npm test
npm run package
code --install-extension shelly-shell.vsix
```

Configure the executable in VS Code settings:

```json
{
    "shelly-shell.serverPath": "/absolute/path/to/shelly/target/debug/shelly"
}
```

The default is `shelly` on PATH. Absolute paths, `~/` paths, and paths relative
to the first workspace folder are supported. The executable must support
`--language-server`; an older installed shell will not. After changing the path,
run **Shelly: Restart Language Server**. The server is launched directly, without
another shell. In WSL, SSH, or a container, install the extension in that remote
environment and configure a path available there.

In a WSL window, use **Preferences: Open Remote Settings (JSON)** for the Linux
executable path. A newly installed extension may need **Developer: Reload Window**
to refresh its settings schema. **Shelly: Restart Language Server** starts a new
process after changing the path or rebuilding the binary. The extension checks
CLI help for `--language-server` before connecting, so older binaries produce a
specific error. The **Shelly** output channel records the selected executable and
startup failures. A release built before server support must be updated manually
or replaced in this setting with the current debug binary.

Packaging uses Node.js 22 or later. See
[publishing instructions](https://github.com/cstrainge/shelly/blob/main/extensions/vscode/PUBLISHING.md)
for preparing a Marketplace release.

`npm test` runs Shelly assertions against the real TextMate and Oniguruma token
engines used by VS Code. Coverage includes comment/string exclusion, escapes,
multiline strings, incomplete input, Unicode positions, and bracket configuration. Build the repository's debug Shelly binary first.

For development, open `shelly-shell.code-workspace`, build Shelly, and press F5.
The workspace launches an Extension Development Host using the debug binary.
The **Shelly** output channel reports startup failures. Set
`shelly-shell.trace.server` to `messages` or `verbose` for protocol tracing.

## Language features

Highlighting includes lexical tokens, parser context, declarations, native types
and methods, and understood references within the current document. For example,
`let` is an argument in `echo let`, and references inside interpolated strings
are classified without evaluating them. Member interpolation such as `${p.name}`
colors the root as a variable and the member as a property; nested getters retain
method highlighting. Dynamic accessors such as `$p.$name` and `${p.$name}` are
variable references, including assignment targets and unknown names. Interpolation
braces stay outside structural bracket coloring. These forms require a Shelly
binary built with member interpolation and dynamic member support; rebuild it and
run **Shelly: Restart Language Server** after updating the shell.

Private storage declarations, `pub` fields, named `Type::new` factories, and enum
default constructors use the shared parser. Getter/setter properties highlight as
properties using their advertised type, including computed properties and chained
methods such as `$account.balance.trim`. Raw access `$self.::balance` uses the
backing field's type; `$self.::$name` and `${self.::$name}` highlight the accessor
as a variable. Constructors have no implicit `$self`. A nullable factory result
is left uncertain during editing instead of assuming a valid struct. Constructor
visibility, property signatures, and return validity are enforced when Shelly
compiles or runs the program; editor diagnostics currently cover syntax only.

These changes require a Shelly executable built from the updated language sources.
Point `shelly-shell.serverPath` at that build and run **Shelly: Restart Language
Server**. Updating the extension alone does not update its server executable.

Invalid syntax gets diagnostics;
unfinished input gets hints. Clearing or closing a document clears its diagnostics.

Hover a type to see its public storage fields, accessible properties, and methods
visible from the current module. Function and method hovers show their declared
parameter and return types. Property hovers distinguish read-only, write-only,
and writable properties. External binaries found on the server's PATH use the
placeholder signature `fn command($args: any...): any`.

Completion supports local variables and parameters, types, functions, imported
namespaces, methods, properties, raw access, and named constructor arguments.
Private constructors remain visible to their defining module and its children;
private storage is offered only inside the owning type's methods. `Type::` offers
constructors, methods, and enum variants; `Type.` describes the type object.
Open, unsaved dependency buffers take precedence over disk contents. Workspace
`.shy` file changes invalidate the declaration cache.

Imports are indexed using local-first lookup, `module.shy` and `module/init.shy`,
`SHELLY_MODULE_PATH`, and the separate `SHELLY_STD_PATH` lookup for `std::` modules.
The standard prelude is indexed automatically. Conditions that require executing
code are left uncertain; their declarations are not advertised. Indexing is
bounded to 64 modules, 1 MiB per module, and 8 MiB of source in total.

The server reads declarations and executable metadata. It never executes profiles,
RC files, prelude code, imported module code, getters, or external binaries.
Navigation, formatting, and type checking remain follow-up work. Unknown names do
not produce speculative type errors. Source analysis covers up to 1 MiB and
640,000 parser reads per document, with a diagnostic when a limit is reached.
Analysis results are cached by document version; stale updates are ignored.
