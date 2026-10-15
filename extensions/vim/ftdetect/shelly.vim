augroup ShellyFiletype
    autocmd!
    autocmd BufRead,BufNewFile *.shy setfiletype shelly
    autocmd BufRead,BufNewFile * if getline(1) =~# '^#!.*\<shelly\>\%(\s\|$\)'
                \ | setfiletype shelly | endif
augroup END
