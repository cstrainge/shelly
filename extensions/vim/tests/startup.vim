" Observe an ordinary terminal startup, without calling LspEnable ourselves.
let s:started = reltime()

function! s:observe(timer) abort
    let l:ready = LspServerReady()
    let l:tokens = len(get(b:, 'LspSemanticTokensData', []))
    if (!l:ready || !l:tokens) && reltimefloat(reltime(s:started)) < 8
        return
    endif
    let l:report = {'ready': l:ready, 'tokens': l:tokens,
                \ 'properties': prop_list(2), 'filetype': &filetype,
                \ 'keyword_color': synIDattr(synIDtrans(hlID('LspSemanticKeyword')), 'fg', 'cterm'),
                \ 'messages': execute('messages')}
    call writefile([json_encode(l:report)], $SHELLY_TEST_WORK . '/startup.json')
    call timer_stop(a:timer)
    qa!
endfunction

autocmd VimEnter * call timer_start(20, function('s:observe'), {'repeat': -1})
