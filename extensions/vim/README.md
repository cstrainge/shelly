# Shelly for Vim

Vim integration for Shelly's existing language server. It supplies semantic
highlighting, diagnostics, hover, and completion through
[yegappan/lsp](https://github.com/yegappan/lsp). Use Vim 9 with jobs, channels,
text properties, and popup windows; a recent Vim 9.1 is recommended.
This adapter targets classic Vim, rather than Neovim's separate LSP API.

## Use the repository plugin and release binary

Install the Vim LSP client once:

```sh
git clone https://github.com/yegappan/lsp ~/.vim/pack/downloads/opt/lsp
```

Put this in `~/.vimrc`, adjusting the repository path if necessary:

```vim
set nocompatible
packadd lsp

let g:shelly_server_path = expand('~/workdir/shelly/target/release/shelly')
execute 'set runtimepath^=' . fnameescape(expand('~/workdir/shelly/extensions/vim'))
runtime plugin/shelly.vim

filetype plugin on
syntax enable
```

Open a `.shy` file. The plugin starts the selected binary with
`--language-server`; it does not run your shell startup files or execute the
document. This setup reads the plugin directly from your checkout. Restart Vim
after changing plugin code or the configured executable path. After updating
the binary at the same path, use `:ShellyLspRestart` to restart its server.

The example selects your existing release executable. The plugin never builds,
installs, or overwrites that binary. For development validation you can instead
point at `target/debug/shelly`, built with `cargo build --locked`.

## Package installation

Alternatively, link the adapter into Vim's native package directory:

```sh
mkdir -p ~/.vim/pack/shelly/start
ln -s ~/workdir/shelly/extensions/vim ~/.vim/pack/shelly/start/shelly-shell
```

Keep `packadd lsp` and set `g:shelly_server_path` in your vimrc, but omit the
explicit `runtimepath` and `runtime plugin/shelly.vim` lines. A plugin manager
can also add `extensions/vim` to the runtime path.

## Commands and settings

| Setting | Default | Meaning |
| --- | --- | --- |
| `g:shelly_server_path` | `'shelly'` | Executable path or command found through PATH. |
| `g:shelly_auto_start` | `1` | Start automatically for Shelly buffers. Set to `0` for manual startup. |
| `g:shelly_highlight_delay` | `150` | Semantic highlight debounce in milliseconds; `0` is allowed. |

Set these before loading the plugin. Registration enables the client's global
`semanticHighlight` option and sets its global `semanticHighlightDelay`; those
two client options also apply to other servers managed by yegappan/lsp.

- `:ShellyLspStart`: start or attach manually; retry after fixing setup errors.
- `:ShellyLspRestart`: restart Shelly for all its attached buffers.
- `:LspHover`: show information about the symbol under the cursor.
- `:LspDiag show`: show document diagnostics.
- Insert mode **Ctrl+X Ctrl+O**: complete through Vim's `omnifunc`.
- `:LspServer show status`: inspect server status.
- `:LspServer debug on`, then `:LspServer debug messages`: inspect protocol traffic.

No user key mappings are replaced. If desired, add your own buffer-local hover
mapping:

```vim
autocmd FileType shelly nnoremap <buffer> K <Cmd>LspHover<CR>
```

To install the help index, run `:helptags ~/workdir/shelly/extensions/vim/doc`,
then `:help shelly`.

If only strings and comments are colored, check `:set filetype?`: Shelly source
should report `filetype=shelly`. The plugin registers detection even when a
system vimrc enabled syntax or filetype detection earlier. For an already open
buffer with the wrong type, use `:set filetype=shelly`, then `:ShellyLspStart`.
Manual startup reports a warning if the current buffer has another filetype.

## Highlighting and pipeline alignment

The server supplies syntax and semantic roles, including ordinary command
arguments and references inside interpolated strings. The adapter translates
Shelly's custom token names to standard names supported by the Vim client,
without changing token indices, source positions, or the server. Builtins and
external commands use function highlighting, aliases use macro highlighting,
and ordinary arguments use string highlighting. Your Vim theme chooses colors
through the client's `LspSemantic*` highlight groups.

Semantic colors refresh while you type, including with the completion popup
open. Updates use the client's `semanticHighlightDelay` (150 ms by default),
so brief pauses refresh the latest text without issuing a request for every key.
Leaving Insert mode uses the client's ordinary Normal-mode refresh. Disabling
the client's `semanticHighlight` option also disables these Insert-mode requests.

A small syntax file supplies comment/string boundaries for Vim's bracket
matching, including unfinished quotes and both multiline string forms. It does
not guess keywords or replace server analysis with a second language grammar.

For visual pipeline splits, the file's header `#tab-size N` controls buffer tab
stops. Without a pragma, the plugin uses inherited `$SHELLY_TAB_SIZE`, falling
back to four for an unset or invalid value. It preserves `expandtab` and
`shiftwidth`, and clears `vartabstop` so variable stops cannot override the
chosen grid. The server inherits the same environment. Prefer a file pragma for
portable diagrams; startup scripts are not loaded by language-server mode.

Shelly currently supplies no rename, definition-navigation, or formatting
requests. An unsupported client command does not add those server features.

## Development checks

From the repository root, install an ignored local client checkout and run:

```sh
git clone https://github.com/yegappan/lsp extensions/vim/.deps/lsp
cargo build --locked
./target/debug/shelly -m extensions/vim/test.shy
```

An optional final argument selects another absolute client directory. These
checks run real headless Vim processes and the debug server. Shelly owns the
assertions and deadlines; Vim fixtures report editor observations as JSON.
Coverage includes actual text properties, UTF-16/Unicode positions, edits and
diagnostics, restart, bracket exclusions, tab pragmas, early system syntax/filetype
setup, load order, manual startup,
paths with spaces and tildes, and helpful configuration errors. A real terminal
check also verifies ordinary `VimEnter` startup and colored keyword output without
explicitly enabling the client in the fixture. Real typing checks cover updates
before leaving Insert mode, unfinished input, and edits with the completion popup
open. No Vim or LSP dependency is added to the ordinary language suite.
