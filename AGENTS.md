# Timetable

`docs/design.md` is the spec; the README is the tour. Read the spec section
for whatever you change.

## Crates

- `bar-anchor`: Hyprland anchoring and panel placement, no GTK. A copy of
  Tannoy's (from Roundhouse); its `Anchor` JSON must stay identical to
  Roundhouse's, Switchyard's and Tannoy's.
- `timetable-core`: the model, the iCal reader (`ical.rs`), recurrence and
  placing events in time (`expand.rs`), meeting links and events' Google
  Calendar pages (`links.rs`), the agenda logic (`agenda.rs`: bar states,
  gaps, focus plans, Join's target, due reminders), fetching (`fetch.rs`),
  config, the socket protocol and the waybar reader. No threads, no UI.
- `timetable`: the daemon (`daemon.rs`), the waybar line (`bar.rs`), Join
  and notifications (`act.rs`) and the CLI (one binary, `timetable`).
- `timetable-board`: the Linux GTK4 layer-shell drawer. `anim.rs` and
  `stage.rs` came from Tannoy's board.

## Build and test

`cargo build --workspace --locked`, `cargo test --workspace --locked` and
`cargo clippy --workspace --all-targets -- -D warnings`. The drawer needs
gtk4-layer-shell; `.github/scripts/gtk4-layer-shell.sh` is how CI gets it.

Parsers are pure functions over text and are tested against samples in
`crates/timetable-core/fixtures/`: an anonymized cut of a real Google feed
and hand-written edge cases. Expansion results were checked against
Python's `recurring_ical_events`; when you change `ical.rs` or `expand.rs`,
check again and add a case to `edge-cases.ics` or `tests/fixtures.rs`.

## Privacy

The repo is public; calendars are not.

- Never commit a real feed, event title, attendee, address or link. A new
  fixture is cut from a real feed only after every summary, description,
  location, address, UID and URL is replaced with a placeholder.
- The calendar's address is a password. It lives in the daemon's memory
  only: never log it, cache it, print it or put it in an error.
  `fetch::redact` scrubs it from anything that might carry it, and
  `Location`'s `Debug` hides it. Test with `broker exec
  TIMETABLE_ICAL_URL -- ...`, never by pasting it.
- Never join a real meeting while testing. Use a fixture event with a
  harmless link, `join --dry-run`, or a dev config whose `join.opener`
  writes to a file and whose `join.workspace` is empty.

## Isolation

The machine you work on may run an installed Timetable. Point a dev build
somewhere else with `TIMETABLE_RUNTIME_DIR`, `TIMETABLE_STATE_DIR` and
`TIMETABLE_CACHE_DIR`, and `TIMETABLE_CONFIG_DIR` for a test config with
`[calendar] file = ...` and `[reminders] minutes = []`.

## Rules

- Nothing in the bar's path starts a process or touches the network. The
  fetch runs on its own thread with a timeout; `url_command` runs at start,
  on an auth failure and on `refresh`; `notify-send`, the opener and the
  Hyprland dispatch run only when something is due or asked for.
- Nothing may stall the bar stream. The lock that feeds the bar is held for
  memory work only: parse and expand happen before it is taken.
- No one-second polling. The ticker wakes at the next minute, and every
  second only while a focus block runs.
- All-day events are never "next" and never get reminders.
- Do not add do not disturb logic: reminders are ordinary notifications and
  swaync silences them.

## When behavior changes

Update in the same commit:

- `docs/design.md` for a new config key, protocol message, state or rule.
- The README for anything a new user meets (CLI, keys, install, config).

Commits are conventional, scoped by area: `anchor`, `core`, `ical`,
`expand`, `links`, `agenda`, `fetch`, `bar`, `cli`, `daemon`, `board`,
`docs`, `ci`.
