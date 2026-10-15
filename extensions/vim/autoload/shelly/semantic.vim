vim9script

import autoload 'lsp/buffer.vim' as buffers

# The server's token numbers and UTF-16 spans are unchanged. Only this client's
# legend uses Vim's supported names, before its delayed first highlight request.
const names = {
    shellyBuiltin: 'function',
    shellyAlias: 'macro',
    shellyCommand: 'function',
    shellyArgument: 'string',
    shellyLiteral: 'enumMember',
    shellyDelimiter: 'operator',
    shellyFormat: 'namespace',
    shellyEscape: 'regexp',
    shellyError: 'operator',
}

augroup ShellySemantic
augroup END

export def Cancel(buffer: number)
    var timer = getbufvar(buffer, 'shelly_semantic_timer', 0)
    if timer > 0
        timer_stop(timer)
        setbufvar(buffer, 'shelly_semantic_timer', 0)
    endif
enddef

def Enabled(buffer: number): bool
    return bufloaded(buffer) && getbufvar(buffer, '&filetype') == 'shelly'
        && g:LspOptionsGet().semanticHighlight
enddef

def Refresh(buffer: number)
    if !bufexists(buffer)
        return
    endif
    setbufvar(buffer, 'shelly_semantic_timer', 0)
    if !Enabled(buffer)
        return
    endif
    var server = buffers.BufLspServerGet(buffer, 'semanticTokens')
    if get(server, 'name', '') == 'shelly-shell'
            && get(server, 'running', false) && get(server, 'ready', false)
        # The client flushes pending edits and rejects replies for older changedticks.
        server.semanticHighlightUpdate(buffer)
    endif
enddef

export def Schedule(buffer: number)
    Cancel(buffer)
    if !Enabled(buffer)
        return
    endif
    var delay = g:LspOptionsGet().semanticHighlightDelay
    var timer = timer_start(delay, (_) => Refresh(buffer))
    setbufvar(buffer, 'shelly_semantic_timer', timer)
enddef

export def Translate()
    var context = get(g:, 'LspAttachedContext', {})
    var buffer = get(context, 'bufnr', bufnr())
    if getbufvar(buffer, '&filetype') != 'shelly'
        return
    endif
    for server in buffers.BufLspServersGet(buffer)
        if get(server, 'name', '') != 'shelly-shell'
            continue
        endif
        var legend = get(server, 'semanticTokensLegend', {})
        if has_key(legend, 'tokenTypes')
            # Preserve token index order when replacing the client's token names.
            legend.tokenTypes = map(copy(legend.tokenTypes), (_, name) => get(names, name, name))
        endif
    endfor

    # The client's TextChanged hook covers Normal mode. Add the two Insert-mode
    # events, including edits with a completion popup, without replacing its hooks.
    Cancel(buffer)
    autocmd_delete([{group: 'ShellySemantic', bufnr: buffer}])
    autocmd_add([
        {group: 'ShellySemantic', bufnr: buffer, event: ['TextChangedI', 'TextChangedP'],
            cmd: $'call shelly#semantic#Schedule({buffer})'},
        {group: 'ShellySemantic', bufnr: buffer,
            event: ['TextChanged', 'InsertLeave', 'BufLeave', 'BufUnload'],
            cmd: $'call shelly#semantic#Cancel({buffer})'},
    ])
enddef
