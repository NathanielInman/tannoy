# Tannoy

`docs/design.md` is the spec; the README is the tour. Read the spec section
for whatever you change.

## Crates

- `bar-anchor`: Hyprland anchoring and panel placement, no GTK. A copy of
  Roundhouse's; its `Anchor` JSON must stay identical to Roundhouse's and
  Switchyard's.
- `tannoy-core`: the model, MPRIS parsing (`mpris.rs`), the player pick
  (`pick.rs`), the daemon's state and its transitions (`state.rs`: what
  happened in, effects out), the bar line (`bar.rs`), config, cover art, the
  socket protocol, the waybar reader and the swaync, dunst, mako and wpctl
  parsers. No D-Bus, no UI.
- `tannoy`: the daemon (`daemon.rs`: threads, sockets and D-Bus calls around
  `state::Model`), the zbus glue (`dbus.rs`), the CLI and the waybar module
  (one binary, `tannoy`). A new rule about what the daemon shows belongs in
  `state.rs` with a test, not here.
- `tannoy-board`: the Linux GTK4 layer-shell drawer. `anim.rs` and
  `stage.rs` came from Roundhouse's board.

## Build and test

`cargo build --workspace --locked`, `cargo test --workspace --locked`,
`cargo clippy --workspace --all-targets -- -D warnings` and `tools/e2e.sh`
(the real daemon, bar and CLI against fake players on a private bus; CI
runs it too). CI also runs `cargo deny check` (`deny.toml`: advisories,
licenses, bans, sources); an ignored advisory needs a reason there. The drawer needs
gtk4-layer-shell; `.github/scripts/gtk4-layer-shell.sh` is how CI gets it.

Parsers are pure functions over plain data and are tested against real
samples in `crates/tannoy-core/fixtures/` (busctl `GetAll` dumps, a busctl
monitor captures of swaync, dunst and mako, wpctl output, waybar CSS). D-Bus values become
`MValue`s in `dbus.rs`; everything after that is testable without a bus.
Add a recorded sample when you touch a parser.

`tools/fake-player.py` owns an MPRIS name and behaves like a player (it
needs python-gobject). Use it instead of pressing play on real media.

## Isolation

The machine you work on may run an installed Tannoy. Point a dev build
somewhere else with `TANNOY_RUNTIME_DIR`, `TANNOY_STATE_DIR` and
`TANNOY_CACHE_DIR` (and `TANNOY_CONFIG_DIR` for a test config). A dev config
with `[art] signal = 0` keeps a dev daemon from signalling the real waybar.

## Rules

- No subprocesses and no polling in the daemon. Metadata comes from
  signals; the position is read only while the drawer is open.
- Nothing may stall the bar stream. D-Bus calls have a 3 second timeout and
  run on helper or connection threads, never while a signal stream waits;
  downloads run on their own thread.
- `tannoy art` runs on waybar's main thread: it must answer in well under a
  second and never start the daemon.
- Do not disturb and screen shares hide titles and covers everywhere they
  reach the screen. A new surface that shows track text checks
  `snapshot.hidden`.
- Hands off other programs: Tannoy reads notification daemons, Hyprland
  and waybar but never changes their state (no `SetDnd`, no compositor
  commands), and never calls a name nobody owns (the bus could start it). Users
  run different tools alongside it; the only things it sends are the player
  controls the user asks for, waybar's redraw signal and the user's own
  `output.command`.
- The notification daemon (swaync, dunst, mako) is optional. None running
  means do not disturb is off, never an error.

## When behavior changes

Update in the same commit:

- `docs/design.md` for a new config key, protocol message, signal or rule.
- The README for anything a new user meets (CLI, keys, install, config).
- `docs/assets/hero.py` when the drawer or the bar module looks different,
  then run `python3 docs/assets/hero.py` to rewrite `hero.svg` (never edit
  the SVG by hand).

Commits are conventional, scoped by area: `anchor`, `core`, `mpris`,
`pick`, `art`, `cli`, `daemon`, `board`, `docs`, `ci`.
