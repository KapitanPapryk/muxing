# bash completion for muxing
_muxing() {
    local cur=${COMP_WORDS[COMP_CWORD]}
    local cmds="start stop list new edit delete check autostart autostop service completions dir help"
    local cmd="" i

    # First non-option word: a subcommand or a project name.
    for ((i = 1; i < COMP_CWORD; i++)); do
        if [[ ${COMP_WORDS[i]} != -* ]]; then
            cmd=${COMP_WORDS[i]}
            break
        fi
    done

    if [[ -z $cmd ]]; then
        if [[ $cur == -* ]]; then
            COMPREPLY=($(compgen -W "-d --detached --help --version" -- "$cur"))
        else
            COMPREPLY=($(compgen -W "$cmds $(command muxing list --names 2>/dev/null)" -- "$cur"))
        fi
        return
    fi

    case $cmd in
        start | s | stop | edit | e | delete | rm | check)
            if [[ $cur == -* ]]; then
                local opts=""
                case $cmd in
                    start | s) opts="-d --detached" ;;
                    delete | rm) opts="-y --yes" ;;
                esac
                COMPREPLY=($(compgen -W "$opts --help" -- "$cur"))
            else
                COMPREPLY=($(compgen -W "$(command muxing list --names 2>/dev/null)" -- "$cur"))
            fi
            ;;
        service)
            if [[ ${COMP_WORDS[COMP_CWORD - 1]} == service ]]; then
                COMPREPLY=($(compgen -W "install uninstall status" -- "$cur"))
            elif [[ ${COMP_WORDS[COMP_CWORD - 1]} == install ]]; then
                COMPREPLY=($(compgen -W "--now" -- "$cur"))
            fi
            ;;
        completions)
            [[ ${COMP_WORDS[COMP_CWORD - 1]} == completions ]] && COMPREPLY=($(compgen -W "bash fish" -- "$cur"))
            ;;
        list | ls | new | autostart | autostop | dir | help) ;;
        *)
            # `muxing <project> ...`
            COMPREPLY=($(compgen -W "-d --detached" -- "$cur"))
            ;;
    esac
}
complete -F _muxing muxing
