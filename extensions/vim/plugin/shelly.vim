" Shelly uses the same server as the REPL and VS Code extension.
if exists('g:loaded_shelly_shell')
    finish
endif
let g:loaded_shelly_shell = 1

command! ShellyLspStart call shelly#start()
command! ShellyLspRestart call shelly#restart()

augroup ShellyLsp
    autocmd!
    autocmd User LspSetup if get(g:, 'shelly_auto_start', 1) | call shelly#setup() | endif
    autocmd User LspAttached call shelly#semantic#Translate()
    autocmd FileType shelly if get(g:, 'shelly_auto_start', 1) | call shelly#start() | endif
augroup END

" A system vimrc may have enabled detection before this runtime path was added.
runtime ftdetect/shelly.vim

if exists('*LspAddServer') && get(g:, 'shelly_auto_start', 1)
    call shelly#setup()
endif
