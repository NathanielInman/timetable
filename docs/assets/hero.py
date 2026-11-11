#!/usr/bin/env python3
"""Writes docs/assets/hero.svg: the README's animated hero.

A 16 second loop on the same desktop as Roundhouse's and Switchyard's
heroes, in a floating bar of pills. The clock is calm until the hour before
Standup, when it names it. A click springs the drawer out of the clock:
three zones, the next meeting with Join, the day strip with its free gaps
and a now line, and a focus block sized to the gap. Starting it turns the
clock into a countdown. Later the clock goes amber, then red with Join at
start time, and a middle-click switches to the meeting's workspace.

Every event, name and time is made up.

Pure CSS animation inside the SVG, so GitHub plays it in an <img>. With
reduced motion it is a still of the open drawer.

    python3 docs/assets/hero.py
"""

from pathlib import Path

W, H = 1200, 660
T = 16  # loop seconds

# Nord, matching the waybar theme and the siblings' heroes.
BG0, BG1 = "#14171c", "#0d0f12"
PILL, PILL_LINE, PILL_ON = "#24262a", "#3a404d", "#3c4651"
BAR_FG, BAR_BRIGHT, BAR_DIM = "#c5c8c6", "#d8dee9", "#616e88"
PANEL, PANEL_LINE = "#1e2024", "#41464e"
TILE, TILE_LINE, WELL = "#26282c", "#313338", "#15171a"
FG, DIM, IDLE = "#c5c8c6", "#7c838e", "#555b64"
FROST, GO, CAUTION, STOP, CLOCK, DARK = "#88c0d0", "#a3be8c", "#ebcb8b", "#bf616a", "#b48ead", "#1d2024"
FONT = "JetBrains Mono, DejaVu Sans Mono, Menlo, Consolas, monospace"

# The bar floats 6px off the bottom; each group of modules is a pill.
BAR_Y, BAR_H = 620, 34
MID = BAR_Y + 22  # text baseline in the bar
CH = 8.4  # advance of a 14px monospace character
TT_R = W - 6 - 86 - 6  # the clock's right edge, left of the tray

# The story, in seconds.
FLIPS = (1.4, 1.62, 1.84)  # the clock runs from 13:47 to 14:33
CLICK_CLOCK, OPEN = 2.9, 2.98
CLICK_FOCUS = 5.4
TICKS = (6.4, 7.4, 8.4, 9.4)
CLICK_OUT, CLOSED = 7.7, 8.15
JUMP = (9.9, 10.12, 10.34)  # the focus block runs out, 14:41 to 14:58
STANDUP = 12.0
CLICK_JOIN, SWITCH = 13.0, 13.15
RESET = 15.0

out, kfs = [], []


def add(s):
    out.append(s)


def esc(s):
    return s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def pct(sec):
    return f"{max(0.0, min(100.0, sec / T * 100)):.2f}%"


def keys(pts):
    """A keyframe set from (seconds, declarations) points; returns its name."""
    name = f"k{len(kfs)}"
    rows = " ".join(f"{pct(t)} {{ {v}; }}" for t, v in pts)
    kfs.append(f"@keyframes {name} {{ {rows} }}")
    return name


def anim(prop, pts):
    return keys([(t, f"{prop}: {v}") for t, v in pts])


def fade(spans, ramp=0.22):
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


SHOWN, BELOW, ABOVE, GONE = "opacity: 1; transform: none", "opacity: 0; transform: translateY(14px)", "opacity: 0; transform: translateY(-14px)", "opacity: 0; transform: none"


def board(spans):
    """Keyframes for text shown during each (start, end, enter, leave) span.
    A "flip" rolls in from below and out the top, as on a departure board;
    a "cut" changes in place, for the seconds of a countdown."""
    pts = []
    for s, e, enter, leave in spans:
        if s <= 0:
            pts.append((0, SHOWN))
        else:
            hidden = BELOW if enter == "flip" else GONE
            pts += [(0, hidden)] if not pts else []
            pts += [(s, hidden), (s + (0.22 if enter == "flip" else 0.06), SHOWN)]
        if e >= T:
            pts.append((T, SHOWN))
        else:
            pts += [(e, SHOWN), (e + 0.22, ABOVE)] if leave == "flip" else [(e, SHOWN), (e + 0.06, GONE)]
    if pts[-1][0] < T:
        pts.append((T, BELOW))
    return keys(pts)


def rise(at):
    """Drawer content arriving `at` seconds after the drawer opens."""
    return keys([
        (0, "opacity: 0; transform: translateY(10px)"), (OPEN + at, "opacity: 0; transform: translateY(10px)"),
        (OPEN + at + 0.4, "opacity: 1; transform: none"), (T, "opacity: 1; transform: none"),
    ])


def style(name, extra=""):
    return f'class="a" style="animation-name:{name};{extra}"'


def text(x, y, s, size=13, fill=FG, weight=400, anchor="start", attrs=""):
    add(f'<text x="{x:.1f}" y="{y:.1f}" font-size="{size}" fill="{fill}" font-weight="{weight}" text-anchor="{anchor}" {attrs}>{esc(s)}</text>')


def label(x, y, s, fill=DIM, anchor="start"):
    """The drawer's small bold caps, as .t-label."""
    text(x, y, s, 10, fill, 700, anchor, 'letter-spacing="1"')


def group(name, base_opacity=1, extra=""):
    add(f'<g {style(name, extra)} opacity="{base_opacity}">')


def chip(right, y, s, color):
    w = len(s) * 6.6 + 18
    add(f'<rect x="{right - w:.1f}" y="{y - 10}" width="{w:.1f}" height="20" rx="10" fill="{color}" fill-opacity=".15"/>')
    text(right - w / 2, y + 4, s, 11, color, 700, "middle")


def button(x, y, s, fill=None, color=FG, size=12):
    w = len(s) * size * 0.6 + 22
    if fill:
        add(f'<rect x="{x:.1f}" y="{y}" width="{w:.1f}" height="26" rx="8" fill="{fill}"/>')
    else:
        add(f'<rect x="{x:.1f}" y="{y}" width="{w:.1f}" height="26" rx="8" fill="{TILE}" stroke="{TILE_LINE}"/>')
    text(x + w / 2, y + 17, s, size, color, 700 if fill else 400, "middle")
    return w


def pill(x, w):
    add(f'<rect x="{x:.1f}" y="{BAR_Y + 0.5}" width="{w:.1f}" height="{BAR_H - 1}" rx="10" fill="{PILL}" stroke="{PILL_LINE}"/>')


# ---------------------------------------------------------------- the clock's frames

# What the clock module prints, in order: (from, icon, event, its color,
# clock, how it arrives). Standup is at 15:00; a focus block ends 2 minutes
# before it. The module's class colors the event, and clock_color keeps the
# clock purple.
FRAMES = [
    (0, None, "free until 15:00", CLOCK, "13:47", "flip"),
    (FLIPS[0], "event", "Standup in 55m", FROST, "14:05", "flip"),
    (FLIPS[1], "event", "Standup in 41m", FROST, "14:19", "flip"),
    (FLIPS[2], "event", "Standup in 27m", FROST, "14:33", "flip"),
    (CLICK_FOCUS + 0.05, "focus", "Focus 25:00", GO, "14:33", "flip"),
    (TICKS[0], "focus", "Focus 24:59", GO, "14:33", "cut"),
    (TICKS[1], "focus", "Focus 24:58", GO, "14:33", "cut"),
    (TICKS[2], "focus", "Focus 24:57", GO, "14:33", "cut"),
    (TICKS[3], "focus", "Focus 24:56", GO, "14:33", "cut"),
    (JUMP[0], "focus", "Focus 16:12", GO, "14:41", "flip"),
    (JUMP[1], "focus", "Focus 07:30", GO, "14:50", "flip"),
    (JUMP[2], "event", "Standup in 2m", CAUTION, "14:58", "flip"),
    (STANDUP, "now", "Standup · Join", STOP, "15:00", "flip"),
    (RESET, None, "free until 15:00", CLOCK, "13:47", "flip"),
]
STILL = 3  # the frame the reduced-motion still shows


def frame_width(f):
    _, icon, event, _, clock, _ = f
    return 24 + (19 if icon else 0) + len(f"{event}  Tue Nov 10  {clock}") * CH


WIDTHS = [frame_width(f) for f in FRAMES]
W_MAX, W_STILL = max(WIDTHS), WIDTHS[STILL]


def width_pts(value):
    """Keyframes following the clock's width, with value(width) at each
    frame and a short slide between frames."""
    pts = [(0, value(WIDTHS[0]))]
    for i in range(1, len(FRAMES)):
        t = FRAMES[i][0]
        if WIDTHS[i] != WIDTHS[i - 1]:
            pts += [(t, value(WIDTHS[i - 1])), (t + 0.2, value(WIDTHS[i]))]
    pts.append((T, value(WIDTHS[-1])))
    return pts


def shift(w):
    return f"translateX({W_STILL - w:.1f}px)"


def icon(kind, x, color, cls=""):
    """The module's glyphs, drawn: a calendar, a camera, a timer."""
    c = f' class="{cls}"' if cls else ""
    y = MID - 11
    if kind == "event":
        add(f'<g{c} fill="none" stroke="{color}" stroke-width="1.6"><rect x="{x + 1}" y="{y + 1.5}" width="12" height="11" rx="2"/><path d="M{x + 1},{y + 5.5} h12 M{x + 4},{y} v3 M{x + 10},{y} v3"/></g>')
    elif kind == "now":
        add(f'<g{c} fill="{color}" stroke="none"><rect x="{x}" y="{y + 2.5}" width="10" height="9" rx="2"/><path d="M{x + 10.5},{y + 7} l4,-3.5 v7 z"/></g>')
    elif kind == "focus":
        add(f'<g{c} fill="none" stroke="{color}" stroke-width="1.6"><circle cx="{x + 7}" cy="{y + 7.5}" r="5.6"/><path d="M{x + 7},{y + 7.5} v-3.4 M{x + 5},{y} h4"/></g>')


# ---------------------------------------------------------------- scene

css = f"""
text {{ white-space: pre; }}
.a {{ animation-duration: {T}s; animation-iteration-count: infinite; animation-timing-function: linear; animation-fill-mode: both; }}
.now {{ animation: now 1s ease-in-out infinite alternate; }}
@keyframes now {{ to {{ fill: #6e3b40; }} }}
@media (prefers-reduced-motion: reduce) {{
  .a, .now {{ animation: none !important; }}
  #cursor, .ripple {{ display: none; }}
}}
"""

add(f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" width="{W}" height="{H}" font-family="{FONT}" xml:space="preserve" role="img" aria-label="A floating waybar whose purple clock says free until 15:00, then names the next meeting: Standup in 27m. A click opens the Timetable drawer out of the clock: Chicago, EET and GMT side by side, Standup next with Join on workspace 2 and Copy link, the day strip with its events, dashed free gaps and a red now line, and a focus block sized to the gap before Standup. Starting it turns the clock green, counting down from 25:00. Later the clock goes amber at Standup in 2m, then red with Standup · Join at 15:00, and a middle-click switches to the meeting on workspace 2.">')
STYLE_AT = len(out)
add("")  # the <style> goes here once every keyframe exists
add(f"""<defs>
<linearGradient id="wall" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{BG0}"/><stop offset="1" stop-color="{BG1}"/></linearGradient>
<radialGradient id="haze1" cx=".85" cy="0" r=".75"><stop offset="0" stop-color="{FROST}" stop-opacity=".13"/><stop offset="1" stop-color="{FROST}" stop-opacity="0"/></radialGradient>
<radialGradient id="haze2" cx=".05" cy="1" r=".6"><stop offset="0" stop-color="{CLOCK}" stop-opacity=".09"/><stop offset="1" stop-color="{CLOCK}" stop-opacity="0"/></radialGradient>
<filter id="shadow" x="-20%" y="-20%" width="140%" height="150%"><feDropShadow dx="0" dy="18" stdDeviation="22" flood-color="#000" flood-opacity=".65"/></filter>
<clipPath id="roll"><rect x="{TT_R - W_MAX:.1f}" y="{BAR_Y + 2}" width="{W_MAX:.1f}" height="{BAR_H - 4}"/></clipPath>
<clipPath id="screen"><rect width="{W}" height="{H}" rx="18"/></clipPath>
</defs>""")
add('<g clip-path="url(#screen)">')
add(f'<rect width="{W}" height="{H}" fill="url(#wall)"/><rect width="{W}" height="{H}" fill="url(#haze1)"/><rect width="{W}" height="{H}" fill="url(#haze2)"/>')


def window(x, y, w, h, title=""):
    add(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="12" fill="#1a1d22" stroke="#2a2e35"/>')
    add(f'<rect x="{x}" y="{y}" width="{w}" height="30" rx="12" fill="#1f2328"/>')
    for k, c in enumerate([STOP, CAUTION, GO]):
        add(f'<circle cx="{x + 18 + k * 16}" cy="{y + 15}" r="5" fill="{c}" fill-opacity=".55"/>')
    if title:
        text(x + w / 2, y + 20, title, 12, DIM, anchor="middle")


# Workspace 1: the editor you were in. Hyprland slides it away on Join, and
# the loop slides it back.
ws_out = anim("transform", [
    (0, "none"), (SWITCH, "none"), (SWITCH + 0.4, f"translateX(-{W}px)"),
    (RESET, f"translateX(-{W}px)"), (RESET + 0.4, "none"), (T, "none"),
])
add(f'<g {style(ws_out, "animation-timing-function:cubic-bezier(.3,0,.2,1)")}>')
window(36, 36, 600, 360)
lines = [(0, 210), (0, 300), (24, 260), (48, 330), (48, 180), (24, 120), (0, 60), (0, 0), (0, 240), (24, 350), (24, 200), (0, 90)]
cols = ["#4c566a", "#5e81ac99", "#434c5e", "#88c0d055", "#4c566a", "#434c5e", "#4c566a", "", "#5e81ac99", "#88c0d055", "#434c5e", "#4c566a"]
for i, ((indent, lw), col) in enumerate(zip(lines, cols)):
    if lw:
        add(f'<rect x="{58 + indent}" y="{86 + i * 24}" width="{lw}" height="9" rx="4" fill="{col}"/>')
window(220, 330, 470, 250)
for i, lw in enumerate([300, 220, 340, 180, 260, 140]):
    add(f'<rect x="{242 + (i % 3) * 12}" y="{380 + i * 26}" width="{lw}" height="9" rx="4" fill="#3b4252" opacity=".8"/>')
add("</g>")

# Workspace 2: work chat and the meeting Join opened. All names made up.
ws_in = anim("transform", [
    (0, f"translateX({W}px)"), (SWITCH, f"translateX({W}px)"), (SWITCH + 0.4, "none"),
    (RESET, "none"), (RESET + 0.4, f"translateX({W}px)"), (T, f"translateX({W}px)"),
])
add(f'<g {style(ws_in, "animation-timing-function:cubic-bezier(.3,0,.2,1)")} transform="translate({W},0)">')
ZX, ZY, ZW, ZH = 36, 36, 760, 560
window(ZX, ZY, ZW, ZH, "Standup")
people = [("AV", "Avery", "#5e81ac"), ("JO", "Jordan", "#b48ead"), ("SM", "Sam", "#a3be8c"), ("YO", "You", "#d08770")]
tone = {name: col for _, name, col in people}
tw, th = (ZW - 36) / 2, (ZH - 30 - 72 - 12) / 2
for k, (ini, name, col) in enumerate(people):
    x, y = ZX + 12 + (k % 2) * (tw + 12), ZY + 42 + (k // 2) * (th + 12)
    add(f'<rect x="{x:.1f}" y="{y:.1f}" width="{tw:.1f}" height="{th:.1f}" rx="10" fill="#15171a" stroke="{GO if k == 0 else "#2a2e35"}" stroke-width="{2 if k == 0 else 1}"/>')
    add(f'<circle cx="{x + tw / 2:.1f}" cy="{y + th / 2 - 8:.1f}" r="34" fill="{col}" fill-opacity=".85"/>')
    text(x + tw / 2, y + th / 2 + 1, ini, 22, DARK, 700, "middle")
    text(x + 14, y + th - 14, name, 12, FG)
bx = ZX + ZW / 2 - 170
for lbl, col in [("Mute", None), ("Video", None), ("Share", None), ("Leave", STOP)]:
    w = button(bx, ZY + ZH - 46, lbl, col, DARK if col else FG)
    bx += w + 10 + 28
window(812, 36, 352, 560, "#team")
for i, (who, lw) in enumerate([("Avery", 230), ("Sam", 180), ("Jordan", 260), ("Avery", 140), ("Sam", 210)]):
    y = 92 + i * 92
    add(f'<circle cx="{842}" cy="{y}" r="12" fill="{tone[who]}" fill-opacity=".7"/>')
    text(864, y - 2, who, 12, FG, 700)
    add(f'<rect x="864" y="{y + 10}" width="{lw}" height="8" rx="4" fill="#3b4252"/>')
    add(f'<rect x="864" y="{y + 26}" width="{lw - 70}" height="8" rx="4" fill="#3b4252"/>')
add("</g>")

# ---------------------------------------------------------------- the bar

# Left: workspaces and the window title.
WS_X, WS_W = 6, 6 * 26 + 8
pill(WS_X, WS_W)
ws_move = anim("transform", [(0, "none"), (SWITCH, "none"), (SWITCH + 0.25, "translateX(26px)"), (RESET, "translateX(26px)"), (RESET + 0.25, "none"), (T, "none")])
add(f'<rect x="{WS_X + 4}" y="{BAR_Y + 5}" width="26" height="{BAR_H - 10}" rx="7" fill="{PILL_ON}" {style(ws_move)}/>')
away, here = [(0, SWITCH + 0.1), (RESET + 0.1, T)], [(SWITCH + 0.1, RESET + 0.1)]
for i in range(6):
    x = WS_X + 4 + i * 26 + 13
    if i > 1:
        text(x, MID, str(i + 1), 14, BAR_FG, 400, "middle")
        continue
    # 1 is active until Join switches to 2.
    for spans, active in [(away, i == 0), (here, i == 1)]:
        group(fade(spans, 0.15), 1 if spans is away else 0)
        text(x, MID, str(i + 1), 14, BAR_BRIGHT if active else BAR_FG, 700 if active else 400, "middle")
        add("</g>")

WIN_X = WS_X + WS_W + 6
for title, spans, base in [("nvim  src/cart.ts", [(0, SWITCH + 0.05), (RESET + 0.2, T)], 1), ("Zoom Meeting", [(SWITCH + 0.2, RESET + 0.05)], 0)]:
    group(fade(spans, 0.12), base)
    pill(WIN_X, len(title) * CH + 24)
    text(WIN_X + 12, MID, title, 14, BAR_BRIGHT)
    add("</g>")

# Right: the tray, the clock, then the pills the clock pushes along.
TRAY_X = W - 6 - 86
pill(TRAY_X, 86)
for k, c in enumerate(["#81a1c1", CLOCK, "#5e81ac"]):
    add(f'<rect x="{TRAY_X + 14 + k * 22}" y="{BAR_Y + 10}" width="14" height="14" rx="4" fill="{c}" fill-opacity=".8"/>')

push = anim("transform", width_pts(shift))
add(f'<g {style(push)}>')
x = TT_R - W_STILL - 6
DESK_W, AUDIO_W, LAMPS_W = 62, (7 + 9) * CH + 16 + 24, 4 * 30 + 12
x -= DESK_W
pill(x, DESK_W)
mx, my = x + 20, BAR_Y + 17
add(f'<g fill="none" stroke="{BAR_DIM}" stroke-width="1.6" stroke-linecap="round"><rect x="{mx - 3}" y="{my - 8}" width="6" height="10" rx="3"/><path d="M{mx - 6},{my - 1} a6,6 0 0 0 12,0 M{mx},{my + 5} v3"/><path d="M{x + 46},{my - 5} l-5,5 l5,5"/></g>')
x -= 6 + AUDIO_W
pill(x, AUDIO_W)
text(x + 12, MID, "vol 35%", 14, FROST)
text(x + 12 + 7 * CH + 16, MID, "mouse 80%", 14, BAR_DIM)
x -= 6 + LAMPS_W
pill(x, LAMPS_W)
lx = x + 6 + 15
add(f'<circle cx="{lx}" cy="{BAR_Y + 17}" r="5.5" fill="{GO}"/>')
lx += 30
add(f'<rect x="{lx - 5}" y="{BAR_Y + 8}" width="10" height="18" rx="4" fill="none" stroke="{FROST}" stroke-width="1.4"/>')
for dy in (-5, 0, 5):
    add(f'<circle cx="{lx}" cy="{BAR_Y + 17 + dy}" r="1.8" fill="{FROST}"/>')
lx += 30
add(f'<circle cx="{lx}" cy="{BAR_Y + 17}" r="5.5" fill="none" stroke="{BAR_DIM}" stroke-width="1.6"/>')
lx += 30
add(f'<path d="M{lx - 5},{BAR_Y + 21} h10 l-1.6,-2 v-4 a3.4,3.4 0 0 0 -6.8,0 v4 z M{lx - 1.5},{BAR_Y + 23} h3" fill="none" stroke="{BAR_DIM}" stroke-width="1.5" stroke-linejoin="round"/>')
add("</g>")

# The clock's pill grows and shrinks from its left edge: two rounded caps
# and a middle that stretches between them.
cap_l = anim("transform", width_pts(lambda w: f"translateX({W_MAX - w:.1f}px)"))
add(f'<rect x="{TT_R - 40}" y="{BAR_Y + 0.5}" width="40" height="{BAR_H - 1}" rx="10" fill="{PILL}" stroke="{PILL_LINE}"/>')
add(f'<rect x="{TT_R - W_MAX:.1f}" y="{BAR_Y + 0.5}" width="40" height="{BAR_H - 1}" rx="10" fill="{PILL}" stroke="{PILL_LINE}" {style(cap_l)} transform="translate({W_MAX - W_STILL:.1f},0)"/>')
span = W_MAX - 40
stretch = anim("transform", width_pts(lambda w: f"scaleX({(w - 40) / span:.4f})"))
add(f'<g {style(stretch, "transform-box:fill-box;transform-origin:right center")} transform="translate({TT_R - 20:.1f},0) scale({(W_STILL - 40) / span:.4f},1) translate({-(TT_R - 20):.1f},0)">')
add(f'<rect x="{TT_R - 20 - span:.1f}" y="{BAR_Y + 1}" width="{span:.1f}" height="{BAR_H - 2}" fill="{PILL}"/>')
for y in (BAR_Y, BAR_Y + BAR_H - 1):
    add(f'<rect x="{TT_R - 20 - span:.1f}" y="{y}" width="{span:.1f}" height="1" fill="{PILL_LINE}"/>')
add("</g>")
# Hovered while the cursor is on it.
k = fade([(CLICK_CLOCK - 0.12, 3.5), (CLICK_JOIN - 0.12, CLICK_JOIN + 0.7)], 0.12)
add(f'<rect x="{TT_R - W_STILL + 3:.1f}" y="{BAR_Y + 4}" width="{W_STILL - 6:.1f}" height="{BAR_H - 8}" rx="7" fill="{FROST}" fill-opacity=".18" {style(k)} opacity="0"/>')


# The module's text, one frame at a time.
add('<g clip-path="url(#roll)">')
for i, (t0, kind, event, color, clock, how) in enumerate(FRAMES[:-1]):
    t1, leave = FRAMES[i + 1][0], FRAMES[i + 1][5]
    spans = [(t0, t1, how, leave)]
    if i == 0:  # the loop comes back to it
        spans.append((RESET, T, "flip", "flip"))
    x = TT_R - WIDTHS[i] + 12
    group(board(spans), 1 if i == STILL else 0)
    if kind:
        icon(kind, x, color, "now" if kind == "now" else "")
        x += 19
    cls = ' class="now"' if kind == "now" else ""
    add(f'<text x="{x:.1f}" y="{MID}" font-size="14" fill="{color}"><tspan{cls}>{esc(event)}</tspan><tspan fill="{CLOCK}">  Tue Nov 10  {clock}</tspan></text>')
    add("</g>")
add("</g>")

# ---------------------------------------------------------------- the drawer

# Opens from where the clock was clicked, clamped to the screen.
CLOCK_X, CLOCK_Y = TT_R - W_STILL / 2, BAR_Y + BAR_H / 2
DW, DH = 440, 546
DX, DY = min(max(CLOCK_X - DW / 2, 18), W - DW - 12), BAR_Y - 8 - DH
P = 14  # panel padding
IW = DW - 2 * P

spring = anim("transform", [
    (0, "translateY(26px) scale(.06)"), (OPEN - 0.01, "translateY(26px) scale(.06)"),
    (OPEN + 0.10, "translateY(14px) scale(.5)"), (OPEN + 0.20, "translateY(4px) scale(.9)"),
    (OPEN + 0.30, "translateY(-1px) scale(1.035)"), (OPEN + 0.42, "translateY(0) scale(.988)"),
    (OPEN + 0.56, "scale(1.006)"), (OPEN + 0.72, "scale(1)"), (CLICK_OUT + 0.08, "scale(1)"),
    (CLOSED, "translateY(14px) scale(.4)"), (T, "translateY(14px) scale(.4)"),
])
show = anim("opacity", [(0, 0), (OPEN - 0.01, 0), (OPEN + 0.12, 0.85), (OPEN + 0.2, 1), (CLICK_OUT + 0.08, 1), (CLOSED, 0), (T, 0)])
add(f'<g class="a" style="animation-name:{spring},{show};transform-origin:{CLOCK_X:.1f}px {CLOCK_Y:.1f}px"><g transform="translate({DX:.1f},{DY:.1f})">')
add(f'<rect width="{DW}" height="{DH}" rx="16" fill="{PANEL}" stroke="{PANEL_LINE}" filter="url(#shadow)"/>')

FOCUSED = CLICK_FOCUS + 0.05
before, after = [(0, FOCUSED)], [(FOCUSED + 0.12, T)]  # out, then in


def card(x, y, w, h, tint=None):
    if tint:
        add(f'<rect x="{x:.1f}" y="{y}" width="{w:.1f}" height="{h}" rx="10" fill="{tint}" fill-opacity=".07" stroke="{tint}" stroke-opacity=".45"/>')
    else:
        add(f'<rect x="{x:.1f}" y="{y}" width="{w:.1f}" height="{h}" rx="10" fill="{TILE}" stroke="{TILE_LINE}"/>')


# Header: the lamp and the chip follow the state.
add(f'<g {style(rise(0.1))}>')
for spans, color, chip_text, base in [(before, FROST, "next 27m", 1), (after, GO, "focus", 0)]:
    group(fade(spans, 0.12), base)
    add(f'<circle cx="{P + 5}" cy="31" r="4.5" fill="{color}"/>')
    chip(DW - P, 31, chip_text, color)
    add("</g>")
text(P + 20, 28, "TIMETABLE", 12, FG, 700, attrs='letter-spacing="2"')
text(P + 20, 45, "Tue Nov 10 · 5 events · refreshed 2 min ago", 11, DIM)
add("</g>")

# Zones: Chicago is local (purple); the others say how far ahead they are.
y = 57
zw = (IW - 12) / 3
for k, (name, meta, hhmm) in enumerate([("CHICAGO", "CST", "14:33"), ("EET", "+8h", "22:33"), ("GMT", "+6h", "20:33")]):
    x = P + k * (zw + 6)
    add(f'<g {style(rise(0.18 + k * 0.06))}>')
    card(x, y, zw, 54)
    label(x + 10, y + 18, name)
    label(x + zw - 10, y + 18, meta, anchor="end")
    text(x + 10, y + 44, hhmm, 20, CLOCK if k == 0 else FG, 700)
    add("</g>")

# Next: Standup, with Join and Copy link.
y = 122
add(f'<g {style(rise(0.36))}>')
card(P, y, IW, 106)
label(P + 10, y + 19, "NEXT")
label(DW - P - 10, y + 19, "ZOOM", anchor="end")
text(P + 10, y + 41, "Standup", 15, FG, 700)
text(P + 10, y + 60, "15:00 to 15:15 · in 27m", 11.5, DIM)
bx = P + 10 + button(P + 10, y + 70, "Join on workspace 2") + 6
button(bx, y + 70, "Copy link")
add("</g>")

# Today: the strip from 08:00 to 18:00, events, free gaps and a now line.
y = 240
SX, SY, SW = P, y + 16, IW
add(f'<g {style(rise(0.46))}>')
label(P, y + 9, "TODAY")
label(DW - P, y + 9, "08:00 TO 18:00 · 6H 45M FREE", anchor="end")
add(f'<rect x="{SX}" y="{SY}" width="{SW}" height="58" rx="8" fill="{WELL}"/>')
for h in range(8, 19, 2):
    tx = min(max((h - 8) / 10 * SW - 3, 1), SW - 12)
    text(SX + tx, SY + 49, str(h), 9, DIM)
add("</g>")


def sx(h):
    return SX + (h - 8) / 10 * SW


EVENTS = [  # (from, to, title, its class) at 14:33
    (9.5, 10, "Planning", "past"), (11, 12, "Design sync", "past"), (13, 13.5, "Code review", "past"),
    (15, 15.25, "Standup", "next"), (16, 17, "Sprint review", ""),
]
GAPS = [(8, 9.5), (10, 11), (12, 13), (13.5, 15), (15.25, 16), (17, 18)]
FOCUS_GAP = 3
for k, (a, b) in enumerate(GAPS):
    gx, gw = sx(a) + 1, sx(b) - sx(a) - 2
    group(fade([(OPEN + 0.95 + k * 0.05, T)], 0.25), 1)
    add(f'<rect x="{gx:.1f}" y="{SY + 6}" width="{gw:.1f}" height="28" rx="4" fill="none" stroke="{DIM}" stroke-opacity=".7" stroke-dasharray="3 2"/>')
    add("</g>")
    if k == FOCUS_GAP:
        group(fade(after, 0.25), 0)
        add(f'<rect x="{gx:.1f}" y="{SY + 6}" width="{gw:.1f}" height="28" rx="4" fill="{GO}" fill-opacity=".1" stroke="{GO}" stroke-dasharray="3 2"/>')
        add("</g>")
for k, (a, b, title, cls) in enumerate(EVENTS):
    ex, ew = sx(a), sx(b) - sx(a) - 1
    fill, op, color, weight = {"past": (FROST, ".14", DIM, 400), "next": (CAUTION, "1", WELL, 700)}.get(cls, (FROST, ".32", FG, 400))
    grow = keys([(0, "opacity: 0; transform: scaleY(.2)"), (OPEN + 0.55 + k * 0.07, "opacity: 0; transform: scaleY(.2)"),
                 (OPEN + 0.8 + k * 0.07, "opacity: 1; transform: none"), (T, "opacity: 1; transform: none")])
    add(f'<g {style(grow, "transform-box:fill-box;transform-origin:center")}>')
    add(f'<rect x="{ex:.1f}" y="{SY + 6}" width="{ew:.1f}" height="28" rx="4" fill="{fill}" fill-opacity="{op}"/>')
    chars = int((ew - 8) / 6)
    if chars >= 2:
        shown = title if len(title) <= chars else title[: chars - 1] + "…"
        text(ex + 4, SY + 24, shown, 9.5, color, weight)
    add("</g>")
drop = keys([(0, "opacity: 0; transform: translateY(-10px)"), (OPEN + 1.1, "opacity: 0; transform: translateY(-10px)"),
             (OPEN + 1.35, "opacity: 1; transform: none"), (T, "opacity: 1; transform: none")])
add(f'<rect x="{sx(14 + 33 / 60) - 1:.1f}" y="{SY + 2}" width="2" height="36" fill="{STOP}" {style(drop)}/>')

# The rest of today, from the one just finished.
add(f'<g {style(rise(0.62))}>')
for k, (when, title, kind, past) in enumerate([
    ("13:00 to 13:30", "Code review", "", True),
    ("15:00 to 15:15", "Standup", "Zoom", False),
    ("16:00 to 17:00", "Sprint review", "Google Meet", False),
]):
    ry = SY + 76 + k * 19
    text(P, ry, when, 12, IDLE if past else DIM)
    text(P + 14 * 7.2 + 10, ry, title, 12, IDLE if past else FG)
    text(DW - P, ry, kind, 11, DIM, anchor="end")
add("</g>")

# Focus: what a block would be, then the block counting down.
y = 386
add(f'<g {style(rise(0.72))}>')
group(fade(before, 0.12), 1)
card(P, y, IW, 108)
label(P + 10, y + 19, "FOCUS")
label(DW - P - 10, y + 19, "25 MIN", anchor="end")
text(P + 10, y + 40, "Free until 15:00: 25 min of focus, warned at", 12.5, FG)
text(P + 10, y + 57, "14:58.", 12.5, FG)
button(P + 10, y + 70, "Start focus block")
add("</g>")
group(fade(after, 0.12), 0)
card(P, y, IW, 108, GO)
label(P + 10, y + 19, "FOCUS", GO)
label(DW - P - 10, y + 19, "UNTIL 14:58", anchor="end")
for spans, count in [((FOCUSED + 0.12, TICKS[0]), "25:00"), ((TICKS[0], TICKS[1]), "24:59"), ((TICKS[1], TICKS[2]), "24:58"), ((TICKS[2], T), "24:57")]:
    group(fade([spans], 0.06), 1 if count == "25:00" else 0)
    text(P + 10, y + 44, count, 20, GO, 700)
    add("</g>")
text(P + 10, y + 62, "Started at 14:33. Ends 2 min before Standup.", 12.5, FG)
button(P + 10, y + 72, "Stop", STOP, WELL)
add("</g>")
add("</g>")

# Footer: the keys.
add(f'<g {style(rise(0.8))}>')
text(P, DH - 31, "Esc close · j join · f focus · r refresh ·", 10.5, DIM)
text(P, DH - 17, "right-click the clock for the calendar", 10.5, DIM)
add("</g>")
add("</g></g>")

# ---------------------------------------------------------------- cursor
focus_cx, focus_cy = DX + P + 10 + (17 * 7.2 + 22) / 2, DY + 386 + 83
OUT_X, OUT_Y = 420, 260
REST_X, REST_Y = 640, 380
clicks = [(CLICK_CLOCK, CLOCK_X, CLOCK_Y), (CLICK_FOCUS, focus_cx, focus_cy), (CLICK_OUT, OUT_X, OUT_Y), (CLICK_JOIN, CLOCK_X, CLOCK_Y)]
for t, cx_, cy_ in clicks:
    k = anim("opacity", [(0, 0), (t - 0.01, 0), (t, 0.9), (t + 0.45, 0), (T, 0)])
    k2 = anim("transform", [(0, "scale(.3)"), (t, "scale(.3)"), (t + 0.45, "scale(1.6)"), (T, "scale(1.6)")])
    color = STOP if t == CLICK_JOIN else FROST
    add(f'<circle class="a ripple" style="animation-name:{k},{k2};transform-box:fill-box;transform-origin:center" cx="{cx_:.1f}" cy="{cy_:.1f}" r="18" fill="none" stroke="{color}" stroke-width="2.5" opacity="0"/>')


def tr(x, y):
    return f"translate({x - 3:.1f}px, {y - 4:.1f}px)"


cur = anim("transform", [
    (0, tr(REST_X, REST_Y)), (1.9, tr(REST_X, REST_Y)), (CLICK_CLOCK - 0.15, tr(CLOCK_X, CLOCK_Y)), (3.4, tr(CLOCK_X, CLOCK_Y)),
    (4.3, tr(DX + 300, DY + 300)), (4.6, tr(DX + 300, DY + 300)),
    (CLICK_FOCUS - 0.15, tr(focus_cx, focus_cy)), (6.6, tr(focus_cx, focus_cy)),
    (CLICK_OUT - 0.15, tr(OUT_X, OUT_Y)), (CLICK_JOIN - 0.9, tr(OUT_X, OUT_Y)),
    (CLICK_JOIN - 0.15, tr(CLOCK_X, CLOCK_Y)), (CLICK_JOIN + 0.7, tr(CLOCK_X, CLOCK_Y)),
    (RESET + 0.3, tr(REST_X, REST_Y)), (T, tr(REST_X, REST_Y)),
])
add(f'<g id="cursor" class="a" style="animation-name:{cur};animation-timing-function:cubic-bezier(.45,0,.25,1);transform:{tr(REST_X, REST_Y)}">')
add('<path d="M0,0 L0,22 L6,17 L10,26 L14,24 L10,15 L18,15 Z" fill="#eceff4" stroke="#1d2024" stroke-width="1.4" stroke-linejoin="round"/>')
# Join is a middle-click.
k = fade([(CLICK_JOIN - 0.5, CLICK_JOIN + 0.5)], 0.15)
add(f'<g {style(k)} opacity="0"><rect x="-58" y="-34" width="96" height="22" rx="6" fill="{PANEL}" stroke="{PANEL_LINE}"/>')
text(-10, -19, "middle-click", 11.5, FG, anchor="middle")
add("</g></g>")
add("</g></svg>")

out[STYLE_AT] = "<style>" + css + "\n".join(kfs) + "</style>"
Path(__file__).with_name("hero.svg").write_text("\n".join(out) + "\n")
print(f"wrote hero.svg ({sum(len(s) + 1 for s in out) // 1024} KB, {len(kfs)} keyframe sets)")
