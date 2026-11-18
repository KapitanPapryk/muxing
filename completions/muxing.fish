# fish completion for muxing
set -l __muxing_cmds start s stop list ls new edit e delete rm check autostart autostop service completions dir help

# True while completing the first non-option argument (subcommand or project name).
function __muxing_first_arg
    set -l tokens (commandline -xpc 2>/dev/null; or commandline -opc)
    for t in $tokens[2..]
        string match -q -- '-*' $t; or return 1
    end
end

# True when the first argument is a project name, i.e. `muxing <project> ...`.
function __muxing_project_given
    set -l tokens (commandline -xpc 2>/dev/null; or commandline -opc)
    for t in $tokens[2..]
        string match -q -- '-*' $t; and continue
        contains -- $t start s stop list ls new edit e delete rm check autostart autostop service completions dir help
        and return 1
        return 0
    end
    return 1
end

complete -c muxing -f

# `muxing <project>`
complete -c muxing -n __muxing_first_arg -a "(command muxing list --names 2>/dev/null)" -d Project
complete -c muxing -n "__muxing_first_arg; or __muxing_project_given" -s d -l detached -d "Don't attach"

complete -c muxing -n __muxing_first_arg -a start -d 'Start a project and attach'
complete -c muxing -n __muxing_first_arg -a stop -d "Kill a project's session"
complete -c muxing -n __muxing_first_arg -a list -d 'List projects'
complete -c muxing -n __muxing_first_arg -a new -d 'Create a new project'
complete -c muxing -n __muxing_first_arg -a edit -d 'Edit a project'
complete -c muxing -n __muxing_first_arg -a delete -d 'Delete a project'
complete -c muxing -n __muxing_first_arg -a check -d 'Validate projects'
complete -c muxing -n __muxing_first_arg -a autostart -d 'Start autostart projects'
complete -c muxing -n __muxing_first_arg -a autostop -d 'Stop autostart projects'
complete -c muxing -n __muxing_first_arg -a service -d 'Manage the systemd service'
complete -c muxing -n __muxing_first_arg -a completions -d 'Print shell completions'
complete -c muxing -n __muxing_first_arg -a dir -d 'Print the config directory'

# Project names (file names without .yml) after subcommands that take one
complete -c muxing -n "__fish_seen_subcommand_from start s stop edit e delete rm check" -a "(command muxing list --names 2>/dev/null)" -d Project
complete -c muxing -n "__fish_seen_subcommand_from start s" -s d -l detached -d "Don't attach"
complete -c muxing -n "__fish_seen_subcommand_from delete rm" -s y -l yes -d "Don't ask for confirmation"

complete -c muxing -n "__fish_seen_subcommand_from service; and not __fish_seen_subcommand_from install uninstall status" -a "install uninstall status"
complete -c muxing -n "__fish_seen_subcommand_from service; and __fish_seen_subcommand_from install" -l now -d 'Also start it now'

complete -c muxing -n "__fish_seen_subcommand_from completions" -a "bash fish"
