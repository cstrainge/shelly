" Only lexical boundaries live here. The server supplies all language roles.
if exists('b:current_syntax')
    finish
endif

syntax match shellyComment /#.*/
syntax match shellyEscape /\\./ contained
syntax region shellyString start=+"+ skip=+\\.+ end=+"+ end=+$+ oneline contains=shellyEscape
syntax region shellyString start=+'+ skip=+\\.+ end=+'+ end=+$+ oneline contains=shellyEscape
syntax region shellyString start=+"\*+ skip=+\\.+ end=+\*"+ contains=shellyEscape
syntax region shellyString start=+'\*+ skip=+\\.+ end=+\*'+ contains=shellyEscape

highlight default link shellyComment Comment
highlight default link shellyString String
highlight default link shellyEscape SpecialChar

let b:current_syntax = 'shelly'
