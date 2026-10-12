# Shelly for VS Code

The extension's internal name is `shelly-shell`, matching the Rust package.
It recognizes `.shy` files and Shelly shebangs, starts `shelly --language-server`,
and displays semantic highlighting and syntax diagnostics. The server uses the
same tokenizer, parser, and semantic classifications as the REPL. There is no
separate TextMate grammar. Your VS Code theme chooses the colors.

Requires VS Code 1.140 or later and a Shelly build supporting `--language-server`
(introduced in the shell's 0.5.2 source). The extension does not bundle the shell.
On Windows, run it in a WSL workspace with Shelly installed inside WSL. Linux
and macOS use their native Shelly executable.

## Build and install locally

Build Shelly from the repository root with `cargo build --locked`. Then:

```sh
cd extensions/vscode
npm ci
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

For development, open `shelly-shell.code-workspace`, build Shelly, and press F5.
The workspace launches an Extension Development Host using the debug binary.
The **Shelly** output channel reports startup failures. Set
`shelly-shell.trace.server` to `messages` or `verbose` for protocol tracing.

## First-version boundaries

Highlighting includes lexical tokens, parser context, declarations, native types
and methods, and understood references within the current document. For example,
`let` is an argument in `echo let`, and references inside interpolated strings
are classified without evaluating them. Invalid syntax gets diagnostics;
unfinished input gets hints. Clearing or closing a document clears its diagnostics.

The server never executes profiles, RC files, the prelude, imports, or document
code. Standard-library and workspace import indexing, completion, hover,
navigation, formatting, and type checking are follow-up work. Unknown names do
not produce speculative type errors. Source analysis covers up to 1 MiB and
640,000 parser reads per document, with a diagnostic when a limit is reached.
Analysis results are cached by document version; stale updates are ignored.
