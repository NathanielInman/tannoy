#!/usr/bin/env python3
"""Writes docs/assets/hero.svg: the README's animated hero.

A 14 second loop on the same desktop as Roundhouse's and Switchyard's
heroes, under the floating Platform bar: the cover in the media pill is
clicked and the drawer springs out of it, a podcast plays in Chrome and
skips ahead 15 seconds, Spotify is picked and played, do not disturb hides
the title everywhere and gives it back, and a click outside closes the
drawer.

Pure CSS animation inside the SVG, so GitHub plays it in an <img>. With
reduced motion it is a still of the open drawer playing the podcast.

    python3 docs/assets/hero.py
"""

import math
from pathlib import Path

W, H = 1200, 660
T = 14  # loop seconds

# Nord, matching the Platform waybar and the siblings' heroes.
BG0, BG1 = "#14171c", "#0d0f12"
PILL, PILL_LINE, PILL_HOVER = "rgba(40,42,46,.84)", "rgba(76,86,106,.55)", "rgba(55,62,74,.92)"
BAR_FG, BRIGHT, BAR_DIM = "#c5c8c6", "#d8dee9", "#616e88"
PANEL, PANEL_LINE, WELL = "#1e2024", "#41464e", "#15171a"
TILE, TILE_LINE = "#25282c", "#2f3237"
FG, DIM = "#c5c8c6", "#7c838e"
FROST, GO, AMBER, PURPLE = "#88c0d0", "#a3be8c", "#ebcb8b", "#b48ead"
FONT = "JetBrains Mono, DejaVu Sans Mono, Menlo, Consolas, monospace"

# The README's alt text says the same.
ALT = (
    "A floating waybar whose media pill shows a small podcast cover and Slow Signals · Where the Night Trains…, "
    "cut at a word with a real ellipsis. A click on the cover opens the Tannoy drawer out of it: the podcast "
    "playing in Chrome with its scrub bar moving, play and pause, previous and next, and 15 second skips. "
    "A click on +15 jumps ahead. A click on the Spotify chip swaps the cover, the title and the bar pill to a song, "
    "the skips go away, and play starts it. Do not disturb turns on: the bell turns amber, the bar keeps only a dim "
    "music glyph, and the drawer says the title is hidden while its controls stay. Do not disturb turns off again, "
    "and a click outside closes the drawer."
)

# The bar floats: pills on a transparent surface, 10px off the bottom.
PY, PH = 614, 36
MID = PY + PH / 2 + 4.5
BAR_SIZE, CW = 13, 7.8  # bar font size and its character width


def ellipsize(text, limit=40):
    """tannoy_core::text::ellipsize: cut at a word, end on a real ellipsis."""
    if len(text) <= limit:
        return text
    keep = text[:limit - 1]
    cut = keep if text[limit - 1].isspace() else keep[:keep.rfind(" ")]
    while cut != (trimmed := cut.rstrip().rstrip(",;:-·|/([&").rstrip()):
        cut = trimmed
    return cut + "…"


# The two tracks. Made up, and so are the covers.
POD = dict(show="Slow Signals", title=["Where the Night Trains Go,", "with Iris Calloway"], at=552, length=1600)
POD["bar"] = ellipsize(f"{POD['show']} · {' '.join(POD['title'])}")
SONG = dict(artist="Marlow Fen", title="Paper Lanterns", album="Low Tide Radio", at=107, length=232)
SONG["bar"] = ellipsize(f"{SONG['artist']} · {SONG['title']}")

# The story, in seconds.
CLICK_BAR, OPEN = 1.0, 1.08
CLICK_SKIP = 3.3
CLICK_CHIP = 5.0
CLICK_PLAY = 6.4
DND_ON, DND_OFF = 8.0, 10.0
CLICK_OUT, CLOSED = 11.6, 12.05
RESET = 13.2
LAG = 0.1  # a click reaches the player and comes back as a snapshot
SNAP = 0.04  # labels and buttons change at once; only the track crossfades

out, kfs = [], []


def add(s):
    out.append(s)


def esc(s):
    return s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


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


def fade(spans, ramp=0.16):
    """Opacity keyframes visible during each (start, end) span."""
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
            pts += [(e, 1), (e + ramp, 0)]
    if pts[-1][0] < T:
        pts.append((T, 0))
    return anim("opacity", pts)


def style(*names, extra=""):
    return f'class="a" style="animation-name:{",".join(names)};{extra}"'


def group(name, shown=True, extra=""):
    """A group that fades by `name`; `shown` is its state in the still."""
    add(f'<g {style(name, extra=extra)} opacity="{1 if shown else 0}">')


def text(x, y, s, size=13, fill=FG, weight=400, anchor="start", spacing=0, attrs=""):
    ls = f' letter-spacing="{spacing}"' if spacing else ""
    attrs = f" {attrs}" if attrs else ""
    add(f'<text x="{x:.1f}" y="{y:.1f}" font-size="{size}" fill="{fill}" font-weight="{weight}" text-anchor="{anchor}"{ls}{attrs}>{esc(s)}</text>')


def rise(at):
    """Content arriving in the drawer, a beat after it opens."""
    return frames([(0, "opacity: 0; transform: translateY(12px)"), (OPEN + at, "opacity: 0; transform: translateY(12px)"),
                   (OPEN + at + 0.4, "opacity: 1; transform: none"), (T, "opacity: 1; transform: none")])


def press(t):
    """A button's flash under a click."""
    return anim("opacity", [(0, 0), (t - 0.01, 0), (t, 1), (t + 0.3, 0), (T, 0)])


def ticks(x, y, anchor, steps, still=None):
    """A clock label: (from, to, text) steps that swap without a fade."""
    for s, e, label in steps:
        group(fade([(s, e)], 0.01), label == still)
        text(x, y, label, 11.5, DIM, anchor=anchor)
        add("</g>")


def clock(sec):
    return f"{int(sec) // 60}:{int(sec) % 60:02d}"


# ---------------------------------------------------------------- glyphs
# Drawn, not typed: the bar's Nerd Font glyphs would not survive an <img>.

def g_play(cx, cy, c):
    add(f'<path d="M{cx - 4.5},{cy - 6.5} L{cx - 4.5},{cy + 6.5} L{cx + 6},{cy} Z" fill="{c}" stroke="{c}" stroke-width="1.5" stroke-linejoin="round"/>')


def g_pause(cx, cy, c):
    for dx in (-4.5, 1.5):
        add(f'<rect x="{cx + dx}" y="{cy - 6.5}" width="3.4" height="13" rx="1" fill="{c}"/>')


def g_track(cx, cy, c, fwd):
    s = 1 if fwd else -1
    add(f'<path d="M{cx - 4.5 * s},{cy - 5.5} L{cx - 4.5 * s},{cy + 5.5} L{cx + 3 * s},{cy} Z" fill="{c}" stroke="{c}" stroke-width="1.2" stroke-linejoin="round"/>')
    add(f'<rect x="{cx + (3.5 if fwd else -5.5)}" y="{cy - 5.5}" width="2" height="11" rx=".8" fill="{c}"/>')


def g_skip(cx, cy, c, fwd):
    """A circular arrow around the number of seconds it jumps."""
    r = 9.5
    a0, a1 = (-100, 150) if fwd else (-80, 30)  # degrees; the gap is at the top
    p0 = (cx + r * math.cos(math.radians(a0)), cy + r * math.sin(math.radians(a0)))
    p1 = (cx + r * math.cos(math.radians(a1)), cy + r * math.sin(math.radians(a1)))
    sweep = 0 if fwd else 1
    add(f'<path d="M{p1[0]:.1f},{p1[1]:.1f} A{r},{r} 0 1 {sweep} {p0[0]:.1f},{p0[1]:.1f}" fill="none" stroke="{c}" stroke-width="1.7" stroke-linecap="round"/>')
    s = 1 if fwd else -1
    x, y = p0
    add(f'<path d="M{x + 4.2 * s:.1f},{y:.1f} L{x - 0.6 * s:.1f},{y - 3.6:.1f} L{x - 0.6 * s:.1f},{y + 3.6:.1f} Z" fill="{c}"/>')
    text(cx, cy + 3.2, "15", 8.5, c, 700, "middle")


def g_bell(cx, cy, c, off=False):
    add(f'<path d="M{cx - 5.5},{cy + 3.5} L{cx - 5.5},{cy - 1} Q{cx - 5.5},{cy - 6.5} {cx},{cy - 6.5} Q{cx + 5.5},{cy - 6.5} {cx + 5.5},{cy - 1} L{cx + 5.5},{cy + 3.5} L{cx + 7},{cy + 5} L{cx - 7},{cy + 5} Z" fill="{c}"/>')
    add(f'<circle cx="{cx}" cy="{cy + 7}" r="1.8" fill="{c}"/>')
    if off:
        add(f'<line x1="{cx - 7.5}" y1="{cy - 7.5}" x2="{cx + 7.5}" y2="{cy + 7.5}" stroke="#1c1e22" stroke-width="3.6"/>')
        add(f'<line x1="{cx - 7.5}" y1="{cy - 7.5}" x2="{cx + 7.5}" y2="{cy + 7.5}" stroke="{c}" stroke-width="1.7" stroke-linecap="round"/>')


def g_note(cx, cy, c, s=1.0):
    """md-music: two beamed notes, what do not disturb leaves in the bar."""
    add(f'<g transform="translate({cx},{cy}) scale({s})" fill="{c}">'
        '<ellipse cx="-4.2" cy="5" rx="2.9" ry="2.3"/><ellipse cx="4.6" cy="3.2" rx="2.9" ry="2.3"/>'
        '<rect x="-1.9" y="-6.5" width="1.6" height="11.5"/><rect x="6.9" y="-8.3" width="1.6" height="11.5"/>'
        '<path d="M-1.9,-6.5 L8.5,-8.3 L8.5,-5.6 L-1.9,-3.8 Z"/></g>')


def g_speaker(cx, cy, c):
    add(f'<path d="M{cx - 7},{cy - 2.5} L{cx - 4},{cy - 2.5} L{cx},{cy - 6} L{cx},{cy + 6} L{cx - 4},{cy + 2.5} L{cx - 7},{cy + 2.5} Z" fill="{c}"/>')
    add(f'<path d="M{cx + 3},{cy - 3} Q{cx + 5.5},{cy} {cx + 3},{cy + 3} M{cx + 5.5},{cy - 5.5} Q{cx + 10},{cy} {cx + 5.5},{cy + 5.5}" fill="none" stroke="{c}" stroke-width="1.5" stroke-linecap="round"/>')


# ---------------------------------------------------------------- scene

css = f"""
.a {{ animation-duration: {T}s; animation-iteration-count: infinite; animation-timing-function: linear; animation-fill-mode: both; }}
@media (prefers-reduced-motion: reduce) {{
  .a {{ animation: none !important; }}
  #cursor, .ripple {{ display: none; }}
}}
"""

add(f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" width="{W}" height="{H}" font-family="{FONT}" xml:space="preserve" role="img" aria-label="{esc(ALT)}">')
STYLE_AT = len(out)
add("")  # the <style> goes here once every keyframe exists


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


add(f"""<defs>
<linearGradient id="wall" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{BG0}"/><stop offset="1" stop-color="{BG1}"/></linearGradient>
<radialGradient id="haze1" cx=".85" cy="0" r=".75"><stop offset="0" stop-color="{FROST}" stop-opacity=".13"/><stop offset="1" stop-color="{FROST}" stop-opacity="0"/></radialGradient>
<radialGradient id="haze2" cx=".05" cy="1" r=".6"><stop offset="0" stop-color="{PURPLE}" stop-opacity=".09"/><stop offset="1" stop-color="{PURPLE}" stop-opacity="0"/></radialGradient>
<linearGradient id="podsky" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#22324a"/><stop offset="1" stop-color="#3b4f6b"/></linearGradient>
<linearGradient id="dusk" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#3b2b4a"/><stop offset=".7" stop-color="#b8706a"/><stop offset="1" stop-color="#d08770"/></linearGradient>
<radialGradient id="glow"><stop offset="0" stop-color="#ebcb8b" stop-opacity=".55"/><stop offset="1" stop-color="#ebcb8b" stop-opacity="0"/></radialGradient>
<clipPath id="art"><rect width="100" height="100" rx="7"/></clipPath>
<symbol id="pod" viewBox="0 0 100 100"><g clip-path="url(#art)">{cover_pod()}</g></symbol>
<symbol id="song" viewBox="0 0 100 100"><g clip-path="url(#art)">{cover_song()}</g></symbol>
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


# Notes in an editor, behind where the drawer opens.
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
g_pause(BX + 52.5, BY + 284, BAR_FG)
add(f'<rect x="{BX + 76}" y="{BY + 282}" width="{BW - 170}" height="4" rx="2" fill="#3b4252"/>')
add(f'<rect x="{BX + 76}" y="{BY + 282}" width="{(BW - 170) * POD["at"] / POD["length"]:.0f}" height="4" rx="2" fill="#7c838e"/>')
for i, w in enumerate([340, 300, 360, 220, 310, 260, 120]):
    add(f'<rect x="{BX + 28}" y="{BY + 336 + i * 24}" width="{w}" height="8" rx="4" fill="#2e3440"/>')

# ---------------------------------------------------------------- the bar


def pill(x, w, fill=PILL):
    add(f'<rect x="{x:.1f}" y="{PY}" width="{w:.1f}" height="{PH}" rx="10" fill="{fill}" stroke="{PILL_LINE}"/>')


# Left: workspaces and the window title.
pill(10, 148)
for i in range(5):
    x = 16 + i * 28
    if i == 1:
        add(f'<rect x="{x}" y="{PY + 5}" width="26" height="{PH - 10}" rx="7" fill="rgba(129,161,193,.26)"/>')
    text(x + 13, MID, str(i + 1), BAR_SIZE, BRIGHT if i == 1 else BAR_FG, 700 if i == 1 else 400, "middle")
pill(164, 14 * CW + 24)
text(176, MID, "nvim  notes.md", BAR_SIZE, BRIGHT)

# Right, packed from the edge: clock, audio, lamps, media.
x = W - 10
clock_w = 17 * CW + 24
x -= clock_w
pill(x, clock_w)
text(x + 12, MID, "Tue Nov 10  14:32", BAR_SIZE, PURPLE)
audio_w = 12 + 16 + 6 + 3 * CW + 16 + 10 + 6 + 3 * CW + 12
x -= 6 + audio_w
pill(x, audio_w)
g_speaker(x + 20, PY + PH / 2, FROST)
text(x + 34, MID, "35%", BAR_SIZE, FROST)
mx = x + 34 + 3 * CW + 16
add(f'<rect x="{mx}" y="{PY + 11}" width="10" height="14" rx="5" fill="none" stroke="{BAR_DIM}" stroke-width="1.5"/><line x1="{mx + 5}" y1="{PY + 12}" x2="{mx + 5}" y2="{PY + 17}" stroke="{BAR_DIM}" stroke-width="1.5"/>')
text(mx + 16, MID, "80%", BAR_SIZE, BAR_DIM)
lamps_w = 4 * 30 + 12
x -= 6 + lamps_w
pill(x, lamps_w)
LX = [x + 21 + i * 30 for i in range(4)]
cy = PY + PH / 2
add(f'<circle cx="{LX[0]}" cy="{cy}" r="6" fill="{GO}"/>')  # Roundhouse
add(f'<rect x="{LX[1] - 5}" y="{cy - 9}" width="10" height="18" rx="5" fill="none" stroke="{FROST}" stroke-width="1.5"/>')  # Switchyard
for dy, o in [(-4.5, .35), (0, .35), (4.5, 1)]:
    add(f'<circle cx="{LX[1]}" cy="{cy + dy}" r="1.6" fill="{FROST}" opacity="{o}"/>')
add(f'<circle cx="{LX[2]}" cy="{cy}" r="5.5" fill="none" stroke="{BAR_DIM}" stroke-width="1.6"/>')  # Signal Box
BELL_X = LX[3]
dnd = [(DND_ON, DND_OFF)]
group(fade([(0, DND_ON), (DND_OFF, T)], 0.04))
g_bell(BELL_X, cy, BAR_DIM)
add("</g>")
group(fade(dnd, 0.04), False)
g_bell(BELL_X, cy, AMBER, off=True)
add("</g>")
k = anim("opacity", [(0, 0), (DND_ON, 0), (DND_ON + 0.05, .8), (DND_ON + 0.7, 0), (T, 0)])
k2 = anim("transform", [(0, "scale(.4)"), (DND_ON, "scale(.4)"), (DND_ON + 0.7, "scale(1.5)"), (T, "scale(1.5)")])
add(f'<circle {style(k, k2, extra="transform-box:fill-box;transform-origin:center")} opacity="0" cx="{BELL_X}" cy="{cy}" r="13" fill="none" stroke="{AMBER}" stroke-width="2"/>')

# The media pill, right-aligned against the lamps: the cover hands its left
# end to the text, and do not disturb leaves only a dim glyph.
MEDIA_R = x - 6


def media(t, color, cover, spans, shown, hover=None):
    w = 10 + 20 + 8 + len(t) * CW + 12
    px = MEDIA_R - w
    group(fade(spans, 0.02), shown)  # waybar redraws, it does not fade
    pill(px, w)
    if hover:
        add(f'<rect x="{px:.1f}" y="{PY}" width="{w:.1f}" height="{PH}" rx="10" fill="{PILL_HOVER}" {style(hover)} opacity="0"/>')
    add(f'<use href="#{cover}" x="{px + 10:.1f}" y="{PY + 8}" width="20" height="20"/>')
    text(px + 38, MID, t, BAR_SIZE, color)
    add("</g>")
    return px + 20, PY + 18


hover = fade([(CLICK_BAR - 0.2, 2.55)], 0.12)
COVER_X, COVER_Y = media(POD["bar"], AMBER, "pod", [(0, CLICK_CHIP + LAG), (RESET, T)], True, hover)
media(SONG["bar"], BAR_DIM, "song", [(CLICK_CHIP + LAG, CLICK_PLAY + LAG)], False)
media(SONG["bar"], AMBER, "song", [(CLICK_PLAY + LAG, DND_ON + 0.05), (DND_OFF + 0.05, RESET)], False)
group(fade([(DND_ON + 0.05, DND_OFF + 0.05)], 0.02), False)
pill(MEDIA_R - 40, 40)
g_note(MEDIA_R - 20, cy, BAR_DIM, 0.95)
add("</g>")

# ---------------------------------------------------------------- the drawer
DW, DH = 460, 462
DX, DY = max(10, COVER_X - DW / 2), PY - 10 - DH
CAP = 40  # the panel's rounded ends
# Do not disturb swaps the track (64..236) for a shorter card; the panel is
# anchored to the bar, so its top comes down by the difference.
CARD_H = 94
DROP = (236 - 64) - CARD_H
SLIDE = [(0, "none"), (DND_ON + 0.04, "none"), (DND_ON + 0.3, f"translateY({DROP}px)"),
         (DND_OFF, f"translateY({DROP}px)"), (DND_OFF + 0.26, "none"), (T, "none")]
mid_h = DH - 2 * CAP
SQUASH = [(t, v if v == "none" else f"scaleY({(mid_h - DROP) / mid_h:.4f})") for t, v in SLIDE]

spring = anim("transform", [
    (0, "translateY(26px) scale(.06)"), (OPEN - 0.01, "translateY(26px) scale(.06)"),
    (OPEN + 0.10, "translateY(14px) scale(.5)"), (OPEN + 0.20, "translateY(4px) scale(.9)"),
    (OPEN + 0.30, "translateY(-1px) scale(1.035)"), (OPEN + 0.42, "translateY(0) scale(.988)"),
    (OPEN + 0.56, "scale(1.006)"), (OPEN + 0.72, "scale(1)"), (CLICK_OUT + 0.08, "scale(1)"),
    (CLOSED, "translateY(14px) scale(.4)"), (T, "translateY(14px) scale(.4)"),
])
show = anim("opacity", [(0, 0), (OPEN - 0.01, 0), (OPEN + 0.12, 0.85), (OPEN + 0.2, 1), (CLICK_OUT + 0.08, 1), (CLOSED, 0), (T, 0)])
add(f'<g {style(spring, show, extra=f"transform-origin:{COVER_X:.1f}px {COVER_Y:.1f}px")}><g transform="translate({DX:.1f},{DY:.1f})">')

# The panel in three pieces, so it can shrink from the top.
slide, squash = anim("transform", SLIDE), anim("transform", SQUASH)
add('<g filter="url(#shadow)">')
add(f'<g {style(squash, extra="transform-box:fill-box;transform-origin:center bottom")}>')
add(f'<rect y="{CAP}" width="{DW}" height="{mid_h + 1}" fill="{PANEL}"/>')
add(f'<path d="M0,{CAP} V{DH - CAP + 1} M{DW},{CAP} V{DH - CAP + 1}" stroke="{PANEL_LINE}" vector-effect="non-scaling-stroke"/>')
add("</g>")
add(f'<path d="M0,{DH - CAP} V{DH - 16} A16,16 0 0 0 16,{DH} H{DW - 16} A16,16 0 0 0 {DW},{DH - 16} V{DH - CAP}" fill="{PANEL}" stroke="{PANEL_LINE}"/>')
# The top last, overlapping the middle well, so the joint never shows.
add(f'<path d="M0,{CAP + 8} V16 A16,16 0 0 1 16,0 H{DW - 16} A16,16 0 0 1 {DW},16 V{CAP + 8}" fill="{PANEL}" stroke="{PANEL_LINE}" {style(slide)}/>')
add("</g>")

# States, as spans: the podcast in Chrome, Spotify paused, then playing,
# and do not disturb over the top.
S_POD = [(0, CLICK_CHIP + LAG)]
S_PAUSED = [(CLICK_CHIP + LAG, CLICK_PLAY + LAG)]
S_PLAYING = [(CLICK_PLAY + LAG, DND_ON), (DND_OFF, T)]
S_SONG = [(CLICK_CHIP + LAG, DND_ON), (DND_OFF + 0.12, T)]
S_DND = [(DND_ON, DND_OFF)]

# Header: the lamp, TANNOY, the active player and a state chip.
add(f'<g {style(rise(0.0))}><g {style(slide)}>')
for spans, color, label, shown in [(S_POD + S_PLAYING, GO, "playing", True), (S_PAUSED, FROST, "paused", False), (S_DND, AMBER, "hidden", False)]:
    group(fade(spans, SNAP), shown)
    add(f'<circle cx="24" cy="26" r="4.5" fill="{color}"/>')
    w = len(label) * 6.9 + 20
    add(f'<rect x="{DW - 18 - w:.1f}" y="16" width="{w:.1f}" height="22" rx="11" fill="{color}" fill-opacity=".15"/>')
    text(DW - 18 - w / 2, 31, label, 11.5, color, 700, "middle")
    add("</g>")
text(38, 31, "TANNOY", 13, FG, 700, spacing=2)
for spans, player, shown in [(S_POD, "Chrome", True), ([(CLICK_CHIP + LAG, T)], "Spotify", False)]:
    group(fade(spans, SNAP), shown)
    text(38, 48, f"{player} · 2 players", 11.5, DIM)
    add("</g>")
add("</g></g>")


def scrub(frac_at, length_text, steps, moves, still=None):
    """The scrub bar: a trough, its fill, the knob, and the clocks under it."""
    x0, w, y = 18, DW - 36, 206
    add(f'<rect x="{x0}" y="{y}" width="{w}" height="4" rx="2" fill="{TILE_LINE}"/>')
    fill = anim("transform", [(t, f"scaleX({f / frac_at:.4f})") for t, f in moves])
    add(f'<rect x="{x0}" y="{y}" width="{w * frac_at:.1f}" height="4" rx="2" fill="{FROST}" {style(fill, extra="transform-box:fill-box;transform-origin:left center")}/>')
    knob = anim("transform", [(t, f"translateX({(f - frac_at) * w:.2f}px)") for t, f in moves])
    add(f'<circle cx="{x0 + w * frac_at:.1f}" cy="{y + 2}" r="6.5" fill="{FG}" stroke="#0b0c0e" stroke-opacity=".5" {style(knob)}/>')
    ticks(x0, 230, "start", steps, still)
    text(DW - 18, 230, length_text, 11.5, DIM, anchor="end")
    return x0 + w * frac_at, y


def pod_at(t):
    """The podcast's position at t seconds, 15 seconds on after the skip."""
    return POD["at"] + max(0.0, t - OPEN) + (15 if t >= CLICK_SKIP + LAG else 0)


# The podcast: a long track with no album in a browser tab.
add(f'<g {style(rise(0.06))}>')
group(fade(S_POD, 0.16))
add('<use href="#pod" x="18" y="64" width="128" height="128"/>')
text(160, 98, "PODCAST · CHROME", 10.5, DIM, 700, spacing=1)
text(160, 122, POD["title"][0], 15.5, FG, 700)
text(160, 142, POD["title"][1], 15.5, FG, 700)
text(160, 164, POD["show"], 13, FG)
L = POD["length"]
steps, edges = [], [0, OPEN + 1, OPEN + 2, CLICK_SKIP + LAG, OPEN + 3, OPEN + 4, T]
for s, e in zip(edges, edges[1:]):
    steps.append((s, e, clock(pod_at(s + 0.001))))
before = pod_at(CLICK_SKIP + LAG - 0.001) / L
moves = [(0, POD["at"] / L), (OPEN, POD["at"] / L), (CLICK_SKIP + LAG, before),
         (CLICK_SKIP + LAG + 0.18, before + 15 / L), (CLICK_CHIP + 0.5, pod_at(CLICK_CHIP + 0.5) / L), (T, pod_at(CLICK_CHIP + 0.5) / L)]
kx, ky = scrub(POD["at"] / L, clock(L), steps, moves, clock(POD["at"]))
# What the skip did, over the knob.
k = frames([(0, "opacity: 0; transform: none"), (CLICK_SKIP + LAG, "opacity: 0; transform: translateY(4px)"),
            (CLICK_SKIP + LAG + 0.15, "opacity: 1; transform: none"), (CLICK_SKIP + 1.1, "opacity: 1; transform: translateY(-4px)"),
            (CLICK_SKIP + 1.4, "opacity: 0; transform: translateY(-6px)"), (T, "opacity: 0; transform: none")])
text(kx + 4, ky - 9, "+15s", 11, FROST, 700, "middle", attrs=f'{style(k)} opacity="0"')
add("</g>")

# The song, from Spotify.
group(fade(S_SONG, 0.16), False)
add('<use href="#song" x="18" y="64" width="128" height="128"/>')
text(160, 104, "SPOTIFY", 10.5, DIM, 700, spacing=1)
text(160, 128, SONG["title"], 15.5, FG, 700)
text(160, 149, SONG["artist"], 13, FG)
text(160, 168, SONG["album"], 12, DIM)
L = SONG["length"]


def song_at(t):
    return SONG["at"] + max(0.0, t - CLICK_PLAY - LAG)


edges = [0] + [CLICK_PLAY + LAG + i for i in range(1, 8)] + [T]
steps = [(s, e, clock(song_at(s + 0.001))) for s, e in zip(edges, edges[1:])]
scrub(SONG["at"] / L, clock(L), steps, [(0, SONG["at"] / L), (CLICK_PLAY + LAG, SONG["at"] / L), (T, song_at(T) / L)])
add("</g>")

# Hidden by do not disturb: the same place, a shorter card.
group(fade([(DND_ON + 0.12, DND_OFF)], 0.16), False)
cy0 = 236 - CARD_H
add(f'<rect x="18" y="{cy0}" width="{DW - 36}" height="{CARD_H}" rx="10" fill="{AMBER}" fill-opacity=".07" stroke="{AMBER}" stroke-opacity=".3"/>')
text(30, cy0 + 21, "HIDDEN", 10.5, AMBER, 700, spacing=1)
text(DW - 30, cy0 + 21, "DO NOT DISTURB IS ON", 10.5, DIM, 700, "end", 1)
for i, line in enumerate(["The title and cover stay off the bar while do not",
                          "disturb is on, so they stay off screen shares. The",
                          "controls still work."]):
    text(30, cy0 + 45 + i * 18, line, 12.5, FG)
add("</g></g>")

# Controls: the skips only while a podcast is up.
add(f'<g {style(rise(0.12))}>')
CY0, CBH = 248, 38
widths = [48, 48, 66, 48, 48]
bx = (DW - sum(widths) - 4 * 9) / 2
BTN = []
for w in widths:
    BTN.append((bx, w))
    bx += w + 9


def button(i, primary=False, flash=None):
    x, w = BTN[i]
    fill, line = (FROST, FROST) if primary else (TILE, TILE_LINE)
    add(f'<rect x="{x:.1f}" y="{CY0}" width="{w}" height="{CBH}" rx="10" fill="{fill}" stroke="{line}"/>')
    if flash:
        add(f'<rect x="{x:.1f}" y="{CY0}" width="{w}" height="{CBH}" rx="10" fill="#fff" fill-opacity=".14" {style(flash)} opacity="0"/>')
    return x + w / 2, CY0 + CBH / 2


group(fade(S_POD, SNAP))
g_skip(*button(0), FG, False)
SKIP = button(4, flash=press(CLICK_SKIP))
g_skip(*SKIP, FG, True)
add("</g>")
g_track(*button(1), FG, False)
PLAY = button(2, primary=True, flash=press(CLICK_PLAY))
group(fade(S_POD + [(CLICK_PLAY + LAG, T)], SNAP))
g_pause(*PLAY, WELL)
add("</g>")
group(fade(S_PAUSED, SNAP), False)
g_play(PLAY[0] + 1, PLAY[1], WELL)
add("</g>")
g_track(*button(3), FG, True)
add("</g>")

# Players: a chip each, the active one lit; picking adds Auto.
add(f'<g {style(rise(0.18))}>')
text(18, 314, "PLAYERS", 10.5, DIM, 700, spacing=1)
for spans, aside, shown in [(S_POD, "MEDIA KEYS FOLLOW THE LATEST", True), ([(CLICK_CHIP + LAG, T)], "PICKED · MEDIA KEYS FOLLOW IT", False)]:
    group(fade(spans, SNAP), shown)
    text(DW - 18, 314, aside, 10.5, DIM, 700, "end", 1)
    add("</g>")
picked = [(CLICK_CHIP + LAG, T)]
cx = 18
CHIPS = {}
for name, on, dots in [("Chrome", S_POD, [(GO, [(0, T)])]),
                       ("Spotify", picked, [(DIM, [(0, CLICK_PLAY + LAG)]), (GO, [(CLICK_PLAY + LAG, T)])]),
                       ("Auto", None, [])]:
    w = 20 + (12 if dots else 0) + len(name) * 6.9
    CHIPS[name] = (cx + w / 2, 335)
    if not on:
        group(fade(picked, SNAP), False)
    add(f'<rect x="{cx:.1f}" y="322" width="{w:.1f}" height="26" rx="13" fill="none" stroke="{PANEL_LINE}"/>')
    tx = cx + (22 if dots else 10)
    if on:
        # Lit while this player is the active one.
        group(fade(on, SNAP), name == "Chrome")
        add(f'<rect x="{cx:.1f}" y="322" width="{w:.1f}" height="26" rx="13" fill="{FROST}" fill-opacity=".08" stroke="{FROST}"/>')
        text(tx, 339, name, 11.5, FROST)
        add("</g>")
        off = [(CLICK_CHIP + LAG, T)] if name == "Chrome" else [(0, CLICK_CHIP + LAG)]
        group(fade(off, SNAP), name != "Chrome")
    text(tx, 339, name, 11.5, DIM)
    add("</g>")
    for color, spans in dots:
        group(fade(spans, SNAP), spans[0][0] == 0)
        add(f'<circle cx="{cx + 13.5:.1f}" cy="335" r="3.5" fill="{color}"/>')
        add("</g>")
    cx += w + 6
add("</g>")

# Output: the default sink and the switcher.
add(f'<g {style(rise(0.24))}>')
add(f'<rect x="18" y="360" width="{DW - 36}" height="60" rx="10" fill="{TILE}" stroke="{TILE_LINE}"/>')
text(30, 378, "OUTPUT", 10.5, DIM, 700, spacing=1)
g_speaker(37, 400, FROST)
text(52, 404.5, "Desk speakers · 35%", 12.5, FG)
add(f'<rect x="{DW - 30 - 64}" y="388" width="64" height="24" rx="8" fill="none" stroke="{PANEL_LINE}"/>')
text(DW - 30 - 32, 404, "Change", 11.5, FG, anchor="middle")
add("</g>")
add(f'<g {style(rise(0.3))}>')
text(18, 446, "Space play/pause · ←/→ seek · Esc close", 10.5, DIM)
add("</g>")
add("</g></g>")

# ---------------------------------------------------------------- cursor
skip_c = (DX + SKIP[0], DY + SKIP[1])
play_c = (DX + PLAY[0], DY + PLAY[1])
chip_c = (DX + CHIPS["Spotify"][0], DY + CHIPS["Spotify"][1])
outside = (640, 250)
clicks = [(CLICK_BAR, COVER_X, COVER_Y), (CLICK_SKIP, *skip_c), (CLICK_CHIP, *chip_c), (CLICK_PLAY, *play_c), (CLICK_OUT, *outside)]
for t, cx_, cy_ in clicks:
    k = anim("opacity", [(0, 0), (t - 0.01, 0), (t, 0.9), (t + 0.45, 0), (T, 0)])
    k2 = anim("transform", [(0, "scale(.3)"), (t, "scale(.3)"), (t + 0.45, "scale(1.6)"), (T, "scale(1.6)")])
    add(f'<circle class="a ripple" style="animation-name:{k},{k2};transform-box:fill-box;transform-origin:center" cx="{cx_:.1f}" cy="{cy_:.1f}" r="18" fill="none" stroke="{FROST}" stroke-width="2.5" opacity="0"/>')


def tr(p):
    return f"translate({p[0] - 3:.1f}px, {p[1] - 4:.1f}px)"


home = (780, 380)
rest = (DX + BTN[4][0] + 30, DY + CY0 + 24)  # where the forward skip was
cur = anim("transform", [
    (0, tr(home)), (CLICK_BAR - 0.15, tr((COVER_X, COVER_Y))), (2.4, tr((COVER_X, COVER_Y))),
    (CLICK_SKIP - 0.15, tr(skip_c)), (4.1, tr(skip_c)),
    (CLICK_CHIP - 0.15, tr(chip_c)), (5.6, tr(chip_c)),
    (CLICK_PLAY - 0.15, tr(play_c)), (7.2, tr(play_c)),
    (7.9, tr(rest)), (10.7, tr(rest)),
    (CLICK_OUT - 0.15, tr(outside)), (12.6, tr(outside)), (T, tr(home)),
])
add(f'<g id="cursor" class="a" style="animation-name:{cur};animation-timing-function:cubic-bezier(.45,0,.25,1);transform:{tr(home)}">')
add('<path d="M0,0 L0,22 L6,17 L10,26 L14,24 L10,15 L18,15 Z" fill="#eceff4" stroke="#1d2024" stroke-width="1.4" stroke-linejoin="round"/>')
add("</g>")
add("</g></svg>")

out[STYLE_AT] = "<style>" + css + "\n".join(kfs) + "</style>"
Path(__file__).with_name("hero.svg").write_text("\n".join(out) + "\n")
print(f"wrote hero.svg ({sum(len(s) + 1 for s in out) // 1024} KB, {len(kfs)} keyframe sets)")
