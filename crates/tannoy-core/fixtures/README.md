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
