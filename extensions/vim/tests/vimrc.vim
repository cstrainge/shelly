set nocompatible
set hidden nomore noswapfile
if $SHELLY_TEST_SCENARIO ==# 'syntax-first'
    syntax on
elseif $SHELLY_TEST_SCENARIO ==# 'filetype-first'
    filetype plugin on
endif
let g:shelly_server_path = $SHELLY_TEST_SERVER
if $SHELLY_TEST_SCENARIO ==# 'tilde'
    " Exercise Vim's expansion, after the harness's environment path conversion.
    let g:shelly_server_path = '~/shelly server'
elseif $SHELLY_TEST_SCENARIO ==# 'path'
    unlet g:shelly_server_path
endif
let g:shelly_highlight_delay = 10
if $SHELLY_TEST_SCENARIO ==# 'zero-delay'
    let g:shelly_highlight_delay = 0
elseif $SHELLY_TEST_SCENARIO ==# 'insert'
    let g:shelly_highlight_delay = 150
endif
if $SHELLY_TEST_SCENARIO ==# 'manual'
    let g:shelly_auto_start = 0
endif
execute 'set runtimepath^=' . fnameescape($SHELLY_TEST_CLIENT)
execute 'set runtimepath^=' . fnameescape($SHELLY_TEST_PLUGIN)
if $SHELLY_TEST_SCENARIO ==# 'plugin-first'
    runtime plugin/shelly.vim
    runtime plugin/lsp.vim
else
    runtime plugin/lsp.vim
    runtime plugin/shelly.vim
endif
filetype plugin on
syntax enable
