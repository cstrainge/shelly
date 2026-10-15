vim9script

import autoload 'lsp/lsp.vim' as lsp
import autoload 'lsp/diag.vim' as diagnostics

var report: dict<any> = {}

def WaitFor(Condition: func(): bool)
    var started = reltime()
    while !Condition() && reltimefloat(reltime(started)) < 8
        sleep 10m
    endwhile
    if !Condition()
        throw 'Timed out waiting for the Vim client'
    endif
enddef

def Properties(): list<any>
    return range(1, line('$'))->mapnew((_, row) => prop_list(row))
enddef

def Changed()
    listener_flush()
    doautocmd TextChanged
enddef

def LexicalBrackets(): list<any>
    syntax sync fromstart
    var brackets: list<any> = []
    for row in range(1, line('$'))
        var text = getline(row)
        for index in range(strchars(text))
            var char = strcharpart(text, index, 1)
            if stridx('()[]{}', char) < 0
                continue
            endif
            var column = byteidx(text, index) + 1
            var group = synIDattr(synID(row, column, 1), 'name')
            add(brackets, [row, column, char, group])
        endfor
    endfor
    return brackets
enddef

try
    execute 'edit ' .. fnameescape($SHELLY_TEST_WORK .. '/sample.shy')
    report.filetype = &filetype
    report.commentstring = &commentstring
    report.tabstop = &tabstop
    report.before = getline(1, '$')
    if $SHELLY_TEST_SCENARIO == 'manual'
        report.manual_before = g:LspServerRunning('shelly')
        ShellyLspStart
    endif
    # -S runs before VimEnter, so explicitly allow the client to initialize.
    g:LspEnable()
    WaitFor(() => g:LspServerReady())
    WaitFor(() => !empty(get(b:, 'LspSemanticTokensData', [])))
    report.properties = Properties()
    report.after = getline(1, '$')
    var server = lsp.Server()
    report.legend = server.semanticTokensLegend.tokenTypes
    var uri = 'file://' .. expand('%:p')
    report.hover = server.rpc('textDocument/hover', {
        textDocument: {uri: uri}, position: {line: 5, character: 1}})
    report.completion = server.rpc('textDocument/completion', {
        textDocument: {uri: uri}, position: {line: 5, character: 3}})
    report.diagnostics = diagnostics.GetDiagsForBuf()
    setline(7, 'let $n = "unfinished')
    Changed()
    WaitFor(() => !empty(diagnostics.GetDiagsForBuf()))
    report.invalid = diagnostics.GetDiagsForBuf()
    report.invalid_properties = Properties()
    setline(7, 'let $n = 9')
    Changed()
    WaitFor(() => empty(diagnostics.GetDiagsForBuf()))
    report.fixed = diagnostics.GetDiagsForBuf()
    var oldpid = job_info(server.job).process
    ShellyLspRestart
    WaitFor(() => g:LspServerReady())
    WaitFor(() => !empty(get(b:, 'LspSemanticTokensData', [])))
    report.restart_legend = lsp.Server().semanticTokensLegend.tokenTypes
    report.restart_properties = Properties()
    report.restarted = job_info(lsp.Server().job).process != oldpid
    report.error = get(g:, 'shelly_lsp_error', '')

    # Filetype detection and lexical boundaries also work without semantic data.
    execute 'edit ' .. fnameescape($SHELLY_TEST_WORK .. '/script')
    report.shebang = &filetype
    execute 'edit ' .. fnameescape($SHELLY_TEST_WORK .. '/plain.txt')
    report.other_filetype = &filetype
    report.wrong_filetype_start = shelly#start()
    report.wrong_filetype_error = get(g:, 'shelly_lsp_error', '')
    execute 'edit ' .. fnameescape($SHELLY_TEST_WORK .. '/lexical.shy')
    if $SHELLY_TEST_SCENARIO == 'manual'
        ShellyLspStart
    endif
    report.recovered_error = get(g:, 'shelly_lsp_error', '')
    report.brackets = LexicalBrackets()
    setline(1, '#tab-size 3')
    Changed()
    report.edited_tabstop = &tabstop
    setline(1, '#tab-size 99')
    Changed()
    report.invalid_tabstop = &tabstop
    setline(1, 'echo "first code"')
    setline(2, '#tab-size 12')
    Changed()
    report.late_tabstop = &tabstop
    report.expandtab = &expandtab
    report.vartabstop = &vartabstop
    runtime plugin/shelly.vim
    report.registered = shelly#setup()
catch
    report.exception = v:exception
    report.throwpoint = v:throwpoint
    report.error = get(g:, 'shelly_lsp_error', '')
endtry
writefile([json_encode(report)], $SHELLY_TEST_WORK .. '/report.json')
qa!
