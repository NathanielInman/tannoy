<h1 align="center">Tannoy</h1>

<p align="center">
  <b>Now playing in your bar, with the cover beside it. Click it and the player slides out of that icon.</b><br>
  Every MPRIS player at once, the one you picked answering the media keys,<br>
  a scrub bar and podcast skips, and titles that stay off your screen shares.
</p>

<p align="center">
  <a href="https://github.com/NathanielInman/tannoy/actions/workflows/ci.yml"><img src="https://github.com/NathanielInman/tannoy/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <img src="https://img.shields.io/badge/status-early%20(0.1)-ebcb8b" alt="Early, version 0.1">
  <img src="https://img.shields.io/badge/Arch%20%2B%20Hyprland%20first-Linux%20%C2%B7%20Wayland-a3be8c" alt="Arch and Hyprland first; Linux with Wayland">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-88c0d0" alt="MIT license"></a>
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
- **Private on a screen share.** While swaync's do not disturb is on, the bar
  shows a dim music glyph instead of the title, the cover goes away, and the
  drawer says why. This is the rule the old script had, kept.
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
skip, and <kbd>Esc</kbd> or a click outside closes it.

`tannoy status` prints the same thing in a terminal, and `tannoy status
--json` gives it to scripts.

## Install

Tannoy is at 0.1. It runs on Linux with Wayland, and Hyprland gets the
exact-icon anchoring. Packages come later; for now grab a tarball from the
[releases](https://github.com/NathanielInman/tannoy/releases) or build from
source:

```sh
# Arch: the drawer's libraries
sudo pacman -S --needed gtk4 gtk4-layer-shell

git clone https://github.com/NathanielInman/tannoy
cd tannoy
cargo build --release
install -Dm755 target/release/tannoy target/release/tannoy-board -t ~/.local/bin/
tannoy doctor
```

The drawer's output row reads the default device with `wpctl` (PipeWire).
Do not disturb comes from [swaync](https://github.com/ErikReider/SwayNotificationCenter);
without it, do not disturb counts as off.

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
`tannoy art` prints nothing when there is no cover, and waybar hides the
module. Keep `signal` in the module and `art.signal` in the config the same.

The text module's class is `playing`, `paused`, `hidden` (do not disturb) or
`stopped`, and its text is empty when nothing plays, which hides it:

```css
#custom-tannoy.playing { color: #ebcb8b; }
#custom-tannoy.paused,
#custom-tannoy.hidden { color: #616e88; }
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
| `tannoy seek +5`, `seek -15`, `seek 90` | Relative with a sign, else to that second |
| `tannoy pick spotify`, `pick auto` | Pick by name, key or prefix; `auto` follows whichever started playing last |
| `tannoy toggle`, `show`, `hide` | The drawer, from the clicked icon (`--center` to center it) |
| `tannoy bar`, `tannoy art` | The two waybar modules |
| `tannoy status [--json]` | What is playing, on which players |
| `tannoy doctor` | Checks D-Bus, swaync, waybar, the art signal and the drawer |

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
hide = true            # hide title and cover while swaync's do not disturb is on
text = ""              # text after icon_hidden, e.g. "hidden"

[art]
enabled = true
signal = 12            # waybar gets SIGRTMIN+12 when the cover changes; 0 = never
fallback = "none"      # no cover: "none" hides the module, "player" shows the app icon, or an image path
timeout = 5            # seconds for an https download

[players]
ignore = []            # bus-name keys never tracked, e.g. ["firefox"]; playerctld always is

[drawer]
width = 400
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

The daemon holds one D-Bus connection and sleeps until a player or swaync
sends a signal. It starts no processes to sample anything. Cover downloads
run on a helper thread with a timeout and never hold up the bar. With the
drawer open it adds one `Position` read a second.

## Development

```sh
cargo test --workspace
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
