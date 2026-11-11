<p align="center">
  <img src="docs/assets/mark.svg" width="46" alt="">
</p>

<h1 align="center">Tannoy</h1>

<p align="center">
  <img src="docs/assets/hero.svg" width="100%" alt="A floating waybar whose media pill shows a small podcast cover and Slow Signals · Where the Night Trains…, cut at a word with a real ellipsis. A click on the cover opens the Tannoy drawer out of it: the podcast playing in Chrome with its scrub bar moving, play and pause, previous and next, and 15 second skips. A click on +15 jumps ahead. A click on the Spotify chip swaps the cover, the title and the bar pill to a song, the skips go away, and play starts it. Do not disturb turns on: the bell turns amber, the bar keeps only a dim music glyph, and the drawer says the title is hidden while its controls stay. Do not disturb turns off again, and a click outside closes the drawer.">
</p>

<p align="center">
  <b>Now playing in your bar, with the cover beside it. Click it and the player slides out of that icon.</b><br>
  Every MPRIS player at once, the one you picked answering the media keys,<br>
  a scrub bar and podcast skips, and titles that stay off your screen shares.
</p>

<p align="center">
  <a href="https://github.com/NathanielInman/tannoy/actions/workflows/ci.yml"><img src="https://github.com/NathanielInman/tannoy/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/NathanielInman/tannoy/releases/latest"><img src="https://img.shields.io/github/v/release/NathanielInman/tannoy?color=88c0d0&amp;label=release" alt="Latest release"></a>
  <img src="https://img.shields.io/badge/status-early%20(0.1)-ebcb8b" alt="Early, version 0.1">
  <img src="https://img.shields.io/badge/Arch%20%2B%20Hyprland%20first-Linux%20%C2%B7%20Wayland-a3be8c" alt="Arch and Hyprland first; Linux with Wayland">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-b48ead" alt="MIT license"></a>
</p>

A tannoy is the station loudspeaker that tells the platform what is arriving.
[Switchyard](https://github.com/NathanielInman/switchyard) dispatches your
work, [Roundhouse](https://github.com/NathanielInman/roundhouse) says whether
the machine is fit to run it, and Tannoy tells you what is playing.

## Why

My bar had a `currently-playing.sh` that ran `playerctl` five times every
three seconds and cut every field at 20 characters, mid-word, which is how a
podcast became "StarTalk - Why We Haven't Reach". With Chrome and Spotify both
open, playerctl picked one and I didn't get a say in which. There was no
position, no way to skip back 15 seconds in a podcast, and no cover. That
script is what Tannoy replaces.

- **Signals, not polling.** The daemon listens to MPRIS over D-Bus: a player
  appearing, its track or state changing, a seek. The bar redraws when
  something changes and costs nothing in between. The position is the one
  thing MPRIS never announces, so Tannoy reads it once a second while the
  drawer is open and never otherwise.
- **The player you meant.** Every `org.mpris.MediaPlayer2.*` player is
  tracked. The bar, the media keys and the drawer all follow the player you
  **picked** while it is running, else the one that most recently started
  playing. A pick survives the player restarting.
- **Text that ends on a word.** Artist and title are cut at a word boundary
  with a real `…`, never in the middle of one (40 characters by default).
- **Cover art in the bar.** `file://` covers (Chrome, local players) and
  `https://` ones (Spotify) are cached on disk, and a waybar `image` module
  shows them. The daemon tells waybar when the cover changes; nothing polls.
- **Private on a screen share.** While do not disturb is on (swaync, dunst
  or mako), the bar
  shows a dim music glyph instead of the title, the cover goes away, and the
  drawer says why. Under Hyprland a screen share does the same on its own,
  so you don't have to remember do not disturb before you share.
- **It opens from the icon you clicked.** The drawer springs out of the bar
  on whichever monitor you clicked, using the same anchoring code as
  Roundhouse and Switchyard, and it takes its font and colors from your
  waybar `style.css`.

## What's in the drawer

The cover, the title, artist and album, and a scrub bar you can drag to seek,
with the position and length under it. Previous, play/pause and next, plus
-15s and +15s when the track looks like a podcast (a Spotify episode, a
podcast genre, or over 20 minutes with no album, which is what a podcast in a
browser tab looks like). A chip per player: click one to pick it. The default
output device with a button that runs your switcher. <kbd>Space</kbd> plays
and pauses, <kbd>←</kbd>/<kbd>→</kbd> seek 5 seconds, <kbd>n</kbd>/<kbd>p</kbd>
skip, and <kbd>Esc</kbd> or a click outside closes it. A control that
cannot work (a player with no next track, or one that cannot seek) says why
at the top of the drawer for a few seconds. A click on the cover closes the
drawer and brings the player's window forward, for players that offer it
over MPRIS (Spotify, mpv and VLC do; Chrome does not).

`tannoy status` prints the same thing in a terminal, and `tannoy status
--json` gives it to scripts.

## Install

Tannoy is at 0.1. It runs on Linux with Wayland, and Hyprland gets the
exact-icon anchoring. Packages come later; for now grab a tarball from the
[releases](https://github.com/NathanielInman/tannoy/releases) (each one also
carries the man page, shell completions, the systemd user unit and example
configs in `examples/`) or build from source:

```sh
# Arch: the drawer's libraries
sudo pacman -S --needed gtk4 gtk4-layer-shell

git clone https://github.com/NathanielInman/tannoy
cd tannoy
cargo build --release
install -Dm755 target/release/tannoy target/release/tannoy-board -t ~/.local/bin/
tannoy doctor
```

To upgrade, install the new binaries the same way. The running daemon,
drawer and bar module notice that their binary was replaced and switch to
the new one on the next media key or player event; `tannoy restart` does it
at once. (Coming from 0.1, stop the old daemon once with
`pkill -f 'tannoy daemon'`.)

To have the daemon start with your session instead of the first time waybar
runs `tannoy bar` (optional), enable the user unit. The packages install it;
for a `~/.local/bin` install, copy
[`packaging/systemd/tannoy.service`](packaging/systemd/tannoy.service) to
`~/.config/systemd/user/` and point `ExecStart` at `%h/.local/bin/tannoy`.

```sh
systemctl --user enable --now tannoy.service
```

systemd restarts it after a crash, an upgrade or `tannoy restart`. If the
bar or a command starts a daemon first, the unit's own start finds it
running and stops without an error: either way one daemon runs.

The drawer's output row reads the default device with `wpctl` (PipeWire).
Do not disturb comes from whichever notification daemon runs:
[swaync](https://github.com/ErikReider/SwayNotificationCenter),
[dunst](https://dunst-project.org) (paused) or
[mako](https://github.com/emersion/mako) (a `do-not-disturb` mode, or the
one `dnd.mako_mode` names). With none of them, do not disturb counts as off.

## Put it in your bar

Two modules in a group, so they share a pill: the cover and the text.

```jsonc
"modules-right": ["group/pill-media", "..."],

"group/pill-media": {
  "orientation": "horizontal",
  "modules": ["image#tannoy-art", "custom/tannoy"]
},
"image#tannoy-art": {
  "exec": "~/.local/bin/tannoy art",
  "size": 18,
  "interval": "once",
  "signal": 12,
  "on-click": "~/.local/bin/tannoy toggle"
},
"custom/tannoy": {
  "exec": "~/.local/bin/tannoy bar",
  "return-type": "json",
  "on-click": "~/.local/bin/tannoy toggle",
  "on-click-right": "~/.local/bin/tannoy next",
  "on-click-middle": "~/.local/bin/tannoy play-pause",
  "on-scroll-up": "~/.local/bin/tannoy seek +5",
  "on-scroll-down": "~/.local/bin/tannoy seek -5",
  "tooltip": true
}
```

Use full paths: a waybar started by systemd usually does not have
`~/.local/bin` on its `PATH`. `tannoy bar` starts the daemon the first time
it runs, and the daemon starts the drawer the first time you click.

The image module runs `tannoy art` once and again whenever the daemon sends
waybar `SIGRTMIN+12` (`art.signal`), which it does when the cover changes.
With no cover `tannoy art` prints a path that never exists, and waybar
hides the module (an empty line would leave the last cover up). Keep
`signal` in the module and `art.signal` in the config the same.

The text module's class is `playing`, `paused`, `hidden` (do not disturb) or
`stopped`, and its text is empty when nothing plays, which hides it:

```css
#custom-tannoy.playing { color: #ebcb8b; }
#custom-tannoy.paused,
#custom-tannoy.hidden { color: #616e88; }
```

Waybar cannot hide a group's padding and border when every module in it is
empty, so a pill drawn on the group would sit there empty whenever nothing
plays. Draw it on the two modules instead: the text module also gets the
class `with-art` while the image module shows a cover, so the text can be a
whole pill on its own and hand its left end to the cover.

```css
#custom-tannoy { background: #282a2e; border: 1px solid #4c566a; border-radius: 10px; padding: 0 12px; }
#custom-tannoy.with-art { border-left: none; border-radius: 0 10px 10px 0; padding-left: 8px; }
#image.tannoy-art image { background: #282a2e; border: 1px solid #4c566a; border-right: none; border-radius: 10px 0 0 10px; padding-left: 10px; }
```

For the media keys, point them at the same commands. In Hyprland's Lua
config:

```lua
hl.bind("XF86AudioPlay", hl.dsp.exec_cmd("~/.local/bin/tannoy play-pause"))
hl.bind("XF86AudioNext", hl.dsp.exec_cmd("~/.local/bin/tannoy next"))
hl.bind("XF86AudioPrev", hl.dsp.exec_cmd("~/.local/bin/tannoy previous"))
```

## Commands

| Command | What it does |
|---|---|
| `tannoy play-pause`, `next`, `previous` | Sent to the picked player |
| `tannoy raise` | Brings the picked player's window forward, if it can |
| `tannoy seek +5`, `seek -15`, `seek 90` | Relative with a sign, else to that second |
| `tannoy pick spotify`, `pick auto` | Pick by name, key or prefix; `auto` follows whichever started playing last |
| `tannoy toggle`, `show`, `hide` | The drawer, from the clicked icon (`--center` to center it) |
| `tannoy bar`, `tannoy art` | The two waybar modules |
| `tannoy status [--json]` | What is playing, on which players |
| `tannoy doctor` | Checks D-Bus, the notification daemon, screen shares, waybar, the art signal and the drawer |
| `tannoy restart` | Restarts the daemon and the drawer |

`man tannoy` has the details. Shell completions for bash, zsh and fish (which
complete `pick` with the running players) are in
[`packaging/completions`](packaging/completions); the packages install them.

## Config

Everything is optional. `~/.config/tannoy/config.toml`:

```toml
[bar]
max_chars = 40         # artist · title, cut at a word boundary
separator = " · "
icon_playing = ""      # a glyph before the text while playing, e.g. ""
icon_paused = ""       # and while paused
icon_hidden = "\U000F075A"  # what do not disturb leaves in the bar (md-music)
namespace = "waybar"   # the bar's layer-shell namespace

[dnd]
hide = true            # hide title and cover while do not disturb is on
text = ""              # text after icon_hidden, e.g. "hidden"
screen_share = true    # also hide title and cover while the screen is shared (Hyprland)
mako_mode = "do-not-disturb"  # the mako mode that means do not disturb

[art]
enabled = true
signal = 12            # waybar gets SIGRTMIN+12 when the cover changes; 0 = never
fallback = "none"      # no cover: "none" hides the module, "player" shows the app icon, or an image path
timeout = 5            # seconds for an https download

[players]
ignore = []            # bus-name keys never tracked, e.g. ["firefox"]; playerctld always is

[drawer]
width = 400
max_height = 0.85      # largest height, as a fraction of the monitor
gap = 8                # between the bar and the panel
skip = 15              # seconds the podcast buttons jump
skips = "auto"         # "auto" (podcasts), "always" or "never"

[output]
command = "~/.config/waybar/scripts/audio-switch.sh output"   # "" hides the button

[theme]
from = "waybar"        # or "none" for the built-in theme
# accent = "#88c0d0"
# font = "JetBrains Mono"
```

A typo in the config falls back to the defaults and the drawer says what is
wrong at the top. `~/.config/tannoy/style.css` overrides any part of the
look; redefining one of the `@define-color tn_*` tokens in
[style.css](crates/tannoy-board/src/style.css) recolors everything that uses
it.

## Siblings

Tannoy is the fourth of a family, each one module in the bar you already
have: [Switchyard](https://github.com/NathanielInman/switchyard) (builds and
launches), [Roundhouse](https://github.com/NathanielInman/roundhouse) (the
machine) and Signal Box (credentials). They share `bar-anchor`, so every
drawer opens from its own icon with the same motion, and the same `Anchor`
JSON, so one could hand another a click. None of them needs the others.

## Cost

The daemon holds one D-Bus connection and sleeps until a player or the
notification daemon sends a signal. It starts no processes to sample anything. Cover downloads
run on a helper thread with a timeout and never hold up the bar. With the
drawer open it adds one `Position` read a second.

## Development

```sh
cargo test --workspace
tools/e2e.sh   # the daemon, bar and CLI against fake players on a private bus
# A dev daemon that leaves an installed one alone:
export TANNOY_RUNTIME_DIR=/tmp/tn-dev/run TANNOY_STATE_DIR=/tmp/tn-dev/state \
       TANNOY_CACHE_DIR=/tmp/tn-dev/cache TANNOY_CONFIG_DIR=/tmp/tn-dev/config
cargo run -p tannoy -- daemon &
# A player to poke at without touching real media:
tools/fake-player.py --title "Get Lucky" --title "Instant Crush" --artist "Daft Punk"
cargo build -p tannoy-board && cargo run -p tannoy -- toggle --center
```

See [AGENTS.md](AGENTS.md) for the crate layout and
[docs/design.md](docs/design.md) for how it works.

## License

MIT
