<p align="center">
  <img src="docs/assets/mark.svg" width="34" alt="">
</p>

<h1 align="center">Timetable</h1>

<p align="center">
  <img src="docs/assets/hero.svg" width="100%" alt="A floating waybar of frosted pills whose purple clock says free until 15:00, then names the next meeting: Standup in 27m. A click opens the Timetable drawer out of the clock: Chicago, EET and GMT side by side, Standup next with Join on workspace 2 and Copy link, the day strip with every hour ruled, its events, dashed free gaps that say how long they are and a red now line tagged 14:33, the rest of today under it, and a focus block sized to the gap before Standup. The arrow beside the day steps to tomorrow, with an all-day release freeze, an unanswered invite outlined and a maybe striped, and Today comes back. Starting the focus block turns the clock green, counting down from 25:00, and tints its gap on the strip; pointing at Standup outlines it, and its tooltip says a click opens it in Google Calendar. Later the clock goes amber at Standup in 2m, then red with Standup · Join at 15:00, and a middle-click switches to the meeting on workspace 2, where the clock counts the joined meeting down calmly: Standup · 15m left.">
</p>

<p align="center">
  <b>The clock in your bar tells you what's next. "Standup in 7m", then a red Join at start time.</b><br>
  One click opens a drawer with three time zones, the day laid out with its free gaps,<br>
  and focus blocks that end two minutes before your next meeting.
</p>

<p align="center">
  <a href="https://github.com/NathanielInman/timetable/actions/workflows/ci.yml"><img src="https://github.com/NathanielInman/timetable/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/NathanielInman/timetable/releases/latest"><img src="https://img.shields.io/github/v/release/NathanielInman/timetable?color=88c0d0&amp;label=release" alt="Latest release"></a>
  <img src="https://img.shields.io/badge/status-early%20(0.1)-ebcb8b" alt="Early, version 0.1">
  <img src="https://img.shields.io/badge/Arch%20%2B%20Hyprland%20first-Linux%20%C2%B7%20Wayland-a3be8c" alt="Arch and Hyprland first; Linux with Wayland">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-b48ead" alt="MIT license"></a>
</p>

A timetable is the board on the platform that says what leaves next and
when. [Switchyard](https://github.com/NathanielInman/switchyard) dispatches
your work, [Roundhouse](https://github.com/NathanielInman/roundhouse) says
whether the machine is fit to run it, [Tannoy](https://github.com/NathanielInman/tannoy)
tells you what is playing, and Timetable tells you where you need to be.

## Why

My bar had two scripts in that corner. `timezone.sh` showed one zone at a
time: seeing Eastern Europe meant clicking, which ran `pgrep -f` to find the
script and sent it a signal, then clicking twice more to get back to
Chicago. `pomodoro.sh` ran its 25 minutes whether or not a meeting started
in 10, and polled every second to do it. Neither knew about my calendar, so
every meeting started with the same hunt: open the calendar tab, find the
event, find the Zoom link, switch to the Slack and Zoom workspace, join a
minute late. Timetable replaces both scripts and makes that hunt one click.

- **The next thing, in the clock.** Calm while the next event is over an
  hour away ("free until 14:00"), named within the hour ("Standup in 34m"),
  amber at 10 minutes, red and pulsing from its start for 5 minutes
  ("Standup · Join"), then "9m left" while it runs. Once you join, the
  amber and red stop. All-day events never count as next.
- **Your own time stays quiet.** An event nobody else is invited to with
  no meeting link (Focus time, a workout), one marked free, or a title you
  list (`own_time = ["Lunch"]`) never turns the clock amber or red and
  gets no reminders. While it runs the clock relaxes into "Focus time ·
  until 16:30", and a focus block runs right through it to your next real
  meeting.
- **Join from wherever your eyes are.** Middle-click the clock, or Join in
  the drawer, and Hyprland switches to your meeting workspace and the Zoom,
  Meet, Teams or Slack huddle link opens. Timetable finds the link in
  Google's conference data, the location or the description.
- **Three zones side by side.** Chicago, EET and GMT by default, any IANA
  zones you like. No cycling.
- **Focus blocks that fit.** Start one and it runs until 2 minutes before
  your next event (50 minutes when the rest of the day is free), counts down
  in the bar, survives a restart, and notifies you when it ends. This is
  what replaced the pomodoro.
- **Reminders at 5 and 1 minutes**, once per meeting, through
  `notify-send`, with a Join button. swaync's do not disturb silences them
  like any other notification.
- **Your calendar, no OAuth app.** Timetable reads Google Calendar's secret
  iCal address (or any iCal feed, or a file). It keeps the address in memory
  only, never in a log, cache or error message, and it can come from
  [broker](https://github.com/NathanielInman/credential-broker) instead of
  a config file.

## What's in the drawer

The date, how many events today, when the calendar was last read and a
state chip. All-day events under that. A tile per zone with its time and
offset. The next meeting (or the one you're in) with its time, where it is,
**Join on workspace 2** and **Copy link**. The day from 08:00 to 18:00
(wider when your day is), with a red now line and each free gap saying how
long it is, two ways: a strip across the drawer with every hour marked and
the rest of today listed under it, or a timeline beside the rest, tall
enough to read every title. The **Timeline** / **Strip** button beside it
switches, and the drawer remembers. **‹** and **›** step to other days for
a look at tomorrow (the drawer opens on today again). Your own time is soft
green; an invite you answered maybe is striped and one you haven't
answered is outlined. Click any event to open it in Google Calendar,
where you change it: Timetable only shows the calendar. The focus card
says what a block would
be right now ("Free until 16:30: 88 min of focus, warned at 16:28") or
counts down the one that is running.

<kbd>j</kbd> joins, <kbd>c</kbd> copies the link, <kbd>e</kbd> opens the
card's event in Google Calendar, <kbd>f</kbd> starts or
stops a focus block, <kbd>v</kbd> switches the day's view, <kbd>←</kbd>
and <kbd>→</kbd> step through days and <kbd>t</kbd> comes back to today,
<kbd>r</kbd>
reads the calendar again, and <kbd>Esc</kbd> or a click outside closes it.

`timetable agenda` prints today in a terminal (`--days 3`, `--json`).

## Install

Timetable is at 0.1. It runs on Linux with Wayland, and Hyprland gets the
exact-icon anchoring and the workspace switch. Packages come later; for now
grab a tarball from the [releases](https://github.com/NathanielInman/timetable/releases)
or build from source:

```sh
# Arch: the drawer's libraries
sudo pacman -S --needed gtk4 gtk4-layer-shell

git clone https://github.com/NathanielInman/timetable
cd timetable
cargo build --release
install -Dm755 target/release/timetable target/release/timetable-board -t ~/.local/bin/
timetable doctor
```

Reminders need `notify-send` (libnotify) and Join opens links with
`xdg-open`.

## Point it at your calendar

In Google Calendar, open **Settings**, pick your calendar under **Settings
for my calendars**, and copy **Secret address in iCal format** from
**Integrate calendar**. Anyone with that address can read your calendar, so
treat it like a password.

With [broker](https://github.com/NathanielInman/credential-broker), store it
in the public tier (no hardware touch, since the daemon reads it at start)
and tell Timetable to ask for it:

```sh
broker set TIMETABLE_ICAL_URL --public --desc "Google Calendar secret iCal address for Timetable"
```

```toml
# ~/.config/timetable/config.toml
[calendar]
url_command = "broker get TIMETABLE_ICAL_URL"
```

Without broker, keep it in a file only you can read:

```sh
install -m600 /dev/null ~/.config/timetable/ical-url
$EDITOR ~/.config/timetable/ical-url
```

```toml
[calendar]
url_command = "cat ~/.config/timetable/ical-url"
```

The daemon runs `url_command` once when it starts, again when the address
stops working (HTTP 401, 403, 404 or 410) and on `timetable refresh`, and
never in between. It reads the feed every 5 minutes on its own thread with a
60 second timeout and keeps the last good agenda when a read fails; the
drawer and the tooltip say how old it is. A busy work calendar's feed is
over 10 MB with years of history; Timetable reads it in about 30 ms and only
expands the window from yesterday to two weeks out.

Events you declined are skipped. Timetable knows it's you from the Google
address (the calendar id between `/ical/` and `/private-` is your email), or
from `calendar.owner` for another feed.

## Put it in your bar

```jsonc
"modules-right": ["...", "custom/timetable", "tray"],

"custom/timetable": {
  "exec": "~/.local/bin/timetable bar",
  "return-type": "json",
  "on-click": "~/.local/bin/timetable toggle",
  "on-click-right": "gnome-calendar",
  "on-click-middle": "~/.local/bin/timetable join",
  "tooltip": true
}
```

Use full paths: a waybar started by systemd usually does not have
`~/.local/bin` on its `PATH`. `timetable bar` starts the daemon the first
time it runs (and again if it ever dies), and the daemon starts the drawer
the first time you click. With two monitors, both bars start `timetable
bar` at once; one daemon wins a lock and the other bar just connects.

The bar prints a line when something changes and once a minute for the
clock. It only ticks every second while a focus block counts down.

The module's first class is the state: `free`, `next`, `soon`, `now`,
`in`, `own` (your own time), `focus` or `offline` (a calendar is set but
has never been read).
`now` also gets `pulse`, and an agenda that hasn't refreshed in a while gets
`stale`:

```css
#custom-timetable { color: #b48ead; }
#custom-timetable.next,
#custom-timetable.in { color: #88c0d0; }
#custom-timetable.soon { color: #ebcb8b; }
#custom-timetable.now {
  color: #bf616a;
  animation: timetable-pulse 1s ease-in-out infinite alternate;
}
#custom-timetable.own { color: rgba(163, 190, 140, 0.75); }
#custom-timetable.focus { color: #a3be8c; }
#custom-timetable.offline { color: #616e88; }
@keyframes timetable-pulse { to { color: #6e3b40; } }
```

The state colors the whole module. To keep the clock its own color while
an event shows, set `bar.clock_color` and Timetable wraps the clock in Pango
markup.

For a key, point a Hyprland bind at the same commands:

```lua
hl.bind(mod .. " + J", hl.dsp.exec_cmd("~/.local/bin/timetable join"))
hl.bind(mod .. " + SHIFT + F", hl.dsp.exec_cmd("~/.local/bin/timetable focus toggle"))
```

## Commands

| Command | What it does |
|---|---|
| `timetable bar` | The waybar module |
| `timetable toggle`, `show`, `hide` | The drawer, from the clicked clock (`--center` to center it) |
| `timetable join [--dry-run]` | Switch to `join.workspace` and open the current or next meeting's link; `--dry-run` only says which |
| `timetable calendar [--dry-run]` | Open today in Google Calendar |
| `timetable focus start`, `stop`, `toggle` | A focus block until just before the next event |
| `timetable focus status` | The running block, or what one would be |
| `timetable agenda [--days N] [--json]` | Today (and more days) as text |
| `timetable refresh` | Read the calendar now, and run `url_command` again |
| `timetable status [--json]` | The state, the agenda's age, the zones |
| `timetable doctor` | Checks the config, the calendar, the daemon, Hyprland, `notify-send`, `xdg-open`, waybar and the drawer |
| `timetable daemon` | The daemon in the foreground |

Join picks the meeting with a link that is running or starts within
`bar.soon_minutes` (the one about to start wins over the tail of the one
before it), else the next one today with a link.

## Config

Everything is optional; without `[calendar]` the module is a clock.
`~/.config/timetable/config.toml`:

```toml
[calendar]
url_command = ""       # prints the address, e.g. "broker get TIMETABLE_ICAL_URL"
url = ""               # or the address itself
file = ""              # or a local .ics file (wins over both)
owner = ""             # your address, for skipping declines; derived from a Google address
refresh = 5            # minutes between reads
timeout = 60           # seconds one read may take
past_days = 1          # the agenda runs from this many days back
future_days = 14       # to this many days ahead
own_time = []          # titles that are your own time even with guests, e.g. ["Lunch"]

[clock]
format = "%a %b %-d  %H:%M"   # chrono strftime
zone = ""              # your zone (IANA); empty reads TZ, then /etc/localtime
zones = [
  { zone = "America/Chicago", label = "Chicago" },
  { zone = "EET", label = "EET" },
  { zone = "GMT", label = "GMT" },
]

[bar]
max_chars = 24         # longest event title, cut at a word
next_minutes = 60      # name the next event within this (class next)
soon_minutes = 10      # amber within this (class soon)
now_minutes = 5        # red with Join for this long after the start (class now)
free_until = true      # "free until 14:00" when something is left today
separator = "  "       # between the event and the clock
clock_color = ""       # Pango color for the clock, e.g. "#B48EAD"
icon_event = "\U000F00ED"    # md-calendar
icon_now = "\U000F0567"      # md-video
icon_in = "\U000F0849"       # md-account-group
icon_focus = "\U000F051B"    # md-timer-outline
icon_own = "\U000F032A"      # md-leaf
icon_offline = "\U000F0164"  # dim, before the clock when the calendar is unreachable
namespace = "waybar"   # the bar's layer-shell namespace, for anchoring

[join]
workspace = "2"        # Hyprland workspace to switch to; "" stays put
dispatch = "hl.dsp.focus({ workspace = {workspace} })"   # the Lua dispatcher
opener = "xdg-open"    # gets the link as its last argument, and the
                       # events Timetable opens in Google Calendar
# Chrome opens links in the profile used last; to pin the one your calendar
# signs in to: opener = 'google-chrome-stable --profile-directory="Profile 1"'
# (chrome://version shows a profile's directory under "Profile Path")

[reminders]
minutes = [5, 1]       # [] for none

[focus]
minutes = 50           # a block's length when nothing else is on today
lead = 2               # end this many minutes before the next event
max = 0                # longest block in minutes; 0 is no limit
min = 5                # shortest block worth starting

[day]
start = "08:00"        # the day strip and free gaps
end = "18:00"
min_gap = 15           # shortest gap drawn as free, in minutes

[drawer]
width = 0.6            # of the monitor; above 1 it is pixels (420)
max_height = 0.85
gap = 8
calendar_hint = "right-click the clock for the calendar"   # the footer's last words

[theme]
from = "waybar"        # or "none" for the built-in theme
# accent = "#88c0d0"
# font = "JetBrains Mono"
```

A typo in the config falls back to the defaults, and the drawer and
`timetable doctor` say what is wrong. `~/.config/timetable/style.css`
overrides any part of the drawer's look; redefining one of the
`@define-color tt_*` tokens in [style.css](crates/timetable-board/src/style.css)
recolors everything that uses it.

## Siblings

Timetable is the fifth of a family, each one module in the bar you already
have: [Switchyard](https://github.com/NathanielInman/switchyard) (builds and
launches), [Roundhouse](https://github.com/NathanielInman/roundhouse) (the
machine), [Tannoy](https://github.com/NathanielInman/tannoy) (now playing)
and Signal Box (credentials, part of
[broker](https://github.com/NathanielInman/credential-broker)). They share
`bar-anchor`, so every drawer opens from its own icon with the same motion,
and the same `Anchor` JSON. None of them needs the others.

## Cost

The daemon wakes once a minute to redraw the clock, once a second only
during a focus block, and every 5 minutes to read the calendar on a
separate thread. Reading and expanding a 12 MB feed takes about 30 ms of
CPU after the download. Nothing in the bar's path starts a process or
touches the network; `notify-send`, the opener and `url_command` run only
when a reminder is due, you join or open an event, or the address is
needed.

## Development

```sh
cargo test --workspace
# A dev daemon that leaves an installed one alone:
export TIMETABLE_RUNTIME_DIR=/tmp/tt-dev/run TIMETABLE_STATE_DIR=/tmp/tt-dev/state \
       TIMETABLE_CACHE_DIR=/tmp/tt-dev/cache TIMETABLE_CONFIG_DIR=/tmp/tt-dev/config
mkdir -p /tmp/tt-dev/config
printf '[calendar]\nfile = "%s"\n[join]\nworkspace = ""\nopener = "echo"\n' \
  "$PWD/crates/timetable-core/fixtures/edge-cases.ics" > /tmp/tt-dev/config/config.toml
cargo build && target/debug/timetable daemon &
target/debug/timetable agenda --days 14
target/debug/timetable toggle --center
```

See [AGENTS.md](AGENTS.md) for the crate layout and
[docs/design.md](docs/design.md) for how it works.

## License

MIT
