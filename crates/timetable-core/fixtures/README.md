# Fixtures

- `google-anonymized.ics`: cut from a real Google Calendar secret iCal feed
  (83 events in 21 series around 2026-11-10, with their moved instances,
  EXDATEs and VTIMEZONEs) and anonymized: every summary, description,
  location, address, UID and link is a placeholder; times and recurrence
  are as Google wrote them, folded at 75 octets with CRLF line ends. The
  expanded window (2026-11-09 to 2026-11-25) was checked against Python's
  `recurring_ical_events`.
- `edge-cases.ics`: hand-written cases a work calendar rarely has all at
  once: a cancelled series and a cancelled instance, an event the owner
  declined, all-day and floating times, a Windows zone name, `DURATION`,
  `RDATE`, a comma-separated `EXDATE`, an alarm with its own description,
  Zoom, Meet, Teams and Slack huddle links, own time (owner and a room
  only, `TRANSP:TRANSPARENT`, no guests and no link) and `TENTATIVE` and
  `NEEDS-ACTION` answers. Checked against `recurring_ical_events` too.
- `waybar-platform.css`: the floating "Platform" bar's stylesheet, whose
  window background is transparent.
