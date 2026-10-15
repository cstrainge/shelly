" A timer observes real typing without leaving Insert mode or dismissing completion.
let s:events = {'insert': 0, 'popup': 0, 'leave': 0}
augroup ShellyInsertFixture
    autocmd!
    autocmd TextChangedI * let s:events.insert += 1
    autocmd TextChangedP * let s:events.popup += 1
    autocmd InsertLeave * let s:events.leave += 1
augroup END
set completeopt=menuone,noinsert,noselect
call LspOptionsSet({'autoComplete': v:false})

function! s:observe(timer) abort
    try
        let l:report = {'ready': LspServerReady(), 'mode': mode(1),
                    \ 'lines': getline(1, '$'), 'popup': pumvisible(),
                    \ 'properties': map(range(1, line('$')), {_, row -> prop_list(row)}),
                    \ 'events': copy(s:events), 'messages': execute('messages')}
    catch
        let l:report = {'exception': v:exception, 'throwpoint': v:throwpoint}
    endtry
    let l:path = $SHELLY_TEST_WORK . '/insert.json'
    call writefile([json_encode(l:report)], l:path . '.tmp')
    call rename(l:path . '.tmp', l:path)
endfunction

autocmd VimEnter * call timer_start(20, function('s:observe'), {'repeat': -1})
