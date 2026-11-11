# bash completion for tannoy
_tannoy_players() {
    tannoy status --json 2>/dev/null | grep -o '"key": *"[^"]*"' | sed 's/.*"\([^"]*\)"$/\1/'
}

_tannoy() {
    local cur=${COMP_WORDS[COMP_CWORD]}
    local commands="play-pause next previous seek raise pick toggle show hide bar art status doctor daemon restart help --version"
    if [[ $COMP_CWORD -eq 1 ]]; then
        COMPREPLY=($(compgen -W "$commands" -- "$cur"))
        return
    fi
    case ${COMP_WORDS[1]} in
        pick) COMPREPLY=($(compgen -W "auto $(_tannoy_players)" -- "$cur")) ;;
        toggle | show) COMPREPLY=($(compgen -W "--center" -- "$cur")) ;;
        status) COMPREPLY=($(compgen -W "--json" -- "$cur")) ;;
        seek) COMPREPLY=($(compgen -W "+5 -5 +15 -15 +30 -30" -- "$cur")) ;;
    esac
}

complete -F _tannoy tannoy
