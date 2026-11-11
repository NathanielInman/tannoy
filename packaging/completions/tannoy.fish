# fish completion for tannoy
function __tannoy_players
    tannoy status --json 2>/dev/null | string match -r -g '"key": *"([^"]*)"'
end

set -l commands play-pause next previous seek raise shuffle loop pick toggle show hide bar art status doctor daemon restart help
complete -c tannoy -f
complete -c tannoy -n "not __fish_seen_subcommand_from $commands" -l version -d 'print the version'
complete -c tannoy -n "not __fish_seen_subcommand_from $commands" -a play-pause -d 'play or pause the picked player'
complete -c tannoy -n "not __fish_seen_subcommand_from $commands" -a next -d 'skip to the next track'
complete -c tannoy -n "not __fish_seen_subcommand_from $commands" -a previous -d 'skip to the previous track'
complete -c tannoy -n "not __fish_seen_subcommand_from $commands" -a seek -d 'seek relative (+5, -15) or to a position (90)'
complete -c tannoy -n "not __fish_seen_subcommand_from $commands" -a raise -d "bring the picked player's window forward"
complete -c tannoy -n "not __fish_seen_subcommand_from $commands" -a shuffle -d 'shuffle the picked player (no argument: flip it)'
complete -c tannoy -n "not __fish_seen_subcommand_from $commands" -a loop -d "set the picked player's repeat (no argument: next)"
complete -c tannoy -n "not __fish_seen_subcommand_from $commands" -a pick -d 'send commands to this player'
complete -c tannoy -n "__fish_seen_subcommand_from shuffle" -a "on off"
complete -c tannoy -n "__fish_seen_subcommand_from loop" -a "none track playlist"
complete -c tannoy -n "not __fish_seen_subcommand_from $commands" -a toggle -d 'open or close the drawer from the clicked icon'
complete -c tannoy -n "not __fish_seen_subcommand_from $commands" -a show -d 'open the drawer'
complete -c tannoy -n "not __fish_seen_subcommand_from $commands" -a hide -d 'close the drawer'
complete -c tannoy -n "not __fish_seen_subcommand_from $commands" -a bar -d 'the waybar module (JSON lines)'
complete -c tannoy -n "not __fish_seen_subcommand_from $commands" -a art -d "the cover's path, for a waybar image module"
complete -c tannoy -n "not __fish_seen_subcommand_from $commands" -a status -d 'what is playing, on which players'
complete -c tannoy -n "not __fish_seen_subcommand_from $commands" -a doctor -d 'check the setup'
complete -c tannoy -n "not __fish_seen_subcommand_from $commands" -a daemon -d 'run the daemon in the foreground'
complete -c tannoy -n "not __fish_seen_subcommand_from $commands" -a restart -d 'restart the daemon and the drawer'
complete -c tannoy -n "not __fish_seen_subcommand_from $commands" -a help -d 'show the usage'
complete -c tannoy -n "__fish_seen_subcommand_from pick" -a "auto (__tannoy_players)"
complete -c tannoy -n "__fish_seen_subcommand_from toggle show" -l center -d 'open centered instead of from the icon'
complete -c tannoy -n "__fish_seen_subcommand_from status" -l json -d 'print the snapshot as JSON'
