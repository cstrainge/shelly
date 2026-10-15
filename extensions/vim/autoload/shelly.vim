let s:registered = 0

function! s:error(message) abort
    let g:shelly_lsp_error = a:message
    return 0
endfunction

function! s:warn(message) abort
    call s:error(a:message)
    echohl WarningMsg
    echomsg a:message
    echohl None
    return 0
endfunction

function! shelly#setup() abort
    if s:registered
        return 1
    endif
    if !has('vim9script') || !has('job') || !has('channel')
                \ || !has('textprop') || !has('popupwin')
        return s:error('Shelly needs Vim 9 with jobs, channels, text properties and popup windows.')
    endif
    if !exists('*LspAddServer') || !exists('*LspOptionsSet')
        return s:error('Install yegappan/lsp and load it with :packadd lsp.')
    endif
    let l:path = get(g:, 'shelly_server_path', 'shelly')
    if type(l:path) != v:t_string || empty(l:path)
        return s:error('g:shelly_server_path must name a Shelly executable.')
    endif
    if l:path =~# '^\~/'
        let l:path = expand('~') . strpart(l:path, 1)
    endif
    if !executable(l:path)
        return s:error('Shelly executable not found: ' . l:path)
    endif
    let l:delay = get(g:, 'shelly_highlight_delay', 150)
    if type(l:delay) != v:t_number || l:delay < 0
        return s:error('g:shelly_highlight_delay must be a nonnegative number.')
    endif

    " LspAddServer can trigger LspSetup during its own initialization.
    let s:registered = 1
    try
        call LspOptionsSet({'semanticHighlight': v:true, 'semanticHighlightDelay': l:delay})
        call LspAddServer([{'name': 'shelly-shell', 'filetype': ['shelly'],
                    \ 'path': l:path, 'args': ['--language-server'],
                    \ 'rootSearch': ['.git/', '.git'], 'omnicompl': v:true}])
    catch
        let s:registered = 0
        return s:error('Could not register Shelly: ' . v:exception)
    endtry
    let g:shelly_lsp_error = ''
    return 1
endfunction

function! shelly#start() abort
    if &l:filetype !=# 'shelly'
        return s:warn('Current filetype is ' . string(&l:filetype)
                    \ . '; use :set filetype=shelly for Shelly source.')
    endif
    if !exists('*LspAddServer')
        silent! packadd lsp
    endif
    if !shelly#setup()
        return s:warn(g:shelly_lsp_error)
    endif
    let g:shelly_lsp_error = ''
    LspServer start
    return 1
endfunction

function! shelly#restart() abort
    if &l:filetype !=# 'shelly'
        echoerr 'Open a Shelly buffer before restarting its server.'
        return
    endif
    if shelly#start()
        LspServer restart
    endif
endfunction

function! shelly#tabs() abort
    let l:default = empty($SHELLY_TAB_SIZE) ? '4' : $SHELLY_TAB_SIZE
    let l:width = l:default =~# '^[0-9]\+$' ? str2nr(l:default) : 4
    if l:width < 1 || l:width > 32
        let l:width = 4
    endif
    let l:seen = 0
    let l:line = 1
    while l:line <= line('$')
        let l:text = getline(l:line)
        if l:text !~# '^\s*\%(#.*\)\?$'
            break
        endif
        if l:text =~# '^\s*#tab-size\%(\s\|$\)'
            let l:match = matchlist(l:text, '^\s*#tab-size\s\+\([0-9]\+\)\s*$')
            if !l:seen && !empty(l:match)
                let l:value = str2nr(l:match[1])
                if l:value >= 1 && l:value <= 32
                    let l:width = l:value
                endif
            endif
            let l:seen = 1
        endif
        let l:line += 1
    endwhile
    let &l:tabstop = l:width
    if exists('+vartabstop')
        setlocal vartabstop=
    endif
endfunction
