#!/usr/bin/env python3
"""Writes docs/assets/hero.svg: the README's animated hero.

A 14 second loop on the same desktop as Roundhouse's, Switchyard's and
Timetable's heroes, under the floating Platform bar: the cover in the media
pill is clicked and the drawer springs out of it, a podcast plays in Chrome,
skips ahead 15 seconds and pauses, Spotify is picked and played, do not
disturb hides the title everywhere and gives it back, and a click outside
closes the drawer.

The drawer is drawn by `draw_drawer()` in its own coordinates at its real
size (400 px wide, as `drawer.width` defaults), one state at a time, so the
desktop tour in dot-files can port it as it ports Roundhouse's drawer. The
story stacks one drawer per state and swaps them as the player changes.

Pure CSS animation inside the SVG, so GitHub plays it in an <img>. With
reduced motion it is a still of the open drawer playing the podcast.

    python3 docs/assets/hero.py
"""

import math
import textwrap
from dataclasses import dataclass, replace
from pathlib import Path

W, H = 1200, 660
T = 14  # loop seconds

# Nord, matching the Platform waybar and the siblings' heroes.
BG0, BG1 = "#14171c", "#0d0f12"
PILL, PILL_LINE, PILL_ON = "#24262a", "#3a404d", "#3c4651"
BAR_FG, BRIGHT, BAR_DIM = "#c5c8c6", "#d8dee9", "#616e88"
PANEL, PANEL_LINE, WELL = "#1e2024", "#41464e", "#15171a"
TILE, TILE_LINE = "#25282c", "#2f3237"
FG, DIM, IDLE = "#c5c8c6", "#7c838e", "#555b64"
FROST, GO, AMBER, PURPLE = "#88c0d0", "#a3be8c", "#ebcb8b", "#b48ead"
FONT = "JetBrains Mono, DejaVu Sans Mono, Menlo, Consolas, monospace"

# The README's alt text says the same.
ALT = (
    "A floating waybar of frosted pills: workspaces with a glyph per open window, the window title, "
    "Tannoy's media pill with a small podcast cover and Slow Signals · Where the Night Trains…, cut at a word "
    "with a real ellipsis, then the lamps, audio, the desk pill with Logbook's mic and a drawer handle, the clock "
    "and the tray. A click on the cover opens the Tannoy drawer out of it: the podcast playing in Chrome with its "
    "scrub bar moving, previous and next, play and pause, and 15 second skips. A click on the forward skip jumps "
    "ahead and a click on pause stops it, dimming the bar pill. A click on the Spotify chip picks Spotify: the cover, "
    "the title and the bar pill swap to a song, the skips give way to shuffle and repeat toggles, an Auto chip "
    "appears, and play starts it. Do not disturb turns on: the bell turns amber, the bar keeps only a dim music "
    "glyph, and the drawer shrinks to say the title is hidden while its controls stay. Do not disturb turns off "
    "again, and a click outside closes the drawer."
)


def esc(s):
    return s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def tx(x, y, s, size=13, fill=FG, weight=400, anchor="start", spacing=0, attrs=""):
    """A <text> element as a string."""
    ls = f' letter-spacing="{spacing}"' if spacing else ""
    attrs = f" {attrs}" if attrs else ""
    return f'<text x="{x:.1f}" y="{y:.1f}" font-size="{size}" fill="{fill}" font-weight="{weight}" text-anchor="{anchor}"{ls}{attrs}>{esc(s)}</text>'


def text_w(s, size):
    """A monospace advance: 0.6em a character."""
    return len(s) * size * 0.6


def ellipsize(text, limit=40):
    """tannoy_core::text::ellipsize: cut at a word, end on a real ellipsis."""
    if len(text) <= limit:
        return text
    keep = text[:limit - 1]
    cut = keep if text[limit - 1].isspace() else keep[:keep.rfind(" ")]
    while cut != (trimmed := cut.rstrip().rstrip(",;:-·|/([&").rstrip()):
        cut = trimmed
    return cut + "…"


def clock(sec):
    return f"{int(sec) // 60}:{int(sec) % 60:02d}"


# ---------------------------------------------------------------- glyphs
# Drawn, not typed: the Nerd Font glyphs would not survive an <img>. Each
# returns its SVG.

def g_play(cx, cy, c):
    return f'<path d="M{cx - 4.5},{cy - 6.5} L{cx - 4.5},{cy + 6.5} L{cx + 6},{cy} Z" fill="{c}" stroke="{c}" stroke-width="1.5" stroke-linejoin="round"/>'


def g_pause(cx, cy, c):
    return "".join(f'<rect x="{cx + dx}" y="{cy - 6.5}" width="3.4" height="13" rx="1" fill="{c}"/>' for dx in (-4.5, 1.5))


def g_track(cx, cy, c, fwd):
    s = 1 if fwd else -1
    return (f'<path d="M{cx - 4.5 * s},{cy - 5.5} L{cx - 4.5 * s},{cy + 5.5} L{cx + 3 * s},{cy} Z" fill="{c}" stroke="{c}" stroke-width="1.2" stroke-linejoin="round"/>'
            f'<rect x="{cx + (3.5 if fwd else -5.5)}" y="{cy - 5.5}" width="2" height="11" rx=".8" fill="{c}"/>')


def g_skip(cx, cy, c, fwd):
    """A circular arrow around the number of seconds it jumps."""
    r = 9.5
    a0, a1 = (-100, 150) if fwd else (-80, 30)  # degrees; the gap is at the top
    p0 = (cx + r * math.cos(math.radians(a0)), cy + r * math.sin(math.radians(a0)))
    p1 = (cx + r * math.cos(math.radians(a1)), cy + r * math.sin(math.radians(a1)))
    sweep = 0 if fwd else 1
    s = 1 if fwd else -1
    x, y = p0
    return (f'<path d="M{p1[0]:.1f},{p1[1]:.1f} A{r},{r} 0 1 {sweep} {p0[0]:.1f},{p0[1]:.1f}" fill="none" stroke="{c}" stroke-width="1.7" stroke-linecap="round"/>'
            f'<path d="M{x + 4.2 * s:.1f},{y:.1f} L{x - 0.6 * s:.1f},{y - 3.6:.1f} L{x - 0.6 * s:.1f},{y + 3.6:.1f} Z" fill="{c}"/>'
            + tx(cx, cy + 3.2, "15", 8.5, c, 700, "middle"))


def g_shuffle(cx, cy, c):
    """Two crossing arrows."""
    out = ""
    for y0, y1 in ((-4.5, 4.5), (4.5, -4.5)):
        out += (f'<path d="M{cx - 7},{cy + y0} C{cx - 1},{cy + y0} {cx + 1},{cy + y1} {cx + 6},{cy + y1}" fill="none" stroke="{c}" stroke-width="1.6" stroke-linecap="round"/>'
                f'<path d="M{cx + 8.5},{cy + y1} L{cx + 5},{cy + y1 - 2.6} L{cx + 5},{cy + y1 + 2.6} Z" fill="{c}"/>')
    return out


def g_repeat(cx, cy, c):
    """A loop of two arrows."""
    return (f'<path d="M{cx - 7},{cy + 1} L{cx - 7},{cy - 4} L{cx + 5},{cy - 4}" fill="none" stroke="{c}" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/>'
            f'<path d="M{cx + 8},{cy - 4} L{cx + 4.5},{cy - 6.6} L{cx + 4.5},{cy - 1.4} Z" fill="{c}"/>'
            f'<path d="M{cx + 7},{cy - 1} L{cx + 7},{cy + 4} L{cx - 5},{cy + 4}" fill="none" stroke="{c}" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/>'
            f'<path d="M{cx - 8},{cy + 4} L{cx - 4.5},{cy + 1.4} L{cx - 4.5},{cy + 6.6} Z" fill="{c}"/>')


def g_speaker(cx, cy, c):
    return (f'<path d="M{cx - 7},{cy - 2.5} L{cx - 4},{cy - 2.5} L{cx},{cy - 6} L{cx},{cy + 6} L{cx - 4},{cy + 2.5} L{cx - 7},{cy + 2.5} Z" fill="{c}"/>'
            f'<path d="M{cx + 3},{cy - 3} Q{cx + 5.5},{cy} {cx + 3},{cy + 3} M{cx + 5.5},{cy - 5.5} Q{cx + 10},{cy} {cx + 5.5},{cy + 5.5}" fill="none" stroke="{c}" stroke-width="1.5" stroke-linecap="round"/>')


def g_bell(cx, cy, c, off=False):
    out = (f'<path d="M{cx - 5.5},{cy + 3.5} L{cx - 5.5},{cy - 1} Q{cx - 5.5},{cy - 6.5} {cx},{cy - 6.5} Q{cx + 5.5},{cy - 6.5} {cx + 5.5},{cy - 1} L{cx + 5.5},{cy + 3.5} L{cx + 7},{cy + 5} L{cx - 7},{cy + 5} Z" fill="{c}"/>'
           f'<circle cx="{cx}" cy="{cy + 7}" r="1.8" fill="{c}"/>')
    if off:
        out += (f'<line x1="{cx - 7.5}" y1="{cy - 7.5}" x2="{cx + 7.5}" y2="{cy + 7.5}" stroke="{PILL}" stroke-width="3.6"/>'
                f'<line x1="{cx - 7.5}" y1="{cy - 7.5}" x2="{cx + 7.5}" y2="{cy + 7.5}" stroke="{c}" stroke-width="1.7" stroke-linecap="round"/>')
    return out


def g_note(cx, cy, c, s=1.0):
    """md-music: two beamed notes, what do not disturb leaves in the bar."""
    return (f'<g transform="translate({cx},{cy}) scale({s})" fill="{c}">'
            '<ellipse cx="-4.2" cy="5" rx="2.9" ry="2.3"/><ellipse cx="4.6" cy="3.2" rx="2.9" ry="2.3"/>'
            '<rect x="-1.9" y="-6.5" width="1.6" height="11.5"/><rect x="6.9" y="-8.3" width="1.6" height="11.5"/>'
            '<path d="M-1.9,-6.5 L8.5,-8.3 L8.5,-5.6 L-1.9,-3.8 Z"/></g>')


# ---------------------------------------------------------------- covers
# Made up, and so are the tracks.

def cover_pod():
    """Slow Signals: a lamp on a night line, its signal rippling out."""
    return ('<rect width="100" height="100" fill="url(#podsky)"/>'
            '<circle cx="74" cy="26" r="13" fill="#ebcb8b" opacity=".9"/><circle cx="79" cy="22" r="12" fill="#22324a"/>'
            + "".join(f'<path d="M{28 - r},{66} A{r},{r} 0 0 1 {28 + r},{66}" fill="none" stroke="#88c0d0" stroke-width="2.6" opacity="{o}"/>'
                      for r, o in [(10, .9), (19, .6), (28, .35), (37, .18)])
            + '<circle cx="28" cy="66" r="4.5" fill="#a3be8c"/>'
            '<rect y="78" width="100" height="22" fill="#121a24"/>'
            '<path d="M0,84 H100 M0,92 H100" stroke="#4c566a" stroke-width="1.6"/>'
            + "".join(f'<rect x="{x}" y="82" width="3" height="12" fill="#3b4252"/>' for x in range(4, 100, 12)))


def cover_song():
    """Paper Lanterns: three lanterns over a dusk lake."""
    lanterns = ""
    for x, y, s in [(26, 40, 1.0), (54, 28, 1.25), (78, 46, 0.85)]:
        lanterns += (f'<line x1="{x}" y1="0" x2="{x}" y2="{y - 9 * s:.1f}" stroke="#2e2235" stroke-width="1.2"/>'
                     f'<circle cx="{x}" cy="{y}" r="{18 * s:.1f}" fill="url(#glow)"/>'
                     f'<rect x="{x - 6 * s:.1f}" y="{y - 9 * s:.1f}" width="{12 * s:.1f}" height="{18 * s:.1f}" rx="{5 * s:.1f}" fill="#ebcb8b"/>'
                     f'<rect x="{x - 6 * s:.1f}" y="{y - 1 * s:.1f}" width="{12 * s:.1f}" height="{2 * s:.1f}" fill="#d08770" opacity=".7"/>')
    return ('<rect width="100" height="100" fill="url(#dusk)"/>' + lanterns
            + '<path d="M0,74 Q22,64 44,72 T100,68 V100 H0 Z" fill="#2e2235"/>'
            '<rect y="80" width="100" height="20" fill="#3b4252" opacity=".55"/>'
            '<path d="M18,86 h16 M48,90 h12 M70,85 h18" stroke="#ebcb8b" stroke-width="1.4" opacity=".55"/>')


def cover_defs():
    """The <defs> the covers need: `<use href="#pod">` and `#song`."""
    return ('<linearGradient id="podsky" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#22324a"/><stop offset="1" stop-color="#3b4f6b"/></linearGradient>'
            '<linearGradient id="dusk" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#3b2b4a"/><stop offset=".7" stop-color="#b8706a"/><stop offset="1" stop-color="#d08770"/></linearGradient>'
            '<radialGradient id="glow"><stop offset="0" stop-color="#ebcb8b" stop-opacity=".55"/><stop offset="1" stop-color="#ebcb8b" stop-opacity="0"/></radialGradient>'
            '<clipPath id="art"><rect width="100" height="100" rx="7"/></clipPath>'
            f'<symbol id="pod" viewBox="0 0 100 100"><g clip-path="url(#art)">{cover_pod()}</g></symbol>'
            f'<symbol id="song" viewBox="0 0 100 100"><g clip-path="url(#art)">{cover_song()}</g></symbol>')


# ---------------------------------------------------------------- the drawer
# crates/tannoy-board: drawer.rs builds it, style.css sizes it. Everything
# below is in the drawer's own coordinates, (0, 0) at the panel's top left.

@dataclass(frozen=True)
class Track:
    player: str  # the MPRIS player's name, as the drawer shows it
    title: str
    artist: str
    album: str
    at: float  # seconds into the track
    length: float
    cover: str  # a symbol from cover_defs()
    podcast: bool  # long, no album: the skips show (drawer.skips = "auto")
    shuffle: bool | None = None  # None while the player has no Shuffle
    loop: str | None = None  # LoopStatus: None, "None", "Track", "Playlist"


POD = Track("Chrome", "Where the Night Trains Go, with Iris Calloway", "Slow Signals", "", 552, 1600, "pod", True)
SONG = Track("Spotify", "Paper Lanterns", "Marlow Fen", "Low Tide Radio", 107, 232, "song", False, False, "None")


@dataclass(frozen=True)
class DrawerState:
    track: Track  # the active player's track
    status: str = "playing"  # playing or paused
    hidden: bool = False  # do not disturb (or a screen share) hides titles
    players: tuple = (("Chrome", "playing"), ("Spotify", "paused"))  # (name, status), the chips in order
    picked: bool = False  # a player was picked: the aside says so and Auto shows


DRAWER_W = 400  # drawer.width
P = 15  # the panel's border and padding
IW = DRAWER_W - 2 * P
GAP = 12  # between the panel's rows
HEADER_H, TRACK_H, CONTROLS_H, PLAYERS_H, OUTPUT_H, FOOT_H = 30, 152, 34, 41, 61, 13
ART = 112
HIDDEN_TEXT = "The title and cover stay off the bar while do not disturb is on or the screen is shared. The controls still work."
HIDDEN_LINES = textwrap.wrap(HIDDEN_TEXT, 44)
HIDDEN_H = 1 + 9 + 13 + 6 + 16 * len(HIDDEN_LINES) + 9 + 1


def drawer_height(s):
    now = HIDDEN_H if s.hidden else TRACK_H
    return P + HEADER_H + now + CONTROLS_H + PLAYERS_H + OUTPUT_H + FOOT_H + 5 * GAP + P


def state_color(s):
    return AMBER if s.hidden else GO if s.status == "playing" else FROST


def draw_drawer(s, scrub=None):
    """One state of the drawer. Returns (svg, height, spots): spots name
    what a story can click, as (x, y, w, h) in drawer coordinates: cover,
    scrub, shuffle, back, prev, play, next, fwd, repeat (those that show),
    chip:<player>, chip:Auto (once picked) and change.

    `scrub(x0, cy, w, times_y)`, when given, draws the scrub bar's fill, its
    knob and the position clock (to animate them); otherwise they sit still
    at the track's position."""
    t = s.track
    o, spots = [], {}
    h = drawer_height(s)
    o.append(f'<rect width="{DRAWER_W}" height="{h}" rx="16" fill="{PANEL}" stroke="{PANEL_LINE}" filter="url(#shadow)"/>')

    # Header: the lamp, TANNOY, the active player and a state chip.
    y = P
    color = state_color(s)
    label = "hidden" if s.hidden else s.status
    o.append(f'<circle cx="{P + 4}" cy="{y + 15}" r="4.5" fill="{color}"/>')
    o.append(tx(P + 18, y + 12, "TANNOY", 12, FG, 700, spacing=2))
    o.append(tx(P + 18, y + 27, f"{t.player} · {len(s.players)} players", 11, DIM))
    w = text_w(label, 11) + 18
    o.append(f'<rect x="{P + IW - w:.1f}" y="{y + 5.5}" width="{w:.1f}" height="19" rx="9.5" fill="{color}" fill-opacity=".15"/>')
    o.append(tx(P + IW - w / 2, y + 19, label, 11, color, 700, "middle"))
    y += HEADER_H + GAP

    if s.hidden:
        # The same place, a shorter card.
        o.append(f'<rect x="{P}" y="{y}" width="{IW}" height="{HIDDEN_H}" rx="10" fill="{AMBER}" fill-opacity=".07" stroke="{AMBER}" stroke-opacity=".3"/>')
        o.append(tx(P + 11, y + 20, "HIDDEN", 10, AMBER, 700, spacing=1))
        o.append(tx(P + IW - 11, y + 20, "DO NOT DISTURB IS ON", 10, DIM, 700, "end", 1))
        for i, line in enumerate(HIDDEN_LINES):
            o.append(tx(P + 11, y + 42 + i * 16, line, 13, FG))
        y += HIDDEN_H + GAP
    else:
        # The cover (a click brings the player forward), then the track.
        o.append(f'<use href="#{t.cover}" x="{P}" y="{y}" width="{ART}" height="{ART}"/>')
        spots["cover"] = (P, y, ART, ART)
        kind = f"PODCAST · {t.player.upper()}" if t.podcast else t.player.upper()
        lines = [(kind, 10, 700, DIM, 13, 1)]
        lines += [(ln, 15, 700, FG, 19, 0) for ln in textwrap.wrap(t.title, int((IW - ART - 12) // 9))[:3]]
        lines += [(t.artist, 13, 400, FG, 16, 0)] if t.artist else []
        lines += [(t.album, 11.5, 400, DIM, 14, 0)] if t.album else []
        block = sum(ln[4] for ln in lines) + 3 * (len(lines) - 1)
        ly = y + (ART - block) / 2
        for s_, size, weight, fill, lh, sp in lines:
            o.append(tx(P + ART + 12, ly + lh * 0.78, s_, size, fill, weight, spacing=sp))
            ly += lh + 3
        cy, times_y = y + ART + 4 + 9, y + ART + 4 + 18 + 4 + 11
        o.append(f'<rect x="{P}" y="{cy - 2}" width="{IW}" height="4" rx="2" fill="{TILE_LINE}"/>')
        spots["scrub"] = (P, cy - 9, IW, 18)
        if scrub:
            o.append(scrub(P, cy, IW, times_y))
        else:
            f = t.at / t.length
            o.append(f'<rect x="{P}" y="{cy - 2}" width="{IW * f:.1f}" height="4" rx="2" fill="{FROST}"/>')
            o.append(f'<circle cx="{P + IW * f:.1f}" cy="{cy}" r="6" fill="{FG}" stroke="#0b0c0e" stroke-opacity=".5"/>')
            o.append(tx(P, times_y, clock(t.at), 11, DIM))
        o.append(tx(P + IW, times_y, clock(t.length), 11, DIM, anchor="end"))
        y += TRACK_H + GAP

    # Controls: shuffle and repeat at the ends for players that have them,
    # the skips inside them while a podcast is up.
    row = [("shuffle", 44, t.shuffle is not None), ("back", 44, t.podcast), ("prev", 44, True), ("play", 56, True),
           ("next", 44, True), ("fwd", 44, t.podcast), ("repeat", 44, t.loop is not None)]
    row = [(n, w) for n, w, shown in row if shown]
    bx = P + (IW - sum(w for _, w in row) - 8 * (len(row) - 1)) / 2
    for name, w in row:
        cx, cy = bx + w / 2, y + CONTROLS_H / 2
        spots[name] = (bx, y, w, CONTROLS_H)
        if name == "play":
            o.append(f'<rect x="{bx:.1f}" y="{y}" width="{w}" height="{CONTROLS_H}" rx="10" fill="{FROST}"/>')
            o.append(g_pause(cx, cy, WELL) if s.status == "playing" else g_play(cx + 1, cy, WELL))
        elif name in ("shuffle", "repeat"):
            # Dim while off, the accent while on, and no tile behind them.
            on = t.shuffle if name == "shuffle" else t.loop in ("Track", "Playlist")
            o.append(f'<rect x="{bx:.1f}" y="{y + .5}" width="{w}" height="{CONTROLS_H - 1}" rx="10" fill="none" stroke="{TILE_LINE}"/>')
            glyph = g_shuffle if name == "shuffle" else g_repeat
            o.append(glyph(cx, cy, FROST if on else DIM))
        else:
            o.append(f'<rect x="{bx:.1f}" y="{y + .5}" width="{w}" height="{CONTROLS_H - 1}" rx="10" fill="{TILE}" stroke="{TILE_LINE}"/>')
            if name in ("back", "fwd"):
                o.append(g_skip(cx, cy, FG, name == "fwd"))
            else:
                o.append(g_track(cx, cy, FG, name == "next"))
        bx += w + 8
    y += CONTROLS_H + GAP

    # Players: a chip each, the active one in the accent; a pick adds Auto.
    o.append(tx(P, y + 10, "PLAYERS", 10, DIM, 700, spacing=1))
    aside = "PICKED · MEDIA KEYS FOLLOW IT" if s.picked else "MEDIA KEYS FOLLOW THE LATEST"
    o.append(tx(P + IW, y + 10, aside, 10, DIM, 700, "end", 1))
    cx, cy = P, y + 19
    chips = [(n, st) for n, st in s.players] + ([("Auto", None)] if s.picked else [])
    for name, status in chips:
        on = name == t.player
        w = 22 + (10 if status else 0) + text_w(name, 11.5)
        o.append(f'<rect x="{cx:.1f}" y="{cy + .5}" width="{w:.1f}" height="21" rx="10.5" fill="none" stroke="{FROST if on else PANEL_LINE}"/>')
        if status:
            o.append(f'<circle cx="{cx + 14:.1f}" cy="{cy + 11}" r="3" fill="{GO if status == "playing" else DIM if status == "paused" else IDLE}"/>')
        o.append(tx(cx + (24 if status else 11), cy + 15, name, 11.5, FROST if on else DIM))
        spots[f"chip:{name}"] = (cx, cy, w, 22)
        cx += w + 6
    y += PLAYERS_H + GAP

    # Output: the default sink and the switcher.
    o.append(f'<rect x="{P}" y="{y}" width="{IW}" height="{OUTPUT_H}" rx="10" fill="{TILE}" stroke="{TILE_LINE}"/>')
    o.append(tx(P + 11, y + 20, "OUTPUT", 10, DIM, 700, spacing=1))
    o.append(g_speaker(P + 19, y + 40, FROST))
    o.append(tx(P + 36, y + 44.5, "Desk speakers · 35%", 13, FG))
    w = text_w("Change", 11.5) + 22
    bx = P + IW - 11 - w
    o.append(f'<rect x="{bx:.1f}" y="{y + 30}" width="{w:.1f}" height="21" rx="8" fill="none" stroke="{PANEL_LINE}"/>')
    o.append(tx(bx + w / 2, y + 44.5, "Change", 11.5, FG, anchor="middle"))
    spots["change"] = (bx, y + 30, w, 21)
    y += OUTPUT_H + GAP

    o.append(tx(P, y + 10, "Space play/pause · ←/→ seek · Esc close", 10.5, DIM))
    return "".join(o), h, spots


def center(spot):
    x, y, w, h = spot
    return x + w / 2, y + h / 2


# ---------------------------------------------------------------- animation

# The story, in seconds.
CLICK_BAR, OPEN = 1.0, 1.08
CLICK_SKIP = 2.9
CLICK_PAUSE = 4.0
CLICK_CHIP = 5.1
CLICK_PLAY = 6.3
DND_ON, DND_OFF = 7.9, 9.9
CLICK_OUT, CLOSED = 11.5, 11.95
RESET = 13.0
LAG = 0.1  # a click reaches the player and comes back as a snapshot

out, kfs = [], []


def add(s):
    out.append(s)


def pct(sec):
    return f"{max(0.0, min(100.0, sec / T * 100)):.2f}%"


def frames(pts):
    """A keyframe set from (seconds, declarations) points; returns its name."""
    name = f"k{len(kfs)}"
    rows = " ".join(f"{pct(t)} {{ {d}; }}" for t, d in pts)
    kfs.append(f"@keyframes {name} {{ {rows} }}")
    return name


def anim(prop, pts):
    return frames([(t, f"{prop}: {v}") for t, v in pts])


def fade(spans, ramp=0.16, out_ramp=None):
    """Opacity keyframes visible during each (start, end) span."""
    out_ramp = ramp if out_ramp is None else out_ramp
    pts = []
    for s, e in spans:
        if s <= 0:
            pts.append((0, 1))
        else:
            pts += [(0, 0)] if not pts else []
            pts += [(s, 0), (s + ramp, 1)]
        if e >= T:
            pts.append((T, 1))
        else:
            pts += [(e, 1), (e + out_ramp, 0)]
    if pts[-1][0] < T:
        pts.append((T, 0))
    return anim("opacity", pts)


def style(*names, extra=""):
    return f'class="a" style="animation-name:{",".join(names)};{extra}"'


def group(name, shown=True, extra=""):
    """A group that fades by `name`; `shown` is its state in the still."""
    add(f'<g {style(name, extra=extra)} opacity="{1 if shown else 0}">')


def press(t):
    """A button's flash under a click."""
    return anim("opacity", [(0, 0), (t - 0.01, 0), (t, 1), (t + 0.3, 0), (T, 0)])


def ticks(x, y, steps, still=None):
    """A clock label: (from, to, text) steps that swap without a fade."""
    o = []
    for s, e, label in steps:
        o.append(f'<g {style(fade([(s, e)], 0.01))} opacity="{1 if label == still else 0}">{tx(x, y, label, 11, DIM)}</g>')
    return "".join(o)


def moving_scrub(track, pos, times, extra=None):
    """A scrub hook for draw_drawer(): the fill and knob follow pos(t) at
    each of `times`, and the clock ticks over at each second."""
    def draw(x0, cy, w, times_y):
        f0 = track.at / track.length
        moves = [(t, pos(t) / track.length) for t in times]
        fill = anim("transform", [(t, f"scaleX({f / f0:.4f})") for t, f in moves])
        knob = anim("transform", [(t, f"translateX({(f - f0) * w:.2f}px)") for t, f in moves])
        edges = sorted({0.0, *[t for t in times if 0 < t < T]})
        steps = []
        for i, s in enumerate(edges):
            e = edges[i + 1] if i + 1 < len(edges) else T
            label = clock(pos(s + 0.001))
            if steps and steps[-1][2] == label:
                steps[-1] = (steps[-1][0], e, label)
            else:
                steps.append((s, e, label))
        o = (f'<rect x="{x0}" y="{cy - 2}" width="{w * f0:.1f}" height="4" rx="2" fill="{FROST}" {style(fill, extra="transform-box:fill-box;transform-origin:left center")}/>'
             f'<circle cx="{x0 + w * f0:.1f}" cy="{cy}" r="6" fill="{FG}" stroke="#0b0c0e" stroke-opacity=".5" {style(knob)}/>'
             + ticks(x0, times_y, steps, clock(track.at)))
        return o + (extra(x0 + w * f0, cy) if extra else "")
    return draw


# ---------------------------------------------------------------- scene

css = f"""
text {{ white-space: pre; }}
.a {{ animation-duration: {T}s; animation-iteration-count: infinite; animation-timing-function: linear; animation-fill-mode: both; }}
@media (prefers-reduced-motion: reduce) {{
  .a {{ animation: none !important; }}
  #cursor, .ripple {{ display: none; }}
}}
"""

add(f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" width="{W}" height="{H}" font-family="{FONT}" xml:space="preserve" role="img" aria-label="{esc(ALT)}">')
STYLE_AT = len(out)
add("")  # the <style> goes here once every keyframe exists
add(f"""<defs>
<linearGradient id="wall" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{BG0}"/><stop offset="1" stop-color="{BG1}"/></linearGradient>
<radialGradient id="haze1" cx=".85" cy="0" r=".75"><stop offset="0" stop-color="{FROST}" stop-opacity=".13"/><stop offset="1" stop-color="{FROST}" stop-opacity="0"/></radialGradient>
<radialGradient id="haze2" cx=".05" cy="1" r=".6"><stop offset="0" stop-color="{PURPLE}" stop-opacity=".09"/><stop offset="1" stop-color="{PURPLE}" stop-opacity="0"/></radialGradient>
{cover_defs()}
<filter id="shadow" x="-20%" y="-20%" width="140%" height="150%"><feDropShadow dx="0" dy="18" stdDeviation="22" flood-color="#000" flood-opacity=".65"/></filter>
<clipPath id="screen"><rect width="{W}" height="{H}" rx="18"/></clipPath>
</defs>""")
add('<g clip-path="url(#screen)">')
add(f'<rect width="{W}" height="{H}" fill="url(#wall)"/><rect width="{W}" height="{H}" fill="url(#haze1)"/><rect width="{W}" height="{H}" fill="url(#haze2)"/>')


def window(x, y, w, h):
    add(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="12" fill="#1a1d22" stroke="#2a2e35"/>')
    add(f'<rect x="{x}" y="{y}" width="{w}" height="30" rx="12" fill="#1f2328"/>')
    for k, c in enumerate(["#bf616a", AMBER, GO]):
        add(f'<circle cx="{x + 18 + k * 16}" cy="{y + 15}" r="5" fill="{c}" fill-opacity=".55"/>')


# Notes in nvim, behind where the drawer opens.
window(36, 36, 470, 330)
for i, (indent, w, c) in enumerate([(0, 220, "#4c566a"), (0, 0, ""), (0, 300, "#3b4252"), (0, 340, "#3b4252"), (0, 180, "#3b4252"),
                                    (0, 0, ""), (0, 140, "#5e81ac55"), (24, 260, "#434c5e"), (24, 210, "#434c5e"), (24, 290, "#434c5e")]):
    if w:
        add(f'<rect x="{58 + indent}" y="{86 + i * 24}" width="{w}" height="9" rx="4" fill="{c}"/>')

# The browser tab the podcast plays in.
BX, BY, BW, BH = 712, 40, 452, 524
window(BX, BY, BW, BH)
add(f'<rect x="{BX + 70}" y="{BY + 7}" width="190" height="23" rx="7" fill="#262a30"/>')
add(f'<use href="#pod" x="{BX + 80}" y="{BY + 12}" width="13" height="13"/>')
add(f'<rect x="{BX + 100}" y="{BY + 15}" width="120" height="7" rx="3" fill="#4c566a"/>')
add(f'<rect x="{BX + 16}" y="{BY + 42}" width="{BW - 32}" height="22" rx="11" fill="#15171a"/>')
add(f'<rect x="{BX + 32}" y="{BY + 50}" width="170" height="6" rx="3" fill="#3b4252"/>')
add(f'<use href="#pod" x="{BX + 28}" y="{BY + 86}" width="150" height="150"/>')
for i, (w, c) in enumerate([(150, "#4c566a"), (210, "#d8dee955"), (190, "#d8dee955"), (120, "#3b4252"), (0, ""), (220, "#3b4252"), (180, "#3b4252")]):
    if w:
        add(f'<rect x="{BX + 196}" y="{BY + 96 + i * 20}" width="{w}" height="{10 if i in (1, 2) else 7}" rx="3" fill="{c}"/>')
add(f'<rect x="{BX + 28}" y="{BY + 262}" width="{BW - 56}" height="44" rx="22" fill="#262a30"/>')
add(f'<circle cx="{BX + 52}" cy="{BY + 284}" r="12" fill="#3b4252"/>')
# The tab's own button follows the podcast: paused from the drawer, then
# playing again when the loop comes round.
POD_PLAYS = [(0, CLICK_PAUSE + LAG), (RESET, T)]
group(fade(POD_PLAYS, 0.02))
add(g_pause(BX + 52.5, BY + 284, BAR_FG))
add("</g>")
group(fade([(CLICK_PAUSE + LAG, RESET)], 0.02), False)
add(g_play(BX + 53, BY + 284, BAR_FG))
add("</g>")
add(f'<rect x="{BX + 76}" y="{BY + 282}" width="{BW - 170}" height="4" rx="2" fill="#3b4252"/>')
add(f'<rect x="{BX + 76}" y="{BY + 282}" width="{(BW - 170) * POD.at / POD.length:.0f}" height="4" rx="2" fill="#7c838e"/>')
for i, w in enumerate([340, 300, 360, 220, 310, 260, 120]):
    add(f'<rect x="{BX + 28}" y="{BY + 336 + i * 24}" width="{w}" height="8" rx="4" fill="#2e3440"/>')

# ---------------------------------------------------------------- the bar
# packages/waybar in dot-files: a floating bar 34 px high, 6 px off the
# edges, 6 px between pills. Modules pad 8 px a side inside a group's 4.

BAR_Y, BAR_H = 620, 34
MID = BAR_Y + 21.5  # text baseline
CY = BAR_Y + BAR_H / 2
BAR_SIZE, CW = 13, 7.8  # bar font size and its advance
GW = 11  # a Nerd Font glyph's width in the bar


def pill(x, w, fill=PILL):
    add(f'<rect x="{x:.1f}" y="{BAR_Y + 0.5}" width="{w:.1f}" height="{BAR_H - 1}" rx="10" fill="{fill}" stroke="{PILL_LINE}"/>')


def bar_glyph(kind, cx, c):
    """The window glyphs waybar rewrites titles and workspaces to."""
    y = CY
    if kind == "term":  # nf-fa-terminal
        return f'<path d="M{cx - 5.5},{y - 4.5} l4,4 l-4,4 M{cx},{y + 4.5} h6" fill="none" stroke="{c}" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"/>'
    if kind == "chrome":  # nf-fa-chrome
        return (f'<circle cx="{cx}" cy="{y}" r="6.2" fill="none" stroke="{c}" stroke-width="1.6"/><circle cx="{cx}" cy="{y}" r="2.3" fill="{c}"/>'
                f'<path d="M{cx},{y - 2.3} h5.6 M{cx + 2},{y + 1.2} l-2.8,4.8 M{cx - 2},{y + 1.2} l-2.8,-4.8" stroke="{c}" stroke-width="1.4"/>')
    if kind == "slack":  # nf-fa-slack
        return f'<path d="M{cx - 2.2},{y - 6} v12 M{cx + 2.2},{y - 6} v12 M{cx - 6},{y - 2.2} h12 M{cx - 6},{y + 2.2} h12" stroke="{c}" stroke-width="1.8" stroke-linecap="round"/>'
    if kind == "vim":  # nf-custom-vim
        return (f'<path d="M{cx},{y - 7} L{cx + 7},{y} L{cx},{y + 7} L{cx - 7},{y} Z" fill="none" stroke="{c}" stroke-width="1.4" stroke-linejoin="round"/>'
                f'<path d="M{cx - 3},{y - 2.5} L{cx},{y + 3} L{cx + 3},{y - 2.5}" fill="none" stroke="{c}" stroke-width="1.6" stroke-linejoin="round"/>')
    raise ValueError(kind)


# Left: workspaces (a dim number, then a glyph per open window) and the
# window title with its app's glyph in place of its suffix.
WORKSPACES = [(1, ["term", "chrome"], True), (2, ["slack"], False)]
x = 6
widths = [8 + CW + len(g) * (CW + GW) + 8 for _, g, _ in WORKSPACES]
ws_w = 4 + sum(w + 2 for w in widths) + 4
pill(x, ws_w)
bx = x + 4
for (num, glyphs, active), w in zip(WORKSPACES, widths):
    bx += 1
    if active:
        add(f'<rect x="{bx:.1f}" y="{BAR_Y + 4}" width="{w:.1f}" height="{BAR_H - 8}" rx="7" fill="{PILL_ON}"/>')
    c = BRIGHT if active else BAR_FG
    add(tx(bx + 8, MID, str(num), BAR_SIZE, c, attrs='fill-opacity=".55"'))
    gx = bx + 8 + CW
    for g in glyphs:
        add(bar_glyph(g, gx + CW + GW / 2, c))
        gx += CW + GW
    bx += w + 1
x += ws_w + 6
TITLE = "~/notes"
title_w = 12 + GW + 2 * CW + len(TITLE) * CW + 12
pill(x, title_w)
add(bar_glyph("vim", x + 12 + GW / 2, BRIGHT))
add(tx(x + 12 + GW + 2 * CW, MID, TITLE, BAR_SIZE, BRIGHT))
LEFT_END = x + title_w

# Right, packed from the edge: the tray, the clock, the desk, audio, the
# lamps, then the media pill.
x = W - 6
tray_w = 10 + 3 * 14 + 2 * 8 + 10
x -= tray_w
pill(x, tray_w)
for k, c in enumerate(["#81a1c1", PURPLE, "#5e81ac"]):
    add(f'<rect x="{x + 10 + k * 22}" y="{CY - 7}" width="14" height="14" rx="4" fill="{c}" fill-opacity=".8"/>')

# Timetable's clock, calm and purple with no event in view.
CLOCK = "Tue Nov 10  14:32"
clock_w = 12 + len(CLOCK) * CW + 12
x -= 6 + clock_w
pill(x, clock_w)
add(tx(x + 12, MID, CLOCK, BAR_SIZE, PURPLE))

# The desk: Logbook's mic lamp (dim, nothing listens), no lit key lights,
# and the collapsed drawer's chevron.
desk_w = 4 + (16 + GW) + (16 + 8) + 4
x -= 6 + desk_w
pill(x, desk_w)
mx = x + 4 + 8 + GW / 2
add(f'<g fill="none" stroke="{BAR_DIM}" stroke-width="1.5" stroke-linecap="round"><rect x="{mx - 2.8}" y="{CY - 7.5}" width="5.6" height="9.5" rx="2.8"/><path d="M{mx - 5.5},{CY - 1} a5.5,5.5 0 0 0 11,0 M{mx},{CY + 4.5} v3"/></g>')
hx = x + 4 + 16 + GW + 8 + 4
add(f'<path d="M{hx + 2},{CY - 5} l-5,5 l5,5" fill="none" stroke="{BAR_DIM}" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"/>')

# Audio: the volume (frost) and the mouse battery (dim, information).
vol_w, mouse_w = 16 + GW + CW + 3 * CW, 16 + 10 + CW + 3 * CW
audio_w = 4 + vol_w + mouse_w + 4
x -= 6 + audio_w
pill(x, audio_w)
add(g_speaker(x + 4 + 8 + 7, CY, FROST))
add(tx(x + 4 + 8 + GW + CW, MID, "35%", BAR_SIZE, FROST))
mx = x + 4 + vol_w + 8
add(f'<rect x="{mx}" y="{CY - 7}" width="10" height="14" rx="5" fill="none" stroke="{BAR_DIM}" stroke-width="1.5"/><line x1="{mx + 5}" y1="{CY - 6}" x2="{mx + 5}" y2="{CY - 1}" stroke="{BAR_DIM}" stroke-width="1.5"/>')
add(tx(mx + 10 + CW, MID, "80%", BAR_SIZE, BAR_DIM))

# The lamps: Roundhouse (calm), Switchyard (at rest), Signal Box (nothing
# granted), the VPN (down) and the bell.
LAMP = 16 + GW
lamps_w = 4 + 5 * LAMP + 4
x -= 6 + lamps_w
pill(x, lamps_w)
LX = [x + 4 + LAMP / 2 + i * LAMP for i in range(5)]
add(f'<circle cx="{LX[0]}" cy="{CY}" r="5.5" fill="{GO}"/>')
add(f'<rect x="{LX[1] - 5}" y="{CY - 9}" width="10" height="18" rx="4" fill="none" stroke="{FROST}" stroke-width="1.4"/>')
for dy in (-5, 0, 5):
    add(f'<circle cx="{LX[1]}" cy="{CY + dy}" r="1.8" fill="{FROST}"/>')
add(f'<circle cx="{LX[2]}" cy="{CY}" r="5.5" fill="none" stroke="{BAR_DIM}" stroke-width="1.6"/>')
vx = LX[3]
add(f'<g fill="none" stroke="{BAR_DIM}" stroke-width="1.4"><rect x="{vx - 2.5}" y="{CY - 7}" width="5" height="4" rx="1"/><rect x="{vx - 7}" y="{CY + 3}" width="5" height="4" rx="1"/><rect x="{vx + 2}" y="{CY + 3}" width="5" height="4" rx="1"/>'
    f'<path d="M{vx},{CY - 3} v2.5 M{vx - 4.5},{CY + 3} v-2 h3 M{vx + 4.5},{CY + 3} v-2 h-3"/></g>')
BELL_X = LX[4]
group(fade([(0, DND_ON), (DND_OFF, T)], 0.04))
add(g_bell(BELL_X, CY, BAR_DIM))
add("</g>")
group(fade([(DND_ON, DND_OFF)], 0.04), False)
add(g_bell(BELL_X, CY, AMBER, off=True))
add("</g>")
k = anim("opacity", [(0, 0), (DND_ON, 0), (DND_ON + 0.05, .8), (DND_ON + 0.7, 0), (T, 0)])
k2 = anim("transform", [(0, "scale(.4)"), (DND_ON, "scale(.4)"), (DND_ON + 0.7, "scale(1.5)"), (T, "scale(1.5)")])
add(f'<circle {style(k, k2, extra="transform-box:fill-box;transform-origin:center")} opacity="0" cx="{BELL_X}" cy="{CY}" r="13" fill="none" stroke="{AMBER}" stroke-width="2"/>')

# Tannoy's media pill, right-aligned against the lamps. The cover module
# (18 px, 10 px in) takes the pill's left end; the text module pads 8 px
# after it and 12 px at the end. Its class colors the text: playing is
# yellow, paused dim, and hidden (do not disturb) leaves only a dim music
# glyph in a pill of its own, with no cover.
MEDIA_R = x - 6
POD_BAR = ellipsize(f"{POD.artist} · {POD.title}")
SONG_BAR = ellipsize(f"{SONG.artist} · {SONG.title}")
MEDIA = [  # (text, cover, color, CSS classes, spans)
    (POD_BAR, "pod", AMBER, "playing with-art", POD_PLAYS),
    (POD_BAR, "pod", BAR_DIM, "paused with-art", [(CLICK_PAUSE + LAG, CLICK_CHIP + LAG)]),
    (SONG_BAR, "song", BAR_DIM, "paused with-art", [(CLICK_CHIP + LAG, CLICK_PLAY + LAG)]),
    (SONG_BAR, "song", AMBER, "playing with-art", [(CLICK_PLAY + LAG, DND_ON + 0.05), (DND_OFF + 0.05, RESET)]),
    (None, None, BAR_DIM, "hidden", [(DND_ON + 0.05, DND_OFF + 0.05)]),
]
for label, cover, color, _, spans in MEDIA:
    group(fade(spans, 0.02), spans[0][0] == 0)  # waybar redraws, it does not fade
    if label is None:
        pill(MEDIA_R - 40, 40)
        add(g_note(MEDIA_R - 22, CY, BAR_DIM, 0.95))
    else:
        w = 10 + 18 + 8 + len(label) * CW + 12
        px = MEDIA_R - w
        pill(px, w)
        add(f'<use href="#{cover}" x="{px + 10:.1f}" y="{CY - 9}" width="18" height="18"/>')
        add(tx(px + 36, MID, label, BAR_SIZE, color))
        if cover == "pod" and color == AMBER:
            COVER_X, COVER_Y, MEDIA_L = px + 19, CY, px
    add("</g>")
assert MEDIA_L - 6 >= LEFT_END, f"the bar overflows by {LEFT_END - MEDIA_L + 6:.0f}px"

# ---------------------------------------------------------------- the drawer
# One drawer per state, stacked in story order and anchored at the bottom
# (bar_anchor::place: centered on the click, 8 px above the bar, 10 px off
# the screen's sides). Each comes in over the one before, then that one
# goes, so the panel never thins mid-swap.
def pod_at(t):
    """The podcast's position at t: playing from the open to the pause,
    15 seconds on after the skip."""
    return POD.at + min(max(t, OPEN), CLICK_PAUSE + LAG) - OPEN + (15 if t >= CLICK_SKIP + LAG else 0)


def song_at(t):
    return SONG.at + max(0.0, t - CLICK_PLAY - LAG)


STATES = [  # (state, spans, scrub)
    (DrawerState(POD), [(0, CLICK_PAUSE + LAG)], "pod"),
    (DrawerState(replace(POD, at=pod_at(CLICK_PAUSE + LAG)), "paused", players=(("Chrome", "paused"), ("Spotify", "paused"))), [(CLICK_PAUSE + LAG, CLICK_CHIP + LAG)], None),
    (DrawerState(SONG, "paused", players=(("Chrome", "paused"), ("Spotify", "paused")), picked=True), [(CLICK_CHIP + LAG, CLICK_PLAY + LAG)], None),
    (DrawerState(SONG, players=(("Chrome", "paused"), ("Spotify", "playing")), picked=True), [(CLICK_PLAY + LAG, DND_ON)], "song"),
    (DrawerState(SONG, hidden=True, players=(("Chrome", "paused"), ("Spotify", "playing")), picked=True), [(DND_ON, DND_OFF)], None),
    (DrawerState(SONG, players=(("Chrome", "paused"), ("Spotify", "playing")), picked=True), [(DND_OFF, T)], "song"),
]
DH = max(drawer_height(s) for s, _, _ in STATES)
DX = min(max(10, COVER_X - DRAWER_W / 2), W - 10 - DRAWER_W)
DY = BAR_Y - 8 - DH


def skipped(kx, ky):
    """What the skip did, over the knob."""
    k = frames([(0, "opacity: 0; transform: none"), (CLICK_SKIP + LAG, "opacity: 0; transform: translateY(4px)"),
                (CLICK_SKIP + LAG + 0.15, "opacity: 1; transform: none"), (CLICK_SKIP + 1.0, "opacity: 1; transform: translateY(-4px)"),
                (CLICK_SKIP + 1.3, "opacity: 0; transform: translateY(-6px)"), (T, "opacity: 0; transform: none")])
    return tx(kx + 8, ky - 10, "+15s", 11, FROST, 700, "middle", attrs=f'{style(k)} opacity="0"')


skip_at = CLICK_SKIP + LAG
pod_times = sorted({0, OPEN, *[OPEN + i for i in range(1, 4)], skip_at, skip_at + 0.18, CLICK_PAUSE + LAG, T})
POD_SCRUB = moving_scrub(POD, lambda t: pod_at(t if t != skip_at else t - 0.001), pod_times, skipped)
song_times = [0, CLICK_PLAY + LAG] + [CLICK_PLAY + LAG + i for i in range(1, 8)] + [T]
SONG_SCRUB = moving_scrub(SONG, song_at, song_times)

spring = anim("transform", [
    (0, "translateY(26px) scale(.06)"), (OPEN - 0.01, "translateY(26px) scale(.06)"),
    (OPEN + 0.10, "translateY(14px) scale(.5)"), (OPEN + 0.20, "translateY(4px) scale(.9)"),
    (OPEN + 0.30, "translateY(-1px) scale(1.035)"), (OPEN + 0.42, "translateY(0) scale(.988)"),
    (OPEN + 0.56, "scale(1.006)"), (OPEN + 0.72, "scale(1)"), (CLICK_OUT + 0.08, "scale(1)"),
    (CLOSED, "translateY(14px) scale(.4)"), (T, "translateY(14px) scale(.4)"),
])
show = anim("opacity", [(0, 0), (OPEN - 0.01, 0), (OPEN + 0.12, 0.85), (OPEN + 0.2, 1), (CLICK_OUT + 0.08, 1), (CLOSED, 0), (T, 0)])
add(f'<g {style(spring, show, extra=f"transform-origin:{COVER_X:.1f}px {BAR_Y}px")}><g transform="translate({DX:.1f},{DY:.1f})">')
SPOTS = []
for i, (s, spans, hook) in enumerate(STATES):
    svg, h, spots = draw_drawer(s, POD_SCRUB if hook == "pod" else SONG_SCRUB if hook == "song" else None)
    SPOTS.append((spots, DH - h))
    # The track crossfades as GTK's stack does; labels change at once.
    ramp = 0.16 if i in (2, 4, 5) else 0.04
    nxt = STATES[i + 1][1][0][0] if i + 1 < len(STATES) else None
    end = spans[0][1] + (0.16 if i + 1 in (2, 4, 5) else 0.04) if nxt is not None else T
    group(fade([(spans[0][0], end)], ramp, 0.01), i == 0)
    add(f'<g transform="translate(0,{DH - h})">{svg}</g>')
    add("</g>")


def spot(i, name):
    spots, dy = SPOTS[i]
    cx, cy = center(spots[name])
    return cx, cy + dy


def flash(i, name, t):
    spots, dy = SPOTS[i]
    x, y, w, h = spots[name]
    add(f'<rect x="{x:.1f}" y="{y + dy:.1f}" width="{w}" height="{h}" rx="10" fill="#fff" fill-opacity=".14" {style(press(t))} opacity="0"/>')


flash(0, "fwd", CLICK_SKIP)
flash(0, "play", CLICK_PAUSE)
flash(2, "play", CLICK_PLAY)
add("</g></g>")

# ---------------------------------------------------------------- cursor


def screen(p):
    return (DX + p[0], DY + p[1])


skip_c = screen(spot(0, "fwd"))
pause_c = screen(spot(0, "play"))
chip_c = screen(spot(1, "chip:Spotify"))
play_c = screen(spot(2, "play"))
rest = (DX + DRAWER_W + 50, DY + DH - 190)  # off the drawer while do not disturb comes and goes
outside = (DX + DRAWER_W + 120, 300)
clicks = [(CLICK_BAR, COVER_X, COVER_Y), (CLICK_SKIP, *skip_c), (CLICK_PAUSE, *pause_c), (CLICK_CHIP, *chip_c),
          (CLICK_PLAY, *play_c), (CLICK_OUT, *outside)]
for t, cx_, cy_ in clicks:
    k = anim("opacity", [(0, 0), (t - 0.01, 0), (t, 0.9), (t + 0.45, 0), (T, 0)])
    k2 = anim("transform", [(0, "scale(.3)"), (t, "scale(.3)"), (t + 0.45, "scale(1.6)"), (T, "scale(1.6)")])
    add(f'<circle class="a ripple" style="animation-name:{k},{k2};transform-box:fill-box;transform-origin:center" cx="{cx_:.1f}" cy="{cy_:.1f}" r="18" fill="none" stroke="{FROST}" stroke-width="2.5" opacity="0"/>')


def tr(p):
    return f"translate({p[0] - 3:.1f}px, {p[1] - 4:.1f}px)"


home = (DX + DRAWER_W + 140, 400)
cur = anim("transform", [
    (0, tr(home)), (CLICK_BAR - 0.15, tr((COVER_X, COVER_Y))), (2.2, tr((COVER_X, COVER_Y))),
    (CLICK_SKIP - 0.15, tr(skip_c)), (3.4, tr(skip_c)),
    (CLICK_PAUSE - 0.15, tr(pause_c)), (4.5, tr(pause_c)),
    (CLICK_CHIP - 0.15, tr(chip_c)), (5.6, tr(chip_c)),
    (CLICK_PLAY - 0.15, tr(play_c)), (6.9, tr(play_c)),
    (7.6, tr(rest)), (10.6, tr(rest)),
    (CLICK_OUT - 0.15, tr(outside)), (12.4, tr(outside)), (T, tr(home)),
])
add(f'<g id="cursor" class="a" style="animation-name:{cur};animation-timing-function:cubic-bezier(.45,0,.25,1);transform:{tr(home)}">')
add('<path d="M0,0 L0,22 L6,17 L10,26 L14,24 L10,15 L18,15 Z" fill="#eceff4" stroke="#1d2024" stroke-width="1.4" stroke-linejoin="round"/>')
add("</g>")
add("</g></svg>")

out[STYLE_AT] = "<style>" + css + "\n".join(kfs) + "</style>"
Path(__file__).with_name("hero.svg").write_text("\n".join(out) + "\n")
print(f"wrote hero.svg ({sum(len(s) + 1 for s in out) // 1024} KB, {len(kfs)} keyframe sets); "
      f"drawer {DRAWER_W}x{DH} at ({DX:.0f}, {DY:.0f}), bar slack {MEDIA_L - 6 - LEFT_END:.0f}px")
