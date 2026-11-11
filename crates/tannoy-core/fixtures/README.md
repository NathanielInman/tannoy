# Fixtures

Recorded with `busctl --user --json=pretty call <bus name>
/org/mpris/MediaPlayer2 org.freedesktop.DBus.Properties GetAll s <interface>`
and `wpctl`:

| File | Source |
|---|---|
| `chrome-player.json`, `chrome-root.json` | Chrome 157 with a StarTalk episode paused in a tab |
| `fake-player.json` | `tools/fake-player.py` with two artists and a genre |
| `spotify-player.json` | Spotify's field set and types (`mpris:length` as `t`, an `/com/spotify/episode/` track id, `i.scdn.co` art), written by hand: Spotify was not installed when the others were recorded. Replace it with a real dump when one is at hand. |
| `wpctl-inspect-bluetooth.txt`, `wpctl-volume.txt` | `wpctl inspect @DEFAULT_AUDIO_SINK@` and `wpctl get-volume` with a Bluetooth speaker as the default sink |
| `hyprland-screencast.txt` | The `screencast` and `screencastv2` lines Hyprland 0.57 wrote to `.socket2.sock` while `wf-recorder` ran for 3 seconds (a pair per monitor), then `grim` took a screenshot (nothing) |
| `dunst-signals.jsonl` | `busctl --json=short monitor` of dunst 1.13.2 on a private bus during `dunstctl set-paused true` then `false` |
| `mako-signals.jsonl`, `mako-modes-dnd.json`, `mako-modes-default.json` | mako 1.11.0 on a private bus: the PropertiesChanged signals for `makoctl mode -a do-not-disturb` then `-r`, and `busctl get-property ... Modes` in each state |
