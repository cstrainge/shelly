" Report configuration failures without requiring a running client or server.
let s:report = {}
try
    execute 'set runtimepath^=' . fnameescape($SHELLY_TEST_PLUGIN)
    let g:shelly_server_path = $SHELLY_TEST_SERVER
    if $SHELLY_TEST_SCENARIO !=# 'missing-client'
        execute 'set runtimepath^=' . fnameescape($SHELLY_TEST_CLIENT)
        runtime plugin/lsp.vim
    endif
    runtime plugin/shelly.vim
    filetype plugin on
    syntax enable
    execute 'edit ' . fnameescape($SHELLY_TEST_WORK . '/sample.shy')
    let s:report.setup = shelly#setup()
    let s:report.error = get(g:, 'shelly_lsp_error', '')
    let s:report.filetype = &filetype
    let s:report.tabstop = &tabstop
catch
    let s:report.exception = v:exception
    let s:report.throwpoint = v:throwpoint
endtry
call writefile([json_encode(s:report)], $SHELLY_TEST_WORK . '/report.json')
qa!
