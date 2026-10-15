if exists('b:did_ftplugin')
    finish
endif
let b:did_ftplugin = 1

setlocal commentstring=#\ %s comments=:# autoindent
setlocal formatoptions-=t
call shelly#tabs()

augroup ShellyBuffer
    autocmd! * <buffer>
    autocmd TextChanged,TextChangedI,BufReadPost <buffer> call shelly#tabs()
augroup END

let b:undo_ftplugin = 'setlocal commentstring< comments< autoindent< formatoptions< tabstop<'
if exists('+vartabstop')
    let b:undo_ftplugin .= ' vartabstop<'
endif
let b:undo_ftplugin .= ' | silent! autocmd! ShellyBuffer * <buffer>'
