# Changelog

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
