# Anchoring the drawer under sway and niri

Question: can the drawer open from the clicked bar icon under sway and niri,
the way it does under Hyprland? What does each compositor's IPC expose, and
what would `bar-anchor` need?

Researched 2026-11-11 against sway master
[`1652c54`](https://github.com/swaywm/sway/tree/1652c54b73f67df17b7b4ab0b0f7048204aa8104)
(sway 1.12 is the latest release), niri master
[`7f81a7f`](https://github.com/niri-wm/niri/tree/7f81a7f77b0e93464645232bd15ba2dc43ca3782)
(26.04 is the latest release) and Waybar master
[`2318da4`](https://github.com/Alexays/Waybar/tree/2318da41bdce5ebb99c6f9115acc27d28f1bdc7b)
(0.16.0 is the latest release). Links below are permalinks to those commits.

## Short answer

Neither sway nor niri lets an IPC client read the cursor position, so
neither can match Hyprland's exact anchoring through IPC alone. Both can
name the right monitor right after a click on the bar. Sway can also give the
bar's edge and the reserved area (from the output and workspace rects); niri
cannot (its layer list has no geometry), so the edge has to come from
Waybar's own config. A compositor-agnostic probe (a transparent full-output
layer surface that reads `wl_pointer.enter`) can recover the exact click
point on both, and fits without changing the `Anchor` JSON.

| | sway (1.8 to 1.12) | niri (25.01+) |
|---|---|---|
| Cursor position | No IPC for it. The only cursor command warps the cursor, it does not read it. | No IPC for it. |
| Outputs | `GET_OUTPUTS`: name, logical `rect` (x, y, w, h), scale, transform, `focused`. | `Outputs` / `FocusedOutput`: name, `logical` {x, y, width, height, scale, transform}. |
| Output under the click | `focused` output, reliable with the default `focus_follows_mouse yes`; not with `focus_follows_mouse no`. | `FocusedOutput`, reliable: a click on the bar focuses the output under the cursor before Waybar sees the click. |
| Layer surfaces | Not listed at all. | `Layers` (25.01+): namespace, output, layer, keyboard interactivity. No geometry, anchors or exclusive zone. |
| Bar edge and reserved area | Indirect: workspace `rect` is the output's usable area shrunk by gaps. | Not exposed. Edge from Waybar's `position`; reserved size unknown. |
| Best via IPC alone | Right monitor, right edge, centered along the bar. | Right monitor, edge from Waybar config, centered along the bar. |
| Best with the probe | Exact icon (unless the cursor is hidden). | Exact icon. |

## How Hyprland does it today (for reference)

`hypr::anchor` asks Hyprland's socket for `j/cursorpos`, `j/monitors` and
`j/layers`, picks the monitor containing the cursor, finds the bar's layer
surface by namespace to get its edge, and copies the monitor's `reserved`
array (`crates/bar-anchor/src/hypr.rs`). The drawer is an overlay layer
surface anchored to all four edges with exclusive zone 0
(`crates/tannoy-board/src/drawer.rs:200-205`), so the compositor sizes it to
the usable area, and `place` subtracts `reserved[0]`/`reserved[1]` to turn
the monitor-local click into surface coordinates
(`crates/bar-anchor/src/place.rs`). For a top or bottom bar `place` only
reads `x` and `reserved[0]`; for a left or right bar only `y` and
`reserved[1]`. That matters below: a wrong `reserved` only hurts when there
is a second bar on a perpendicular edge.

## sway

### Transport

- The socket path is in `$SWAYSOCK` (and `$I3SOCK` for i3 compatibility), or
  from `sway --get-socketpath`
  ([sway-ipc(7)](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/sway-ipc.7.scd#L12-L16)).
- Framing is binary: the 6-byte magic `i3-ipc`, a 32-bit payload length and a
  32-bit payload type, both in native byte order, then the payload; replies
  use the same header and carry JSON
  ([sway-ipc(7), MESSAGE AND REPLY FORMAT](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/sway-ipc.7.scd#L17-L31)).
  Message numbers: `GET_WORKSPACES = 1`, `GET_OUTPUTS = 3`, `GET_TREE = 4`,
  `GET_SEATS = 101`
  ([include/ipc.h](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/include/ipc.h)).

### 1. Cursor position

Not available. No message in sway-ipc(7) returns it: `GET_SEATS` gives each
seat's name, capabilities, focused node id and devices, nothing about the
pointer
([sway-ipc(7), GET_SEATS](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/sway-ipc.7.scd#L1342-L1365)),
and the IPC JSON code never serializes the cursor (it includes
`sway/input/cursor.h` but emits no cursor field;
[sway/ipc-json.c](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/ipc-json.c)).
The only cursor command is `seat <seat> cursor move|set <x> <y>`, which
moves or warps the cursor
([sway-input(5)](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/sway-input.5.scd#L245-L248));
using it to "read" the position is not possible and moving the user's cursor
is off the table anyway.

Indirect route: the focused output tells which monitor was clicked (next
section), and the probe described under "Compositor-agnostic route" gives the
exact point.

### 2. Outputs and which one has focus

`GET_OUTPUTS` returns, per output, `name` (the DRM connector), `scale`,
`transform` (a string: `normal`, `90`, ..., `flipped-270`), `current_mode`
and `rect` {x, y, width, height}
([sway-ipc(7), GET_OUTPUTS](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/sway-ipc.7.scd#L187-L250)).
The `rect` comes from `wlr_output_layout_get_box`, so it is already in
logical layout coordinates, with scale and transform applied
([sway/config/output.c](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/config/output.c#L694-L700),
[output_get_box](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/tree/output.c#L407-L412)).
Unlike Hyprland, `width`/`height` must not be divided by the scale again.

Each output in the reply also has `focused`, which sway overrides for this
reply to mean "this output holds the default seat's focused workspace"
([sway/ipc-server.c](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/ipc-server.c#L678-L697)).
The override is present in 1.8 and 1.12 (checked at both tags). The field is
not in the man page's property table, so sway-ipc(7)'s "any properties not
listed are subject to removal" applies to it
([sway-ipc(7)](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/sway-ipc.7.scd#L34-L37)).
The documented alternative is `GET_WORKSPACES`: the workspace with
`focused: true` names its `output`
([sway-ipc(7), GET_WORKSPACES](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/sway-ipc.7.scd#L115-L167)).

Does the focused output follow the click on the bar? With the default
`focus_follows_mouse yes`
([sway/config.c](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/config.c#L272),
[sway(5)](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/sway.5.scd#L731-L734)),
pointer motion over a layer surface on another output focuses that output's
active workspace ("This is the case if a layer-shell surface is hovered. If
it's on another output, focus the active workspace there.")
([seatop_default.c](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/input/seatop_default.c#L538-L572),
called from motion only when `focus_follows_mouse != no`,
[L611-L612](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/input/seatop_default.c#L611-L612)).
The same branch exists in sway 1.8 through 1.12 (checked at each tag). So by
the time the user has moved onto the icon and clicked, the bar's output is
the focused one. The exception: a keyboard-interactive layer under the cursor
takes focus instead and returns early (same function, lines 556-563); Waybar
is not keyboard-interactive by default (unconfirmed for every Waybar config).

With `focus_follows_mouse no`, clicking a layer surface does not move focus
unless the layer is keyboard-interactive
([seatop_default.c](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/input/seatop_default.c#L382-L394)),
so a click on another monitor's bar would report the wrong output.

### 3. Layer surfaces, bar edge and reserved area

Sway's IPC does not list layer surfaces: none of the message types in
[include/ipc.h](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/include/ipc.h)
covers them and `GET_TREE` contains outputs, workspaces and containers only
([sway-ipc(7), GET_TREE](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/sway-ipc.7.scd#L297-L345)).
`GET_SEATS` reports focus `0` when a layer surface has focus, but no
geometry.

The reserved area is recoverable indirectly. Sway computes each output's
`usable_area` from layer surfaces' exclusive zones
([sway/desktop/layer_shell.c](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/desktop/layer_shell.c#L80-L97))
and places each workspace at `output.lx + usable.x`, `output.ly + usable.y`
with the usable width and height
([sway/tree/arrange.c](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/tree/arrange.c#L264-L286)).
It then shrinks the workspace by its gaps (`gaps outer` plus `gaps inner`
per side, zeroed by `smart_gaps` when one window is visible)
([workspace_add_gaps](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/tree/workspace.c#L1007-L1056),
called at [arrange.c:306](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/tree/arrange.c#L306)),
and the IPC `rect` of a workspace is that shrunk box
([ipc_json_describe_node](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/ipc-json.c#L809-L834),
[workspace_get_box](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/tree/workspace.c#L1091-L1096)).
The man page's own example shows a 23 px top bar this way: workspace rect
`y: 23, height: 1057` on a 1080 px output
([sway-ipc(7)](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/sway-ipc.7.scd#L150-L166)).

So, for the focused output and its visible workspace:
`inset_left = ws.x - out.x`, `inset_top = ws.y - out.y`,
`inset_right = (out.x + out.w) - (ws.x + ws.w)`,
`inset_bottom = (out.y + out.h) - (ws.y + ws.h)`. Each inset is the exclusive
zone on that side plus that side's gap. The IPC does not expose gaps
directly, so subtracting the smallest inset (the gap on a side with no bar)
is a heuristic: right for uniform gaps and a single bar, wrong when per-side
outer gaps differ. The bar's edge is the side with the largest inset (the
same fallback `hypr.rs` uses when it finds no bar layer). A bar with
`exclusive: false` in Waybar
([waybar(5)](https://github.com/Alexays/Waybar/blob/2318da41bdce5ebb99c6f9115acc27d28f1bdc7b/man/waybar.5.scd.in#L120-L123))
reserves nothing and leaves no trace in these rects.

### 4. Best anchoring under sway

Via IPC alone: correct monitor (with `focus_follows_mouse` on), correct edge
and reserved area (heuristic above), panel centered along the bar. Exact icon
needs the probe. Versions: everything used here is in sway 1.8 through 1.12.

## niri

### Transport

- The socket path is in `$NIRI_SOCKET`
  ([niri-ipc/src/socket.rs](https://github.com/niri-wm/niri/blob/7f81a7f77b0e93464645232bd15ba2dc43ca3782/niri-ipc/src/socket.rs#L12)).
- Newline-delimited JSON: write one JSON `Request` on a line (or flush and
  shut down the write half), read one JSON `Reply` line; more requests may
  follow on the same connection
  ([niri-ipc/src/lib.rs](https://github.com/niri-wm/niri/blob/7f81a7f77b0e93464645232bd15ba2dc43ca3782/niri-ipc/src/lib.rs#L25-L32),
  [wiki: IPC, Programmatic Access](https://github.com/niri-wm/niri/blob/7f81a7f77b0e93464645232bd15ba2dc43ca3782/docs/wiki/IPC.md#L31-L44)).
  A unit request is a bare JSON string, e.g. `"FocusedOutput"`; the reply is
  `{"Ok": {"FocusedOutput": {...}}}` or `{"Err": "..."}`
  ([wiki example](https://github.com/niri-wm/niri/blob/7f81a7f77b0e93464645232bd15ba2dc43ca3782/docs/wiki/IPC.md#L36-L44),
  `Reply = Result<Response, String>`,
  [lib.rs](https://github.com/niri-wm/niri/blob/7f81a7f77b0e93464645232bd15ba2dc43ca3782/niri-ipc/src/lib.rs#L124-L135)).
- The niri-ipc crate is not semver-stable and tracks the niri version
  ([lib.rs](https://github.com/niri-wm/niri/blob/7f81a7f77b0e93464645232bd15ba2dc43ca3782/niri-ipc/src/lib.rs#L34-L45)),
  so `bar-anchor` should parse the few fields it needs with its own serde
  structs (as `hypr.rs` does) instead of depending on the crate.

### 1. Cursor position

Not available. The full `Request` list is `Version`, `Outputs`,
`Workspaces`, `Windows`, `Layers`, `KeyboardLayouts`, `FocusedOutput`,
`FocusedWindow`, `PickWindow`, `PickColor`, `Action`, `Output`,
`EventStream`, `ReturnError`, `OverviewState`, `Casts`
([lib.rs](https://github.com/niri-wm/niri/blob/7f81a7f77b0e93464645232bd15ba2dc43ca3782/niri-ipc/src/lib.rs#L67-L122));
none returns the pointer location, and no response type carries one (the
only "cursor position" mention in niri-ipc is about whether a screencast
shows the cursor). `PickWindow` and `PickColor` wait for the user to click
again, so they are not usable here.

### 2. Outputs and which one has focus

`Outputs` returns a map of name to `Output`; each has `logical:
Option<LogicalOutput>` with `x`, `y`, `width`, `height` (logical pixels),
`scale` and `transform`
([lib.rs](https://github.com/niri-wm/niri/blob/7f81a7f77b0e93464645232bd15ba2dc43ca3782/niri-ipc/src/lib.rs#L1302-L1363)).
The logical size already has scale and transform applied
([src/utils/mod.rs](https://github.com/niri-wm/niri/blob/7f81a7f77b0e93464645232bd15ba2dc43ca3782/src/utils/mod.rs#L201-L230)).
`FocusedOutput` returns the same `Output` for the layout's active output
([src/ipc/server.rs](https://github.com/niri-wm/niri/blob/7f81a7f77b0e93464645232bd15ba2dc43ca3782/src/ipc/server.rs#L425-L445));
it exists at least since v0.1.8 (checked at that tag).

Does the active output follow the click? Yes, regardless of
`focus-follows-mouse`. On a button press niri activates the window under the
cursor if there is one, otherwise it calls `focus_output` on
`output_under_cursor()`
([src/input/mod.rs](https://github.com/niri-wm/niri/blob/7f81a7f77b0e93464645232bd15ba2dc43ca3782/src/input/mod.rs#L3294),
[L3417-L3418](https://github.com/niri-wm/niri/blob/7f81a7f77b0e93464645232bd15ba2dc43ca3782/src/input/mod.rs#L3417-L3418),
[output_under_cursor](https://github.com/niri-wm/niri/blob/7f81a7f77b0e93464645232bd15ba2dc43ca3782/src/niri.rs#L3877-L3880)).
Either way the output under the cursor becomes active, and this happens
before the button is forwarded to the client
([pointer.button at L3466](https://github.com/niri-wm/niri/blob/7f81a7f77b0e93464645232bd15ba2dc43ca3782/src/input/mod.rs#L3466)),
so by the time Waybar runs `on-click`, `FocusedOutput` is the bar's monitor.
Early returns before that point (mouse binds, the MRU switcher, the
screenshot UI) do not apply to a plain click on the bar.

### 3. Layer surfaces, bar edge and reserved area

`Layers` (added in v25.01: absent at v0.1.10, present at v25.01) returns
`LayerSurface { namespace, output, layer, keyboard_interactivity }`
([lib.rs](https://github.com/niri-wm/niri/blob/7f81a7f77b0e93464645232bd15ba2dc43ca3782/niri-ipc/src/lib.rs#L1608-L1620),
[server.rs](https://github.com/niri-wm/niri/blob/7f81a7f77b0e93464645232bd15ba2dc43ca3782/src/ipc/server.rs#L292-L312)).
No position, size, anchors or exclusive zone, so the bar's edge cannot be
derived from niri. It does tell whether a surface with the bar's namespace
exists on a given output. Niri's IPC has no working-area or strut field
either (the only "working area" mentions in niri-ipc are action parameters).

The edge therefore has to come from outside the compositor. Waybar's
`position` key is `top`, `bottom`, `left` or `right`, default `top`
([waybar(5)](https://github.com/Alexays/Waybar/blob/2318da41bdce5ebb99c6f9115acc27d28f1bdc7b/man/waybar.5.scd.in#L57-L64)),
and Tannoy already parses it (`tannoy_core::waybar::parse_config` returns
`BarLayout { position, height, layer }`). Waybar's `height` is a request, not
a guarantee ("Height to be used by the bar if possible"), so the reserved
size is approximate at best.

### 4. Best anchoring under niri

Via IPC alone: correct monitor, edge from Waybar's config, panel centered
along the bar. With a single horizontal bar this is good: `place` only needs
`reserved[0]`, which is 0 without a left bar. Exact icon needs the probe.
Versions: `FocusedOutput` and `Outputs` work on every release since 0.1.8;
`Layers` (only useful as a sanity check) needs 25.01+.

## Waybar: what the click itself passes

- `on-click` commands get no output name. `WAYBAR_OUTPUT_NAME` is set only
  for a custom module's `exec`
  ([waybar-custom(5)](https://github.com/Alexays/Waybar/blob/2318da41bdce5ebb99c6f9115acc27d28f1bdc7b/man/waybar-custom.5.scd#L298-L301));
  click handlers call `forkExec(cmd)`, the overload that passes an empty
  output name
  ([AModule.cpp](https://github.com/Alexays/Waybar/blob/2318da41bdce5ebb99c6f9115acc27d28f1bdc7b/src/AModule.cpp#L285),
  [command.hpp](https://github.com/Alexays/Waybar/blob/2318da41bdce5ebb99c6f9115acc27d28f1bdc7b/include/util/command.hpp#L233-L272)).
- New in Waybar 0.16.0 (released 2026-11-05): `on-click` strings may contain
  `{x}` and `{y}`, replaced by the click position inside the module as an
  integer percent of the module's width and height
  ([AModule.cpp](https://github.com/Alexays/Waybar/blob/2318da41bdce5ebb99c6f9115acc27d28f1bdc7b/src/AModule.cpp#L263-L285),
  [PR #3573](https://github.com/Alexays/Waybar/pull/3573), merged
  2026-07-03). It is not in the man pages yet. It says where inside the
  module the click landed, not where the module is on screen, so it cannot
  anchor the drawer by itself.

## Compositor-agnostic route: a pointer probe

Wayland gives no client the global pointer position, but a client learns the
pointer's surface-local position when the pointer enters one of its own
surfaces (`wl_pointer.enter` carries surface-local x and y). A layer surface
anchored to all four edges with exclusive zone `-1` is extended "all the way
to the edges it is anchored to", ignoring other surfaces' exclusive zones
([wlr-layer-shell, set_exclusive_zone](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/protocols/wlr-layer-shell-unstable-v1.xml#L152-L183);
canonical copy in
[wlr-protocols](https://gitlab.freedesktop.org/wlroots/wlr-protocols/-/blob/master/unstable/wlr-layer-shell-unstable-v1.xml)),
so on the overlay layer it covers the whole output, bar included, and its
surface-local coordinates are monitor-local logical coordinates.

Both compositors re-evaluate pointer focus when such a surface maps under a
still pointer, so the client should get `enter` without the user moving:

- sway calls `cursor_rebase_all()` when a layer surface's mapped state
  changes
  ([layer_shell.c](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/desktop/layer_shell.c#L272-L277)),
  and the default seatop's rebase sends `wlr_seat_pointer_notify_enter` with
  surface-local coordinates, unless the cursor is hidden
  ([seatop_default.c](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/input/seatop_default.c#L1105-L1121)).
- niri runs `refresh_pointer_contents()` on every refresh cycle
  ([niri.rs:889](https://github.com/niri-wm/niri/blob/7f81a7f77b0e93464645232bd15ba2dc43ca3782/src/niri.rs#L889)),
  which recomputes what is under the pointer and sends `pointer.motion` with
  the new focus when it changed
  ([niri.rs](https://github.com/niri-wm/niri/blob/7f81a7f77b0e93464645232bd15ba2dc43ca3782/src/niri.rs#L1265-L1300)).

Shape: in `tannoy-board`, not `bar-anchor` (it needs GTK). On show, when not
under Hyprland, map one transparent probe window per GDK monitor (overlay
layer, all four anchors, exclusive zone `-1`, keyboard mode none, full input
region), take the first `GtkEventControllerMotion::enter` (x, y) plus its
monitor, destroy the probes, and fill `Anchor.monitor`, `x`, `y` from it.
The edge is the nearest edge to the point (cross-checked with Waybar's
`position`). Time out after about 100 ms and keep the IPC anchor.

Limits:

- Read from source, not tested: that GTK4 on Wayland reports the
  first `enter` on a freshly mapped window without motion (unconfirmed), and
  the exact frame timing in sway and niri (unconfirmed).
- No `enter` while the cursor is hidden (sway `hide_cursor`, niri's hidden
  pointer state), or if the user moved off the monitor; the timeout covers it.
- For a frame or two the probe holds pointer focus over the bar, so Waybar
  sees a leave and its hover style may flicker; a second click landing
  during the probe goes to the probe.
- Probing every monitor removes the dependency on the compositor's focused
  output (which matters under sway with `focus_follows_mouse no`), and it
  would also work on other layer-shell compositors (for example river or
  Wayfire; untested).
- No wire change: the board can decide to probe from the environment (no
  `HYPRLAND_INSTANCE_SIGNATURE`), so the `Anchor` the CLI sends stays the
  same shape.

## Recommended implementation sketch for bar-anchor

Keep `Anchor` exactly as it is. Add two modules that mirror `hypr.rs` (raw
socket, own serde structs, pure `compute_anchor` tested against recorded
replies) and one dispatcher. All three return monitor-local logical
coordinates, so `physical` stays `false`.

```rust
pub mod niri;
pub mod sway;

/// What the caller knows about the bar from outside the compositor
/// (Tannoy reads it from Waybar's config).
#[derive(Debug, Clone, Copy, Default)]
pub struct BarHint {
    pub edge: Option<Edge>,
    /// Bar thickness in logical pixels, if known.
    pub size: Option<f64>,
}

/// Hyprland, then niri, then sway; None elsewhere.
pub fn anchor(namespace: &str, hint: BarHint, center: bool) -> Option<Anchor>;
```

`BarHint` and the dispatcher are new public API, not a wire change; Roundhouse
and Switchyard can adopt them when they copy the crate. `hypr::anchor` keeps
its signature.

### `sway`

- `socket()`: `$SWAYSOCK`, else `$I3SOCK`.
- `query(kind: u32) -> Result<String>`: write `b"i3-ipc"`, `0u32` (no
  payload), `kind` (native endian); read the 14-byte header, then exactly
  `len` bytes. Same 800 ms read timeout as `hypr::query`.
- Calls: `GET_OUTPUTS` (3) and `GET_WORKSPACES` (1).
- `compute_anchor(outputs_json, workspaces_json, hint, center)`:
  - `monitor`: the active output with `focused: true`; else the output named
    by the focused workspace; else the first active output.
  - `width`, `height`: that output's `rect.width` / `rect.height` as is (no
    scale division).
  - `reserved`: insets of the visible workspace's `rect` (the workspace whose
    name is the output's `current_workspace`) inside the output `rect`, minus
    the smallest inset as the gap estimate, clamped at 0. Order: left, top,
    right, bottom.
  - `edge`: `hint.edge`, else the side with the largest `reserved`, else
    `Bottom` (as `hypr.rs`).
  - `x`, `y`: the middle of the bar along its axis, inside the usable area:
    for top/bottom `x = reserved[0] + (width - reserved[0] - reserved[2]) / 2`
    and `y` the bar's middle (`reserved[1] / 2` or `height - reserved[3] / 2`);
    the mirror for left/right. `place` then docks the panel to the bar,
    centered.
  - `center`: passed through.

### `niri`

- `socket()`: `$NIRI_SOCKET`.
- `query(request: &str) -> Result<serde_json::Value>`: write the JSON request
  and `\n`, read one line, return the value under `Ok.<Variant>` or the `Err`
  string as an error.
- Calls: `"FocusedOutput"`; optionally `"Layers"` (ignore an error from
  pre-25.01 niri).
- `compute_anchor(focused_json, layers_json, namespace, hint, center)`:
  - `monitor`: `FocusedOutput.name`. If `Layers` shows the bar's namespace
    on other outputs but not this one, still keep the focused output (the
    click focused it), but treat that as a sign the namespace is wrong.
  - `width`, `height`: `logical.width` / `logical.height` as is.
  - `edge`: `hint.edge`, else `Top` (Waybar's default `position`).
  - `reserved`: `hint.size` on the bar's edge, 0 elsewhere. Only the
    perpendicular entries matter to `place`, and they are 0 unless there is a
    second bar, so this is safe.
  - `x`, `y`: the middle of the bar along its axis, as for sway.

### Tannoy side (outside bar-anchor, for completeness)

- `crates/tannoy/src/cli.rs:218` calls the dispatcher with a `BarHint` built
  from `tannoy_core::waybar::load_layout()`.
- `tannoy doctor` (`cli.rs:486`) reports which compositor IPC it found.
- Optional: the probe in `tannoy-board`, overriding `x`, `y` (and `monitor`)
  when it answers in time.
- `docs/design.md` "Anchoring" and the README need the new behavior per
  AGENTS.md.

## Open questions

1. Does GTK4 deliver the first `enter` (with coordinates) on a freshly
   mapped layer-shell window under a still pointer, on both sway and niri?
   Needs a quick test on each with a dev build; this decides whether the
   probe is worth building.
2. Is a one-frame Waybar hover flicker during the probe acceptable, or
   should the probe only run when the IPC anchor is uncertain?
3. Sway gaps: is the min-inset heuristic good enough, or should the sway
   path ignore `reserved` beyond the bar's edge (it only matters with a
   second bar on a perpendicular edge)?
4. Should Roundhouse and Switchyard take `BarHint` and the dispatcher at the
   same time, so the three copies of `bar-anchor` stay in step?
5. Waybar could pass `WAYBAR_OUTPUT_NAME` to `on-click` as it already does
   for `exec` (a small upstream change in `AModule`). That would give the
   monitor on every compositor without IPC. Not proposed upstream; whether
   it would be accepted is unconfirmed.
