# Changelog

## Unreleased

- Add module-aware type, function, field, and method hovers with compact Markdown.
- Complete scoped names, visible members, imported namespaces, and named factory arguments.
- Index unsaved buffers, workspace imports, and the standard prelude without executing source.
- Describe external commands with variadic `any` arguments.
- Require Shelly 0.5.3 or later.

- Recognize named struct factories, enum defaults, private storage, and public fields through the updated Shelly server.
- Highlight computed properties using accessor types and raw access using storage types.
- Cover raw interpolation and dynamic access in the bracket-coloring fixtures.
- Require the updated Shelly executable for the new language syntax; restart the language server after rebuilding.

## 0.1.1

- Color bracket nesting from server-classified delimiters using the theme's bracket palette.
- Exclude string and comment text; clear stale bracket colors on edits and server restarts.
- Configure bracket coloring with `shelly-shell.bracketColorization`.
- Use the Marketplace display name Shelly Shell and declare Node.js 22 for packaging.

## 0.1.0

- Recognize Shelly `.shy` files and shebangs.
- Highlight source using Shelly's tokenizer, parser context, and semantic analysis.
- Show syntax errors and incomplete-input hints without executing source or startup files.
- Configure the server executable and restart it from the command palette.
- Detect unsupported executables before connecting and report the selected path in output logs.
- Support local and remote extension hosts, Unicode positions, and document version updates.
