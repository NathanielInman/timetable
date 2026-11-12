#!/usr/bin/env python3
"""Writes docs/assets/hero.svg: the README's animated hero.

A loop on the same desktop as the siblings' heroes, under the floating
Platform bar (dot-files packages/waybar: config.jsonc and style.css). The
clock is calm until the hour before Standup, when it names it. A click
springs the drawer out of the clock: three zones, the next meeting with
Join, the day strip with its free gaps and a now line, and a focus block
sized to the gap. A look at tomorrow and back, then the block starts and
turns the clock into a countdown, and Standup on the strip outlines under
the pointer to say a click opens it in Google Calendar. Later the clock goes
amber, then red with Join at start time, and a middle-click switches to the
meeting's workspace, where the joined meeting counts down calmly.

The drawer is drawn by `drawer()` in its own coordinates, from a `Board`
(the time, a running focus block, what Join opened, the day shown), so
dot-files' docs/bar/scenes.py can port it as it ports Roundhouse's drawer.

Every event, name and time is made up.

Pure CSS animation inside the SVG, so GitHub plays it in an <img>. With
reduced motion it is a still of the open drawer.

    python3 docs/assets/hero.py
"""

from dataclasses import dataclass
from pathlib import Path

# Nord, matching the Platform waybar and the siblings' heroes.
BG0, BG1 = "#14171c", "#0d0f12"
PILL, PILL_LINE = "rgba(40,42,46,.84)", "rgba(76,86,106,.55)"
HOVER, WS_ON = "rgba(129,161,193,.18)", "rgba(129,161,193,.26)"
BAR_FG, BRIGHT, BAR_DIM = "#c5c8c6", "#d8dee9", "#616e88"
PANEL, PANEL_LINE = "#1e2024", "#41464e"
TILE, TILE_LINE, WELL = "#26282c", "#313338", "#15171a"
FG, DIM, IDLE = "#c5c8c6", "#7c838e", "#555b64"
FROST, GO, CAUTION, STOP, CLOCK, DARK = "#88c0d0", "#a3be8c", "#ebcb8b", "#bf616a", "#b48ead", "#1d2024"
FONT = "JetBrains Mono, DejaVu Sans Mono, Menlo, Consolas, monospace"


def esc(s):
    return s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def svg_text(x, y, s, size=13, fill=FG, weight=400, anchor="start", extra=""):
    extra = f" {extra}" if extra else ""
    return (f'<text x="{x:.1f}" y="{y:.1f}" font-size="{size}" fill="{fill}" font-weight="{weight}" '
            f'text-anchor="{anchor}"{extra}>{esc(s)}</text>')


def advance(s, size, spacing=0.0):
    """Width of monospace text: every character is 0.6 em."""
    return len(s) * (size * 0.6 + spacing)


# ================================================================ the drawer
#
# Self-contained: drawer(), drawer_size() and drawer_spots() need only the
# palette above, the made-up calendar below and a Board, and draw at (0, 0)
# in the drawer's own pixels. The caller puts DRAWER_DEFS in its <defs>.
# Layout follows crates/timetable-board/src/drawer.rs and style.css: the
# strip view, waybar's theme, drawer.width 0.6 of a 1200 px monitor.

DRAWER_W = 720
DRAWER_DEFS = (
    '<filter id="tt-shadow" x="-20%" y="-20%" width="140%" height="150%">'
    '<feDropShadow dx="0" dy="18" stdDeviation="22" flood-color="#000" flood-opacity=".65"/></filter>'
    '<pattern id="tt-maybe" width="8" height="8" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">'
    f'<rect width="4" height="8" fill="{WELL}" fill-opacity=".38"/></pattern>'
)

DAY = 86400


def hm(h, m=0):
    return (h * 60 + m) * 60


@dataclass(frozen=True)
class Ev:
    """An event; start and end are seconds from Tue Nov 10's midnight."""
    title: str
    start: int
    end: int
    kind: str | None = None  # the meeting link's label
    own: bool = False  # your own time: never calls for you, counts as free
    answer: str | None = None  # "maybe" or "not answered"
    all_day: bool = False

    def running(self, now):
        return self.start <= now < self.end


def ev(day, a, b, title, kind=None, **kw):
    return Ev(title, day * DAY + a, day * DAY + b, kind, **kw)


EVENTS = [  # day 0 is Tue Nov 10, day 1 Wed Nov 11
    ev(0, hm(9, 30), hm(10), "Planning", "Google Meet"),
    ev(0, hm(11), hm(12), "Design sync", "Zoom"),
    ev(0, hm(12), hm(13), "Lunch", own=True),
    ev(0, hm(13), hm(13, 30), "Code review", "Google Meet"),
    ev(0, hm(15), hm(15, 15), "Standup", "Zoom"),
    ev(0, hm(16), hm(17), "Sprint review", "Google Meet"),
    ev(1, 0, DAY, "Release freeze", all_day=True),
    ev(1, hm(10), hm(11), "Roadmap review", "Google Meet", answer="not answered"),
    ev(1, hm(15), hm(15, 15), "Standup", "Zoom"),
    ev(1, hm(16), hm(16, 45), "Vendor demo", "Zoom", answer="maybe"),
]
DATES = {0: "Tue Nov 10", 1: "Wed Nov 11"}
ZONES = [("CHICAGO", None, 0), ("EET", "+8h", 8), ("GMT", "+6h", 6)]  # Chicago is local, CST
WORKSPACE, LEAD, MIN_GAP, FOCUS_MIN = "2", 2, 15, 5
CALENDAR_HINT = "right-click the clock for gnome-calendar"


@dataclass(frozen=True)
class Board:
    """What the drawer shows. `now` is seconds from Tue Nov 10's midnight;
    `focus` a running block's (start, end); `joined` the titles of today's
    meetings Join opened; `offset` the day shown, in days from today;
    `hover` the title of the strip event under the pointer."""
    now: int
    focus: tuple[int, int] | None = None
    joined: tuple[str, ...] = ()
    offset: int = 0
    hover: str | None = None


def hhmm(t):
    t %= DAY
    return f"{t // 3600:02d}:{t % 3600 // 60:02d}"


def minutes(m):
    m = max(m, 0)
    return f"{m // 60}h {m % 60:02d}m" if m >= 60 else f"{m}m"


def until(t, now):
    """Whole minutes until t, rounded up."""
    return (max(t - now, 0) + 59) // 60


def countdown(secs):
    s = max(secs, 0)
    return f"{s // 60:02d}:{s % 60:02d}"


def upcoming(now):
    return min((e for e in EVENTS if not e.all_day and not e.own and e.start > now), key=lambda e: e.start, default=None)


def status(b):
    """timetable_core::agenda::status: the state and its event."""
    now = b.now
    on = [e for e in EVENTS if not e.all_day and e.running(now)]
    cur = [e for e in on if not e.own]
    nxt = upcoming(now)

    def opened(e):
        return e.start < DAY and e.title in b.joined

    for c in cur:
        if now < c.start + 5 * 60 and not opened(c):
            return "now", c
    if b.focus and now < b.focus[1]:
        return "focus", None
    if nxt and nxt.start - now <= 10 * 60 and not opened(nxt):
        return "soon", nxt
    if cur:
        return "in", cur[0]
    if own := [e for e in on if e.own]:
        return "own", own[0]
    if nxt and nxt.start - now <= 60 * 60:
        return "next", nxt
    return "free", nxt


def busy(span, counts):
    v = sorted((max(e.start, span[0]), min(e.end, span[1])) for e in EVENTS
               if not e.all_day and counts(e) and e.end > span[0] and e.start < span[1])
    out = []
    for s, e in v:
        if out and s <= out[-1][1]:
            out[-1] = (out[-1][0], max(out[-1][1], e))
        else:
            out.append((s, e))
    return out


def gaps(span, min_gap, own_is_free=True):
    out, at = [], span[0]
    for s, e in busy(span, (lambda e: not e.own) if own_is_free else (lambda e: True)):
        if s - at >= min_gap * 60:
            out.append((at, s))
        at = max(at, e)
    if span[1] - at >= min_gap * 60:
        out.append((at, span[1]))
    return out


def lanes(timed):
    """Side-by-side lanes for overlapping events: (lane, lanes in its group)."""
    out, ends, group_from, group_end = [], [], 0, -1
    for i, e in enumerate(timed):
        if e.start >= group_end:
            for k in range(group_from, i):
                out[k] = (out[k][0], max(len(ends), 1))
            ends, group_from = [], i
        lane = next((k for k, x in enumerate(ends) if x <= e.start), None)
        if lane is None:
            ends.append(e.end)
            lane = len(ends) - 1
        else:
            ends[lane] = e.end
        group_end = max(group_end, e.end)
        out.append((lane, 0))
    for k in range(group_from, len(out)):
        out[k] = (out[k][0], max(len(ends), 1))
    return out


def wrap(s, chars):
    lines, line = [], ""
    for word in s.split(" "):
        if line and len(line) + 1 + len(word) > chars:
            lines.append(line)
            line = word
        else:
            line = f"{line} {word}" if line else word
    return lines + [line]


def fit(s, room, size):
    """Ellipsized to `room` pixels, as GTK's EllipsizeMode::End."""
    n = int(room / (size * 0.6))
    if len(s) <= n:
        return s
    return s[: n - 1].rstrip() + "…" if n >= 2 else ""


STATE_COLOR = {"free": GO, "next": FROST, "in": FROST, "soon": CAUTION, "now": STOP, "focus": GO, "own": GO}

P = 14  # the panel's padding
STRIP_TAG, STRIP_LANE, STRIP_TICK = 15, 24, 14
ROW_H = 19


def _card(x, y, w, h, tint=None, strength=(0.06, 0.45)):
    if tint:
        return (f'<rect x="{x:.1f}" y="{y:.1f}" width="{w:.1f}" height="{h}" rx="10" fill="{tint}" '
                f'fill-opacity="{strength[0]}" stroke="{tint}" stroke-opacity="{strength[1]}"/>')
    return f'<rect x="{x:.1f}" y="{y:.1f}" width="{w:.1f}" height="{h}" rx="10" fill="{TILE}" stroke="{TILE_LINE}"/>'


def _label(x, y, s, fill=DIM, anchor="start"):
    """The small bold caps, as .t-label."""
    return svg_text(x, y, s, 10, fill, 700, anchor, 'letter-spacing="1"')


def _button(x, y, s, kind=None, size=12, h=26, pad=11, disabled=False):
    """A .btns button; kind is "primary" or "danger". Returns (svg, width)."""
    w = advance(s, size) + 2 * pad + 2
    fill, color = {"primary": (FROST, WELL), "danger": (STOP, WELL)}.get(kind, (TILE, FG))
    line = fill if kind else TILE_LINE
    op = ' opacity=".45"' if disabled else ""
    out = (f'<g{op}><rect x="{x:.1f}" y="{y:.1f}" width="{w:.1f}" height="{h}" rx="8" fill="{fill}" stroke="{line}"/>'
           + svg_text(x + w / 2, y + h / 2 + size * 0.36, s, size, color, 700 if kind else 400, "middle") + "</g>")
    return out, w


def _render(b, w=DRAWER_W):
    """The drawer for `b`: (svg parts, height, spots). Parts are the panel,
    then each section, so a caller can reuse a part that did not change."""
    now, iw = b.now, w - 2 * P
    state, sev = status(b)
    nxt = upcoming(now)
    parts, spots = [], {}
    sec = []  # the section being drawn

    def flush():
        parts.append("".join(sec))
        sec.clear()

    # ---- header: the lamp, the date, the state's chip
    color = STATE_COLOR[state]
    lamp_op = ' fill-opacity=".7"' if state == "own" else ""
    sec.append(f'<circle cx="{P + 5}" cy="29" r="4.5" fill="{color}"{lamp_op}/>')
    sec.append(svg_text(P + 20, 27, "TIMETABLE", 12, FG, 700, extra='letter-spacing="2"'))
    today = [e for e in EVENTS if e.start < DAY and not e.all_day]
    sec.append(svg_text(P + 20, 42, f"{DATES[0]} · {len(today)} events · refreshed 2 min ago", 11, DIM))
    chip = {"free": "free", "now": "starting", "in": "in a meeting", "own": "own time", "focus": "focus"}.get(
        state, f"next {minutes(until(nxt.start, now)) if nxt else ''}")
    cw = advance(chip, 11) + 18
    sec.append(f'<rect x="{w - P - cw:.1f}" y="20" width="{cw:.1f}" height="19" rx="9.5" fill="{color}" fill-opacity=".15"/>')
    sec.append(svg_text(w - P - cw / 2, 33.5, chip, 11, color, 400 if state == "own" else 700, "middle"))
    flush()

    # ---- zones: Chicago is local and purple; the others say how far ahead
    y = 56
    zw = (iw - 12) / 3
    for k, (name, meta, ahead) in enumerate(ZONES):
        x = P + k * (zw + 6)
        sec.append(_card(x, y, zw, 54))
        sec.append(_label(x + 10, y + 19, name))
        sec.append(_label(x + zw - 10, y + 19, meta or "CST", anchor="end"))
        sec.append(svg_text(x + 10, y + 45, hhmm(now + ahead * 3600), 20, CLOCK if k == 0 else FG, 700))
    flush()

    # ---- next (or now): the meeting with Join and Copy link
    y = 122
    head, e = ("NOW", sev) if state in ("now", "in", "own") else ("NEXT", nxt)
    then = None
    if head == "NOW" and nxt and nxt is not e:
        then = f"Then {nxt.title} at {hhmm(nxt.start)}"
    h = 104 + (18 if then else 0)
    tint = {"soon": (CAUTION, (0.06, 0.45)), "now": (STOP, (0.07, 0.5)), "own": (GO, (0.04, 0.3))}.get(state)
    sec.append(_card(P, y, iw, h, *(tint or (None,))))
    head_color = {"soon": CAUTION, "now": STOP}.get(state, DIM)
    sec.append(_label(P + 10, y + 20, head, head_color))
    if e:
        kind = (e.kind or ("OWN TIME" if e.own else "NO LINK")).upper()
        sec.append(_label(w - P - 10, y + 20, kind, anchor="end"))
        sec.append(svg_text(P + 10, y + 42, e.title, 15, FG, 700))
        spots["title"] = (P + 10 + advance(e.title, 15) / 2, y + 37)
        rng = f"{hhmm(e.start)} to {hhmm(e.end)}"
        when = f"{rng} · {minutes(until(e.end, now))} left" if e.running(now) else f"{rng} · in {minutes(until(e.start, now))}"
        sec.append(svg_text(P + 10, y + 60, when, 11.5, DIM))
        if e.kind:
            urgent = state in ("soon", "now")
            svg, jw = _button(P + 10, y + 69, f"Join on workspace {WORKSPACE}", "primary" if urgent else None)
            sec.append(svg)
            spots["join"] = (P + 10 + jw / 2, y + 82)
            svg, cw = _button(P + 16 + jw, y + 69, "Copy link")
            sec.append(svg)
            spots["copy"] = (P + 16 + jw + cw / 2, y + 82)
        if then:
            sec.append(svg_text(P + 10, y + 113, then, 11, DIM))
    else:
        sec.append(svg_text(P + 10, y + 42, "Nothing else on the calendar", 15, FG, 700))
    flush()

    # ---- the day: a strip with the rest of it listed under it
    y += h + 12
    day = b.offset
    s0, s1 = day * DAY + hm(8), day * DAY + hm(18)
    on_day = [e for e in EVENTS if e.start < (day + 1) * DAY and e.end > day * DAY]
    timed = sorted((e for e in on_day if not e.all_day), key=lambda e: e.start)
    for e in timed:  # the configured span, widened to fit the day's events
        s0, s1 = min(s0, e.start - e.start % 3600), max(s1, e.end + (3600 - e.end % 3600) % 3600)
    # The day redraws once a minute (its signature holds the minute).
    now = now // 60 * 60
    ahead = (max(now, s0), max(s1, now))
    free = sum(b_ - a for a, b_ in gaps(ahead, MIN_GAP)) // 60
    span = f"{hhmm(s0)} TO {hhmm(s1)}"
    aside = {0: f"{span} · {minutes(free).upper()} FREE LEFT", 1: f"{span} · {minutes(free).upper()} FREE"}.get(
        (day > 0) - (day < 0), span)
    name = {0: "TODAY", 1: "TOMORROW", -1: "YESTERDAY"}.get(day, DATES.get(day, "").upper())

    # The row above the strip: ‹ › and the day, Today while away, the free
    # time, and the view switch.
    ry = y
    x = P
    for key, glyph in (("prev", "‹"), ("next_day", "›")):
        svg, bw = _button(x, ry, glyph, size=12, h=22, pad=8)
        sec.append(svg)
        spots[key] = (x + bw / 2, ry + 11)
        x += bw + 4
    sec.append(_label(x + 2, ry + 15, name))
    vw = advance("Timeline", 10.5) + 20
    svg, vw = _button(w - P - vw, ry, "Timeline", size=10.5, h=22, pad=9)
    sec.append(svg)
    spots["view"] = (w - P - vw / 2, ry + 11)
    ax = w - P - vw - 6
    sec.append(_label(ax, ry + 15, aside, anchor="end"))
    if day != 0:
        tw = advance("Today", 10.5) + 20
        tx = ax - advance(aside, 10, 1) - 6 - tw
        svg, tw = _button(tx, ry, "Today", size=10.5, h=22, pad=9)
        sec.append(svg)
        spots["today"] = (tx + tw / 2, ry + 11)

    # The strip.
    sy = ry + 28
    lane_of = lanes(timed)
    band = max((g for _, g in lane_of), default=1) * STRIP_LANE
    sh = STRIP_TAG + band + 4 + STRIP_TICK
    sec.append(f'<rect x="{P}" y="{sy}" width="{iw}" height="{sh}" rx="8" fill="{WELL}"/>')

    def fx(t):
        return P + (min(max(t, s0), s1) - s0) / (s1 - s0) * iw

    for t in range(s0 + (3600 - s0 % 3600) % 3600, s1 + 1, 3600):
        sec.append(f'<rect x="{fx(t) - 0.5:.1f}" y="{sy + STRIP_TAG - 2}" width="1" height="{band + 4}" fill="{DIM}" fill-opacity=".14"/>')
        sec.append(svg_text(fx(t), sy + STRIP_TAG + band + 3 + 10, str(t % DAY // 3600), 9, DIM, anchor="middle"))
    run = b.focus if b.focus and now < b.focus[1] else None
    for a, b_ in gaps(ahead, MIN_GAP, own_is_free=False):
        gx, gw = fx(a) + 1, fx(b_) - fx(a) - 2
        focused = run and run[0] < b_ and run[1] > a
        stroke = f'stroke="{GO}"' if focused else f'stroke="{DIM}" stroke-opacity=".7"'
        fill = f'fill="{GO}" fill-opacity=".1"' if focused else 'fill="none"'
        sec.append(f'<rect x="{gx:.1f}" y="{sy + STRIP_TAG + 0.5}" width="{gw:.1f}" height="{band - 1}" rx="4" {fill} {stroke} stroke-dasharray="3 2"/>')
        tag = fit(f"{minutes((b_ - a) // 60)} free", gw - 10, 9.5)
        if tag:
            sec.append(svg_text(gx + 5, sy + STRIP_TAG + band / 2 + 3.5, tag, 9.5, GO, extra='fill-opacity=".85"'))
    for e, (lane, group) in zip(timed, lane_of):
        a, b_ = max(e.start, s0), min(e.end, s1)
        if b_ <= a:
            continue
        lh = band / group
        ex, ew = fx(a), fx(b_) - fx(a) - 1
        ey, eh = sy + STRIP_TAG + lane * lh, lh - 2 if group > 1 else lh
        cls = "current" if e.running(now) else "past" if e.end <= now else "next" if e is nxt else ""
        if e.own:
            fill, op, color, weight = GO, {"current": ".32", "past": ".08"}.get(cls, ".16"), DIM if cls == "past" else FG, 400
        else:
            fill, op, color, weight = {
                "past": (FROST, ".14", DIM, 400), "next": (CAUTION, "1", WELL, 700), "current": (FROST, "1", WELL, 700),
            }.get(cls, (FROST, ".32", FG, 400))
        sec.append(f'<rect x="{ex:.1f}" y="{ey:.1f}" width="{ew:.1f}" height="{eh:.1f}" rx="4" fill="{fill}" fill-opacity="{op}"/>')
        if e.answer == "maybe":
            sec.append(f'<rect x="{ex:.1f}" y="{ey:.1f}" width="{ew:.1f}" height="{eh:.1f}" rx="4" fill="url(#tt-maybe)"/>')
        if e.answer == "not answered":
            sec.append(f'<rect x="{ex + 0.5:.1f}" y="{ey + 0.5:.1f}" width="{ew - 1:.1f}" height="{eh - 1:.1f}" rx="4" fill="none" stroke="{FG}" stroke-opacity=".75" stroke-dasharray="3 2"/>')
        if b.hover == e.title:
            sec.append(f'<rect x="{ex + 0.5:.1f}" y="{ey + 0.5:.1f}" width="{ew - 1:.1f}" height="{eh - 1:.1f}" rx="4" fill="none" stroke="{FG}" stroke-opacity=".55"/>')
        label = fit(e.title, ew - 10, 10)
        if label:
            italic = ' font-style="italic"' if e.answer else ""
            sec.append(svg_text(ex + 5, ey + eh / 2 + 3.5, label, 10, color, weight, extra=italic.strip()))
        spots[f"ev:{e.title}"] = (ex + ew / 2, ey + eh / 2)
    if s0 < now < s1:
        nx = fx(now)
        sec.append(f'<rect x="{nx - 1:.1f}" y="{sy + STRIP_TAG - 3}" width="2" height="{band + 6}" fill="{STOP}"/>')
        tw = advance("00:00", 9) + 6
        sec.append(f'<rect x="{nx - tw / 2:.1f}" y="{sy}" width="{tw:.1f}" height="13" rx="3" fill="{STOP}"/>')
        sec.append(svg_text(nx, sy + 10, hhmm(now), 9, WELL, 700, "middle"))

    # The rows: today what is left and the one just finished; another day
    # all of it, its all-day events first.
    rows = []
    if day != 0 and (all_day := [e.title for e in on_day if e.all_day]):
        rows.append(("all", "All day: " + " · ".join(all_day)))
    first = next((i for i, e in enumerate(timed) if e.end > now), len(timed)) if day == 0 else 0
    rows += [("ev", e) for e in timed[max(first - 1, 0):][: 7 if day == 0 else 12]]
    if not timed:
        rows.append(("all", "Nothing on the calendar {}.".format({0: "today", 1: "tomorrow"}.get(day, "that day"))))
    elif day == 0 and first == len(timed):
        rows.append(("all", "That was the last one today."))
    yy = sy + sh + 6
    for kind, r in rows:
        base = yy + 13.5
        if kind == "all":
            sec.append(svg_text(P, base, r, 13, DIM))
        else:
            past, cur = r.end <= now, r.running(now)
            sec.append(svg_text(P, base, f"{hhmm(r.start)} to {hhmm(r.end)}", 13, IDLE if past else DIM))
            color = IDLE if past else FROST if cur else GO if r.own else DIM if r.answer == "not answered" else FG
            extra = ('font-style="italic"' if r.answer else "") + (' fill-opacity=".85"' if r.own and not past else "")
            sec.append(svg_text(P + advance("00:00 to 00:00", 13) + 10, base, r.title, 13, color, 700 if cur else 400, extra=extra.strip()))
            note = "own time" if r.own else " · ".join(x for x in (r.answer, r.kind) if x)
            sec.append(svg_text(w - P, base, note, 11, DIM, anchor="end"))
            spots[f"row:{r.title}"] = (P + 140, yy + 9)
        yy += ROW_H
    flush()

    # ---- focus: what a block would be, or the block counting down
    now = b.now
    y = yy - 2 + 12
    if run:
        h = 112
        sec.append(_card(P, y, iw, h, GO, (0.06, 0.45)))
        sec.append(_label(P + 10, y + 20, "FOCUS", GO))
        sec.append(_label(w - P - 10, y + 20, f"UNTIL {hhmm(run[1])}", anchor="end"))
        sec.append(svg_text(P + 10, y + 47, countdown(run[1] - now), 20, GO, 700))
        after = upcoming(run[1] - 1)
        text = f"Started at {hhmm(run[0])}."
        if after and after.start - run[1] <= LEAD * 60 + 60:
            text = f"Started at {hhmm(run[0])}. Ends {(after.start - run[1]) // 60} min before {after.title}."
        sec.append(svg_text(P + 10, y + 67, text, 13, FG))
        svg, bw = _button(P + 10, y + 76, "Stop", "danger")
        sec.append(svg)
        spots["focus"] = (P + 10 + bw / 2, y + 89)
    else:
        nt = upcoming(now) if (upcoming(now) and upcoming(now).start < DAY) else None
        end = (nt.start - LEAD * 60) // 60 * 60 if nt else (now + 50 * 60 + 59) // 60 * 60
        ok = end - now >= FOCUS_MIN * 60
        mins = (end - now) // 60
        if ok:
            text = (f"Free until {hhmm(nt.start)}: {mins} min of focus, warned at {hhmm(end)}." if nt
                    else f"Nothing else today: {mins} min of focus, until {hhmm(end)}.")
        else:
            text = f"{nt.title} starts in {until(nt.start, now)} min, too soon for a focus block."
        if not ok or mins < 25:
            long = next(((a, b_) for a, b_ in gaps((max(now, hm(8)), max(hm(18), now)), 30) if a > now + 60), None)
            if long:
                text += f" Next long gap: {hhmm(long[0])} to {hhmm(long[1])} ({minutes((long[1] - long[0]) // 60)})."
        lines = wrap(text, int((iw - 20) / 7.8))
        h = 86 + 17 * (len(lines) - 1)
        sec.append(_card(P, y, iw, h))
        sec.append(_label(P + 10, y + 20, "FOCUS"))
        if ok:
            sec.append(_label(w - P - 10, y + 20, f"{mins} MIN", anchor="end"))
        for i, line in enumerate(lines):
            sec.append(svg_text(P + 10, y + 41 + 17 * i, line, 13, FG))
        svg, bw = _button(P + 10, y + h - 36, "Start focus block", disabled=not ok)
        sec.append(svg)
        spots["focus"] = (P + 10 + bw / 2, y + h - 23)
    flush()

    # ---- the keys
    y += h + 12
    foot = ("Esc close · j join · e edit · f focus · v view · ← → days · r refresh · " + CALENDAR_HINT)
    lines = wrap(foot, int(iw / advance("x", 10.5)))
    for i, line in enumerate(lines):
        sec.append(svg_text(P, y + 11 + 14 * i, line, 10.5, DIM))
    flush()
    height = y + 14 * len(lines) + P

    panel = f'<rect width="{w}" height="{height}" rx="16" fill="{PANEL}" stroke="{PANEL_LINE}" filter="url(#tt-shadow)"/>'
    return [panel] + parts, height, spots


def drawer(b, w=DRAWER_W):
    """The whole drawer for `b` as SVG, drawn at (0, 0) in its own pixels."""
    return "".join(_render(b, w)[0])


def drawer_parts(b, w=DRAWER_W):
    """The panel, then the header, zones, next, day, focus and keys."""
    return _render(b, w)[0]


def drawer_size(b, w=DRAWER_W):
    """(width, height): the drawer grows while a focus block counts down."""
    return w, _render(b, w)[1]


def drawer_spots(b, w=DRAWER_W):
    """Where a click lands in the drawer's own pixels: join, copy, title
    (opens the card's event in Google Calendar), prev, next_day, today
    (only while away from today), view, focus (start or stop), and
    ev:<title> and row:<title> for the day's events."""
    return _render(b, w)[2]


# ================================================================ the scene

W, H = 1200, 660
T = 17.4  # loop seconds

# The story, in seconds.
FLIPS = (1.4, 1.62, 1.84)  # the clock runs from 13:47 to 14:33
CLICK_CLOCK, OPEN = 2.9, 2.98
CLICK_DAY = 4.5  # ›: tomorrow
CLICK_TODAY = 5.8  # and back
CLICK_FOCUS = 6.9
TICKS = (7.9, 8.9, 9.9)
HOVER_ON, TIP_ON, HOVER_OFF = 8.15, 8.65, 9.75
CLICK_OUT, CLOSED = 10.25, 10.7
JUMP = (11.2, 11.42, 11.64)  # the focus block runs out, 14:41 to 14:58
STANDUP = 12.9
CLICK_JOIN, SWITCH = 14.0, 14.15
RESET = 16.4
LAG = 0.06  # a click reaches the daemon and comes back
STILL = 3.9  # the moment the reduced-motion still shows

out, kfs = [], []


def add(s):
    out.append(s)


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


def merge(spans):
    spans = sorted(spans)
    out_ = []
    for s, e in spans:
        if out_ and s <= out_[-1][1] + 1e-6:
            out_[-1] = (out_[-1][0], max(out_[-1][1], e))
        else:
            out_.append((s, e))
    return out_


def fade(spans, ramp=0.22):
    """Opacity keyframes visible during each (start, end) span."""
    pts = []
    for s, e in merge(spans):
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


def at(spans, t):
    return any(s <= t < e for s, e in spans)


def style(name, extra=""):
    return f'class="a" style="animation-name:{name};{extra}"'


def group(spans, ramp=0.04, extra=""):
    """A group shown during `spans`; its still state is STILL's."""
    add(f'<g {style(fade(spans, ramp), extra)} opacity="{1 if at(spans, STILL) else 0}">')


def text(*a, **kw):
    add(svg_text(*a, **kw))


SHOWN, BELOW, ABOVE, GONE = ("opacity: 1; transform: none", "opacity: 0; transform: translateY(14px)",
                             "opacity: 0; transform: translateY(-14px)", "opacity: 0; transform: none")


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
            pts += [(s, hidden), (s + (0.22 if enter == "flip" else 0.04), SHOWN)]
        if e >= T:
            pts.append((T, SHOWN))
        else:
            pts += [(e, SHOWN), (e + 0.14, ABOVE)] if leave == "flip" else [(e, SHOWN), (e + 0.04, GONE)]
    if pts[-1][0] < T:
        pts.append((T, BELOW))
    return keys(pts)


# ---------------------------------------------------------------- glyphs
# Drawn, not typed: the bar's Nerd Font glyphs would not survive an <img>.
# Each fits a 12 px box around (cx, cy).

def g_calendar(cx, cy, c):  # md-calendar: the next event
    return (f'<g fill="none" stroke="{c}" stroke-width="1.5" stroke-linecap="round"><rect x="{cx - 5.5}" y="{cy - 4.5}" width="11" height="10" rx="1.8"/>'
            f'<path d="M{cx - 5.5},{cy - 1} h11 M{cx - 2.8},{cy - 6.5} v3 M{cx + 2.8},{cy - 6.5} v3"/></g>')


def g_video(cx, cy, c):  # md-video: a meeting starting; fa-video-camera: Zoom
    return (f'<g fill="{c}"><rect x="{cx - 6.5}" y="{cy - 4.5}" width="9" height="9" rx="2"/>'
            f'<path d="M{cx + 3},{cy - 1} L{cx + 6.5},{cy - 3.8} V{cy + 3.8} L{cx + 3},{cy + 1} Z"/></g>')


def g_timer(cx, cy, c):  # md-timer-outline: a focus block
    return (f'<g fill="none" stroke="{c}" stroke-width="1.5" stroke-linecap="round"><circle cx="{cx}" cy="{cy + 1}" r="5.3"/>'
            f'<path d="M{cx},{cy + 1} v-3 M{cx - 2},{cy - 6} h4"/></g>')


def g_group(cx, cy, c):  # md-account-group: in a meeting
    return (f'<g fill="{c}"><circle cx="{cx}" cy="{cy - 2.5}" r="2.4"/><path d="M{cx - 4},{cy + 5} a4,3.6 0 0 1 8,0 Z"/>'
            f'<circle cx="{cx - 5}" cy="{cy - 1}" r="1.8" opacity=".75"/><circle cx="{cx + 5}" cy="{cy - 1}" r="1.8" opacity=".75"/>'
            f'<path d="M{cx - 8},{cy + 5} a3,2.8 0 0 1 4.5,-2.4 M{cx + 8},{cy + 5} a3,2.8 0 0 0 -4.5,-2.4" fill="none" stroke="{c}" stroke-width="1.4" opacity=".75"/></g>')


def g_puzzle(cx, cy, c):  # md-puzzle: Switchyard
    return (f'<path d="M{cx - 5.5},{cy - 3.5} h3 a2,2 0 1 1 3.5,0 h3 v3 a2,2 0 1 1 0,3.5 v3.5 h-9.5 Z" '
            f'fill="none" stroke="{c}" stroke-width="1.5" stroke-linejoin="round"/>')


def g_lan(cx, cy, c):  # md-lan-disconnect: the VPN, down
    return (f'<g fill="none" stroke="{c}" stroke-width="1.4"><rect x="{cx - 6}" y="{cy - 6}" width="5" height="4" rx=".8"/>'
            f'<rect x="{cx + 1}" y="{cy + 2}" width="5" height="4" rx=".8"/><path d="M{cx - 3.5},{cy - 2} v2.5 M{cx + 3.5},{cy + 2} v-2.5" stroke-linecap="round"/>'
            f'<path d="M{cx - 2.5},{cy + 2.5} l5,-5" stroke-width="1.2"/></g>')


def g_bell(cx, cy, c):  # fa-bell: do not disturb, off
    return (f'<g fill="{c}"><path d="M{cx - 5},{cy + 3} V{cy - 0.8} Q{cx - 5},{cy - 5.8} {cx},{cy - 5.8} Q{cx + 5},{cy - 5.8} {cx + 5},{cy - 0.8} '
            f'V{cy + 3} L{cx + 6.3},{cy + 4.4} H{cx - 6.3} Z"/><circle cx="{cx}" cy="{cy + 6}" r="1.6"/></g>')


def g_mic(cx, cy, c):  # md-microphone: Logbook
    return (f'<g fill="none" stroke="{c}" stroke-width="1.5" stroke-linecap="round"><rect x="{cx - 2.5}" y="{cy - 6.5}" width="5" height="8.5" rx="2.5" fill="{c}" stroke="none"/>'
            f'<path d="M{cx - 4.8},{cy - 1} a4.8,4.8 0 0 0 9.6,0 M{cx},{cy + 3.8} v2.7"/></g>')


def g_chevron(cx, cy, c):  # fa-chevron-left: the desk drawer's handle
    return f'<path d="M{cx + 2},{cy - 4.5} L{cx - 2.5},{cy} L{cx + 2},{cy + 4.5}" fill="none" stroke="{c}" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>'


def g_speaker(cx, cy, c):  # fa-volume-up
    return (f'<path d="M{cx - 6},{cy - 2.2} H{cx - 3.4} L{cx},{cy - 5.4} V{cy + 5.4} L{cx - 3.4},{cy + 2.2} H{cx - 6} Z" fill="{c}"/>'
            f'<path d="M{cx + 2.4},{cy - 2.6} Q{cx + 4.6},{cy} {cx + 2.4},{cy + 2.6} M{cx + 4.6},{cy - 4.8} Q{cx + 8.6},{cy} {cx + 4.6},{cy + 4.8}" '
            f'fill="none" stroke="{c}" stroke-width="1.4" stroke-linecap="round"/>')


def g_mouse(cx, cy, c):  # md-mouse
    return (f'<g fill="none" stroke="{c}" stroke-width="1.4"><rect x="{cx - 3.8}" y="{cy - 6}" width="7.6" height="12" rx="3.8"/>'
            f'<path d="M{cx},{cy - 6} v4.4 M{cx - 3.8},{cy - 1.6} h7.6"/></g>')


def g_vim(cx, cy, c):  # Neovide
    return (f'<g fill="none" stroke="{c}" stroke-width="1.4" stroke-linejoin="round"><path d="M{cx},{cy - 6.3} L{cx + 6.3},{cy} L{cx},{cy + 6.3} L{cx - 6.3},{cy} Z"/>'
            f'<path d="M{cx - 2.6},{cy - 2.4} L{cx},{cy + 2.6} L{cx + 2.6},{cy - 2.4}" stroke-width="1.6"/></g>')


def g_chrome(cx, cy, c):  # fa-chrome
    return (f'<g fill="none" stroke="{c}" stroke-width="1.4"><circle cx="{cx}" cy="{cy}" r="5.8"/><circle cx="{cx}" cy="{cy}" r="2.2" fill="{c}"/>'
            f'<path d="M{cx},{cy - 2.2} H{cx + 5.4} M{cx - 1.9},{cy + 1.1} L{cx - 4.6},{cy - 3.6} M{cx + 1.9},{cy + 1.1} L{cx - 0.8},{cy + 5.8}"/></g>')


def g_slack(cx, cy, c, w=1.9):  # fa-slack
    return (f'<g stroke="{c}" stroke-width="{w}" stroke-linecap="round"><path d="M{cx - 1.8},{cy - 5.6} V{cy + 5.6} M{cx + 1.8},{cy - 5.6} V{cy + 5.6} '
            f'M{cx - 5.6},{cy - 1.8} H{cx + 5.6} M{cx - 5.6},{cy + 1.8} H{cx + 5.6}"/></g>')


def g_discord(cx, cy, c):
    return (f'<g><path d="M{cx - 6},{cy + 3.5} Q{cx - 6.4},{cy - 2} {cx - 3.6},{cy - 5} Q{cx},{cy - 6} {cx + 3.6},{cy - 5} '
            f'Q{cx + 6.4},{cy - 2} {cx + 6},{cy + 3.5} Q{cx + 3.6},{cy + 5.6} {cx + 2},{cy + 4} H{cx - 2} Q{cx - 3.6},{cy + 5.6} {cx - 6},{cy + 3.5} Z" fill="{c}"/>'
            f'<circle cx="{cx - 2.2}" cy="{cy}" r="1.2" fill="{WELL}"/><circle cx="{cx + 2.2}" cy="{cy}" r="1.2" fill="{WELL}"/></g>')


def g_bluetooth(cx, cy, c):
    return (f'<path d="M{cx - 3.6},{cy - 3} L{cx + 3.6},{cy + 3} L{cx},{cy + 6} V{cy - 6} L{cx + 3.6},{cy - 3} L{cx - 3.6},{cy + 3}" '
            f'fill="none" stroke="{c}" stroke-width="1.5" stroke-linejoin="round" stroke-linecap="round"/>')


# ---------------------------------------------------------------- the desktop

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

# The README's alt text says the same.
ALT = (
    "A floating waybar of frosted pills whose purple clock says free until 15:00, then names the next meeting: "
    "Standup in 27m. A click opens the Timetable drawer out of the clock: Chicago, EET and GMT side by side, "
    "Standup next with Join on workspace 2 and Copy link, the day strip with every hour ruled, its events, dashed "
    "free gaps that say how long they are and a red now line tagged 14:33, the rest of today under it, and a "
    "focus block sized to the gap before Standup. The arrow beside the day steps to tomorrow, with an all-day "
    "release freeze, an unanswered invite outlined and a maybe striped, and Today comes back. Starting the focus "
    "block turns the clock green, counting down from 25:00, and tints its gap on the strip; pointing at Standup "
    "outlines it, and its tooltip says a click opens it in Google Calendar. Later the clock goes amber at Standup "
    "in 2m, then red with Standup · Join at 15:00, and a middle-click switches to the meeting on workspace 2, "
    "where the clock counts the joined meeting down calmly: Standup · 15m left."
)

add(f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" width="{W}" height="{H}" font-family="{FONT}" '
    f'xml:space="preserve" role="img" aria-label="{esc(ALT)}">')
STYLE_AT = len(out)
add("")  # the <style> goes here once every keyframe exists

# The bar floats 6px off the bottom; each group of modules is a pill.
BAR_Y, BAR_H = 620, 34
CY = BAR_Y + BAR_H / 2
MID = CY + 4.3  # text baseline in the bar
FS, CW = 12, 7.2  # the bar's font size and character advance

add(f"""<defs>
<linearGradient id="wall" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{BG0}"/><stop offset="1" stop-color="{BG1}"/></linearGradient>
<radialGradient id="haze1" cx=".85" cy="0" r=".75"><stop offset="0" stop-color="{FROST}" stop-opacity=".13"/><stop offset="1" stop-color="{FROST}" stop-opacity="0"/></radialGradient>
<radialGradient id="haze2" cx=".05" cy="1" r=".6"><stop offset="0" stop-color="{CLOCK}" stop-opacity=".09"/><stop offset="1" stop-color="{CLOCK}" stop-opacity="0"/></radialGradient>
<linearGradient id="cover" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#3b2b4a"/><stop offset=".7" stop-color="#b8706a"/><stop offset="1" stop-color="#d08770"/></linearGradient>
{DRAWER_DEFS}
<clipPath id="roll"><rect x="600" y="{BAR_Y + 2}" width="520" height="{BAR_H - 4}"/></clipPath>
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


# Workspace 1: Chrome behind Neovide. Hyprland slides it away on Join, and
# the loop slides it back.
ws_out = anim("transform", [
    (0, "none"), (SWITCH, "none"), (SWITCH + 0.4, f"translateX(-{W}px)"),
    (RESET, f"translateX(-{W}px)"), (RESET + 0.4, "none"), (T, "none"),
])
add(f'<g {style(ws_out, "animation-timing-function:cubic-bezier(.3,0,.2,1)")}>')
window(250, 300, 470, 270)
add('<rect x="320" y="307" width="170" height="23" rx="7" fill="#262a30"/>')
add('<rect x="266" y="342" width="438" height="22" rx="11" fill="#15171a"/>')
for i, lw in enumerate([300, 220, 340, 180, 260, 140]):
    add(f'<rect x="{272 + (i % 3) * 12}" y="{388 + i * 26}" width="{lw}" height="9" rx="4" fill="#3b4252" opacity=".8"/>')
window(36, 36, 600, 360, "~/Sites/cart")
lines = [(0, 210), (0, 300), (24, 260), (48, 330), (48, 180), (24, 120), (0, 60), (0, 0), (0, 240), (24, 350), (24, 200), (0, 90)]
cols = ["#4c566a", "#5e81ac99", "#434c5e", "#88c0d055", "#4c566a", "#434c5e", "#4c566a", "", "#5e81ac99", "#88c0d055", "#434c5e", "#4c566a"]
for i, ((indent, lw), col) in enumerate(zip(lines, cols)):
    if lw:
        add(f'<rect x="{58 + indent}" y="{86 + i * 24}" width="{lw}" height="9" rx="4" fill="{col}"/>')
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
    svg, bw = _button(bx, ZY + ZH - 46, lbl, "danger" if col else None)
    add(svg)
    bx += bw + 38
window(812, 36, 352, 560, "#team")
for i, (who, lw) in enumerate([("Avery", 230), ("Sam", 180), ("Jordan", 260), ("Avery", 140), ("Sam", 210)]):
    y = 92 + i * 92
    add(f'<circle cx="842" cy="{y}" r="12" fill="{tone[who]}" fill-opacity=".7"/>')
    text(864, y - 2, who, 12, FG, 700)
    add(f'<rect x="864" y="{y + 10}" width="{lw}" height="8" rx="4" fill="#3b4252"/>')
    add(f'<rect x="864" y="{y + 26}" width="{lw - 70}" height="8" rx="4" fill="#3b4252"/>')
add("</g>")

# ---------------------------------------------------------------- the bar


def pill(x, w, extra=""):
    add(f'<rect x="{x:.1f}" y="{BAR_Y + 0.5}" width="{w:.1f}" height="{BAR_H - 1}" rx="10" fill="{PILL}" stroke="{PILL_LINE}"{extra}/>')


def slot(x):
    """A glyph module (padding 0 8px) starting at x: its glyph's center."""
    return x + 8 + 6, x + 28


# Left: the workspaces (a dim number, then a glyph per window) and the
# focused window's title with its app's glyph. Join adds Zoom to 2 and
# focuses it; waybar redraws at once.
WS = [(0, SWITCH + 0.1), (RESET + 0.1, T)], [(SWITCH + 0.1, RESET + 0.1)]
for spans, active, ws2, title in [
    (WS[0], 1, [g_slack], (g_vim, "~/Sites/cart")),
    (WS[1], 2, [g_slack, g_video], (None, "Zoom Meeting")),
]:
    group(spans)
    buttons = [(1, [g_vim, g_chrome]), (2, ws2)]
    widths = [16 + CW + len(gs) * (CW + 12) for _, gs in buttons]
    pw = 2 + 8 + sum(widths) + 2 * len(widths)
    pill(6, pw)
    x = 6 + 1 + 4 + 1
    for (n, gs), bw in zip(buttons, widths):
        on = n == active
        if on:
            add(f'<rect x="{x:.1f}" y="{BAR_Y + 5}" width="{bw:.1f}" height="{BAR_H - 10}" rx="7" fill="{WS_ON}"/>')
        color = BRIGHT if on else BAR_FG
        text(x + 8, MID, str(n), FS, color, extra='fill-opacity=".55"')
        gx = x + 8 + CW
        for g in gs:
            add(g(gx + CW + 6, CY, color))
            gx += CW + 12
        x += bw + 2
    wx = 6 + pw + 6
    glyph, words = title
    ww = 2 + 24 + (12 + 2 * CW if glyph else 0) + len(words) * CW
    pill(wx, ww)
    tx = wx + 13
    if glyph:
        add(glyph(tx + 6, CY, BRIGHT))
        tx += 12 + 2 * CW
    text(tx, MID, words, FS, BRIGHT)
    add("</g>")

# Right, packed from the edge: the tray, then the clock, whose width pushes
# the desk, audio, lamps and media pills along.
TRAY_W = 2 + 10 + 3 * 14 + 2 * 8 + 10
TRAY_X = W - 6 - TRAY_W
pill(TRAY_X, TRAY_W)
for k, (g, c) in enumerate([(g_slack, "#e01e5a"), (g_discord, "#7289da"), (g_bluetooth, "#5e81ac")]):
    add(g(TRAY_X + 1 + 10 + 7 + k * 22, CY, c))
TT_R = TRAY_X - 6  # the clock's right edge

# What the clock module prints, in order: (from, glyph, event, its color,
# clock, how it arrives). Standup is at 15:00; a focus block ends 2 minutes
# before it. The state's class colors the event; clock_color keeps the
# clock purple.
FRAMES = [
    (0, None, "free until 15:00", CLOCK, "13:47", "flip"),
    (FLIPS[0], g_calendar, "Standup in 55m", FROST, "14:05", "flip"),
    (FLIPS[1], g_calendar, "Standup in 41m", FROST, "14:19", "flip"),
    (FLIPS[2], g_calendar, "Standup in 27m", FROST, "14:33", "flip"),
    (CLICK_FOCUS + LAG, g_timer, "Focus 25:00", GO, "14:33", "flip"),
    (TICKS[0], g_timer, "Focus 24:59", GO, "14:33", "cut"),
    (TICKS[1], g_timer, "Focus 24:58", GO, "14:33", "cut"),
    (TICKS[2], g_timer, "Focus 24:57", GO, "14:33", "cut"),
    (JUMP[0], g_timer, "Focus 16:12", GO, "14:41", "flip"),
    (JUMP[1], g_timer, "Focus 07:30", GO, "14:50", "flip"),
    (JUMP[2], g_calendar, "Standup in 2m", CAUTION, "14:58", "flip"),
    (STANDUP, g_video, "Standup · Join", STOP, "15:00", "flip"),
    (SWITCH + 0.1, g_group, "Standup · 15m left", FROST, "15:00", "flip"),
    (RESET, None, "free until 15:00", CLOCK, "13:47", "flip"),
]
FRAME_STILL = 3


def frame_width(f):
    _, glyph, event, _, clock, _ = f
    return 2 + 24 + (12 + CW if glyph else 0) + len(f"{event}  Tue Nov 10  {clock}") * CW


WIDTHS = [frame_width(f) for f in FRAMES]
W_STILL = WIDTHS[FRAME_STILL]


def frame_spans(i):
    t1 = FRAMES[i + 1][0] if i + 1 < len(FRAMES) else T
    spans = [(FRAMES[i][0], t1)]
    if i == 0:
        spans.append((RESET, T))
    return spans


# The pills the clock pushes, drawn where they sit in the still.
pts = [(0, WIDTHS[0])]
for i in range(1, len(FRAMES)):
    pts += [(FRAMES[i][0], WIDTHS[i - 1]), (FRAMES[i][0] + 0.01, WIDTHS[i])]
pts.append((T, WIDTHS[-1]))
push = anim("transform", [(t, f"translateX({W_STILL - w:.1f}px)") for t, w in pts])
add(f'<g {style(push)}>')
x = TT_R - W_STILL - 6
# The desk: Logbook's lamp (dim: nothing listens), then the drawer of key
# lights and power, collapsed to its handle. Lit key lights would sit
# between them; they are off.
DESK_W = 2 + 8 + 28 + 24
x -= DESK_W
pill(x, DESK_W)
add(g_mic(slot(x + 5)[0], CY, BAR_DIM))
add(g_chevron(x + 5 + 28 + 12, CY, BAR_DIM))
# Audio: volume in frost, the mouse's charge dim.
AUDIO_W = 2 + 8 + 2 * (16 + 12 + 4 * CW)
x -= 6 + AUDIO_W
pill(x, AUDIO_W)
ax = x + 5
for g, s, c in [(g_speaker, "42%", FROST), (g_mouse, "80%", BAR_DIM)]:
    add(g(ax + 8 + 6, CY, c))
    text(ax + 8 + 12 + CW, MID, s, FS, c)
    ax += 16 + 12 + 4 * CW
# Lamps: Roundhouse calm, Switchyard at rest, Signal Box quiet, the VPN
# down, do not disturb off.
LAMPS_W = 2 + 8 + 5 * 28
x -= 6 + LAMPS_W
pill(x, LAMPS_W)
lx = x + 5
for g in [lambda cx, cy, c: f'<circle cx="{cx}" cy="{cy}" r="5.5" fill="{GO}"/>',
          lambda cx, cy, c: g_puzzle(cx, cy, FROST),
          lambda cx, cy, c: f'<circle cx="{cx}" cy="{cy}" r="5.2" fill="none" stroke="{BAR_DIM}" stroke-width="1.5"/>',
          g_lan, g_bell]:
    add(g(slot(lx)[0], CY, BAR_DIM))
    lx += 28
# Media: Tannoy's cover and the song, amber while it plays.
SONG = "Kestrel · Tides"
MEDIA_W = 2 + 10 + 18 + 8 + len(SONG) * CW + 12
x -= 6 + MEDIA_W
pill(x, MEDIA_W)
add(f'<rect x="{x + 11}" y="{CY - 9}" width="18" height="18" rx="3" fill="url(#cover)"/>')
add(f'<circle cx="{x + 23}" cy="{CY - 3}" r="3.2" fill="{CAUTION}" opacity=".9"/>')
add(f'<path d="M{x + 11},{CY + 5} q4,-3 9,0 t9,0 v4 h-18 Z" fill="#2e2235"/>')
text(x + 11 + 18 + 8, MID, SONG, FS, CAUTION)
add("</g>")

# The clock: its own pill. Each frame draws the pill at its width (waybar
# redraws at once) and its text, which flips like a departure board.
for i, (t0, glyph, event, color, clock, how) in enumerate(FRAMES[:-1]):
    spans = frame_spans(i)
    group([(s, e + 0.01) for s, e in spans], 0.01)
    pill(TT_R - WIDTHS[i], WIDTHS[i])
    add("</g>")
# Hovered while the cursor is on it: #custom-timetable:hover.
for t, i in [(CLICK_CLOCK - 0.12, FRAME_STILL), (CLICK_JOIN - 0.12, 11)]:
    k = fade([(t, t + 0.7)], 0.1)
    add(f'<rect x="{TT_R - WIDTHS[i] + 1:.1f}" y="{BAR_Y + 1.5}" width="{WIDTHS[i] - 2:.1f}" height="{BAR_H - 3}" rx="9" fill="{HOVER}" {style(k)} opacity="0"/>')
add('<g clip-path="url(#roll)">')
for i, (t0, glyph, event, color, clock, how) in enumerate(FRAMES[:-1]):
    t1, leave = FRAMES[i + 1][0], FRAMES[i + 1][5]
    spans = [(t0, t1, how, leave)]
    if i == 0:  # the loop comes back to it
        spans.append((RESET, T, "flip", "flip"))
    x = TT_R - WIDTHS[i] + 13
    add(f'<g {style(board(spans))} opacity="{1 if i == FRAME_STILL else 0}">')
    cls = ' class="now"' if glyph is g_video else ""
    if glyph:
        add(f"<g{cls}>{glyph(x + 6, CY, color)}</g>")
        x += 12 + CW
    add(f'<text x="{x:.1f}" y="{MID}" font-size="{FS}" fill="{color}"><tspan{cls}>{esc(event)}</tspan><tspan fill="{CLOCK}">  Tue Nov 10  {clock}</tspan></text>')
    add("</g>")
add("</g>")

# ---------------------------------------------------------------- the drawer

# What the drawer shows while it is open. The clock is stopped at 14:33 for
# the story; only the focus block's seconds move.
NOW = hm(14, 33)
BLOCK = (NOW, hm(14, 58))
OPEN_SPANS = [
    ((OPEN, CLICK_DAY + LAG), Board(NOW)),
    ((CLICK_DAY + LAG, CLICK_TODAY + LAG), Board(NOW, offset=1)),
    ((CLICK_TODAY + LAG, CLICK_FOCUS + LAG), Board(NOW)),
    ((CLICK_FOCUS + LAG, TICKS[0]), Board(NOW, BLOCK)),
    ((TICKS[0], HOVER_ON), Board(NOW + 1, BLOCK)),
    ((HOVER_ON, TICKS[1]), Board(NOW + 1, BLOCK, hover="Standup")),
    ((TICKS[1], HOVER_OFF), Board(NOW + 2, BLOCK, hover="Standup")),
    ((HOVER_OFF, TICKS[2]), Board(NOW + 2, BLOCK)),
    ((TICKS[2], CLOSED + 0.3), Board(NOW + 3, BLOCK)),
]

# Opens from where the clock was clicked, above the bar, clamped to the
# monitor as bar-anchor does (10px side margins, drawer.gap 8).
CLOCK_X = TT_R - W_STILL / 2
DX = min(max(CLOCK_X - DRAWER_W / 2, 10), W - DRAWER_W - 10)


def top(b):
    return BAR_Y - 8 - drawer_size(b)[1]


def spot(b, name):
    lx, ly = drawer_spots(b)[name]
    return DX + lx, top(b) + ly


spring = anim("transform", [
    (0, "translateY(26px) scale(.06)"), (OPEN - 0.01, "translateY(26px) scale(.06)"),
    (OPEN + 0.10, "translateY(14px) scale(.5)"), (OPEN + 0.20, "translateY(4px) scale(.9)"),
    (OPEN + 0.30, "translateY(-1px) scale(1.035)"), (OPEN + 0.42, "translateY(0) scale(.988)"),
    (OPEN + 0.56, "scale(1.006)"), (OPEN + 0.72, "scale(1)"), (CLICK_OUT + 0.08, "scale(1)"),
    (CLOSED, "translateY(14px) scale(.4)"), (T, "translateY(14px) scale(.4)"),
])
show = anim("opacity", [(0, 0), (OPEN - 0.01, 0), (OPEN + 0.12, 0.85), (OPEN + 0.2, 1), (CLICK_OUT + 0.08, 1), (CLOSED, 0), (T, 0)])
add(f'<g class="a" style="animation-name:{spring},{show};transform-origin:{CLOCK_X:.1f}px {CY:.1f}px">')
# Each part (the panel, then each section) is drawn once for every way it
# looks, shown while the drawer looks that way. The drawer grows upward
# while a block counts down, so a part is keyed by where it sits too.
seen = {}
for span, b in OPEN_SPANS:
    for part in drawer_parts(b):
        seen.setdefault((top(b), part), []).append(span)
for (y, part), spans in seen.items():
    group(spans, 0.03)
    add(f'<g transform="translate({DX:.1f},{y:.1f})">{part}</g></g>')
add("</g>")

# Standup's tooltip, from GTK, under the pointer.
FOCUS_B = Board(NOW + 1, BLOCK, hover="Standup")
HX, HY = spot(FOCUS_B, "ev:Standup")
HX, HY = HX - 3, HY + 5  # low on the block, so its outline shows
group([(TIP_ON, HOVER_OFF)], 0.12)
tip = ["15:00 to 15:15  Standup", "Click to open it in Google Calendar"]
tipw = max(len(s) for s in tip) * 6.6 + 20
tx, ty = min(HX - 20, W - tipw - 12), HY + 26
add(f'<rect x="{tx:.1f}" y="{ty:.1f}" width="{tipw:.1f}" height="44" rx="10" fill="#26282c" stroke="#3a404d"/>')
for k, s in enumerate(tip):
    text(tx + 10, ty + 18 + k * 16, s, 11, FG)
add("</g>")

# ---------------------------------------------------------------- cursor
A, Btom, F = Board(NOW), Board(NOW, offset=1), Board(NOW)
DAY_X, DAY_Y = spot(A, "next_day")
TODAY_X, TODAY_Y = spot(Btom, "today")
FOCUS_X, FOCUS_Y = spot(F, "focus")
OUT_X, OUT_Y = 300, 250
REST_X, REST_Y = 640, 380
JOIN_X = TT_R - WIDTHS[11] / 2
clicks = [(CLICK_CLOCK, CLOCK_X, CY), (CLICK_DAY, DAY_X, DAY_Y), (CLICK_TODAY, TODAY_X, TODAY_Y),
          (CLICK_FOCUS, FOCUS_X, FOCUS_Y), (CLICK_OUT, OUT_X, OUT_Y), (CLICK_JOIN, JOIN_X, CY)]
for t, cx_, cy_ in clicks:
    k = anim("opacity", [(0, 0), (t - 0.01, 0), (t, 0.9), (t + 0.45, 0), (T, 0)])
    k2 = anim("transform", [(0, "scale(.3)"), (t, "scale(.3)"), (t + 0.45, "scale(1.6)"), (T, "scale(1.6)")])
    color = STOP if t == CLICK_JOIN else FROST
    add(f'<circle class="a ripple" style="animation-name:{k},{k2};transform-box:fill-box;transform-origin:center" cx="{cx_:.1f}" cy="{cy_:.1f}" r="18" fill="none" stroke="{color}" stroke-width="2.5" opacity="0"/>')


def tr(x, y):
    return f"translate({x - 3:.1f}px, {y - 4:.1f}px)"


cur = anim("transform", [
    (0, tr(REST_X, REST_Y)), (1.9, tr(REST_X, REST_Y)), (CLICK_CLOCK - 0.15, tr(CLOCK_X, CY)), (3.3, tr(CLOCK_X, CY)),
    (CLICK_DAY - 0.15, tr(DAY_X, DAY_Y)), (CLICK_DAY + 0.35, tr(DAY_X, DAY_Y)),
    (CLICK_TODAY - 0.15, tr(TODAY_X, TODAY_Y)), (CLICK_TODAY + 0.3, tr(TODAY_X, TODAY_Y)),
    (CLICK_FOCUS - 0.15, tr(FOCUS_X, FOCUS_Y)), (CLICK_FOCUS + 0.4, tr(FOCUS_X, FOCUS_Y)),
    (HOVER_ON - 0.05, tr(HX, HY)), (HOVER_OFF + 0.05, tr(HX, HY)),
    (CLICK_OUT - 0.15, tr(OUT_X, OUT_Y)), (CLICK_JOIN - 0.9, tr(OUT_X, OUT_Y)),
    (CLICK_JOIN - 0.15, tr(JOIN_X, CY)), (CLICK_JOIN + 0.7, tr(JOIN_X, CY)),
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
