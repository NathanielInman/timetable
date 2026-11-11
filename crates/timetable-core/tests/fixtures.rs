//! The parser, the expansion and the agenda against recorded feeds. The
//! expected occurrences were checked against Python's
//! `recurring_ical_events` (see `fixtures/README.md`).

use chrono::{DateTime, NaiveDateTime};
use chrono_tz::America::Chicago;
use timetable_core::config::Config;
use timetable_core::expand::{expand, local_instant, window};
use timetable_core::model::Event;
use timetable_core::{agenda, ical};

const GOOGLE: &str = include_str!("../fixtures/google-anonymized.ics");
const EDGES: &str = include_str!("../fixtures/edge-cases.ics");

fn at(s: &str) -> i64 {
    local_instant(
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap(),
        Chicago,
    )
}

fn agenda_of(text: &str, now: &str) -> Vec<Event> {
    let feed = ical::parse(text, None);
    let (from, to) = window(at(now), Chicago, 1, 14);
    expand(&feed, Chicago, from, to).events
}

fn rows(events: &[Event]) -> Vec<String> {
    events
        .iter()
        .map(|e| {
            let s = DateTime::from_timestamp(e.start, 0)
                .unwrap()
                .with_timezone(&Chicago);
            format!(
                "{} {}m {} {}",
                s.format("%a %m-%d %H:%M"),
                (e.end - e.start) / 60,
                e.kind.as_deref().unwrap_or("-"),
                e.title
            )
        })
        .collect()
}

#[test]
fn google_feed_parses() {
    let feed = ical::parse(GOOGLE, None);
    assert_eq!(feed.events.len(), 83);
    assert_eq!(feed.calname.as_deref(), Some("owner@example.com"));
    assert_eq!(feed.timezone.as_deref(), Some("America/Chicago"));
    assert!(feed.unknown_zones.is_empty());
    assert_eq!(
        feed.events
            .iter()
            .filter(|e| e.recurrence_id.is_some())
            .count(),
        62
    );
    assert_eq!(feed.events.iter().filter(|e| e.declined).count(), 1);
}

#[test]
fn google_feed_expands_like_the_reference() {
    let ev = agenda_of(GOOGLE, "2026-11-10 12:00");
    assert_eq!(ev.len(), 63);
    assert!(ev.iter().all(|e| !e.all_day));
    let r = rows(&ev);
    let today: Vec<&String> = r.iter().filter(|r| r.starts_with("Tue 11-10")).collect();
    assert_eq!(
        today,
        vec![
            "Tue 11-10 07:00 50m - Event 019",
            "Tue 11-10 08:30 30m meet Event 008 (moved)",
            "Tue 11-10 09:00 60m meet Event 020 (moved)",
            "Tue 11-10 11:30 60m - Event 009",
            "Tue 11-10 12:30 240m - Event 011",
        ]
    );
    // The one Zoom meeting keeps its link from the location.
    let zoom: Vec<&Event> = ev
        .iter()
        .filter(|e| e.kind.as_deref() == Some("zoom"))
        .collect();
    assert_eq!(zoom.len(), 1);
    assert_eq!(
        zoom[0].link.as_deref(),
        Some("https://example.zoom.us/j/1000000007?pwd=placeholder")
    );
    assert_eq!(
        ev.iter()
            .filter(|e| e.kind.as_deref() == Some("meet"))
            .count(),
        22
    );
}

#[test]
fn google_feed_agenda() {
    let ev = agenda_of(GOOGLE, "2026-11-10 12:00");
    let cfg = Config::default();
    let s = agenda::status(
        &ev,
        at("2026-11-10 12:00"),
        None,
        &[],
        true,
        true,
        &cfg,
        Chicago,
    );
    // A daily hold with no guests and no link: your own time, not a
    // meeting.
    assert_eq!(s.state, agenda::State::Own);
    assert_eq!(s.event.unwrap().title, "Event 009");
    let s = agenda::status(
        &ev,
        at("2026-11-10 12:25"),
        None,
        &[],
        true,
        true,
        &cfg,
        Chicago,
    );
    // Event 011 at 12:30 is a guestless hold too, so nothing turns amber.
    assert_eq!(s.state, agenda::State::Own);
    assert_eq!(s.event.unwrap().title, "Event 009");
}

#[test]
fn edge_cases() {
    let ev = agenda_of(EDGES, "2026-11-10 12:00");
    assert_eq!(
        rows(&ev),
        vec![
            "Mon 11-09 09:30 15m zoom Standup",
            "Tue 11-10 00:00 1440m - Out of office",
            "Tue 11-10 09:30 15m zoom Standup",
            "Tue 11-10 14:00 60m teams Sprint review",
            "Wed 11-11 11:30 60m - Lunch",
            "Wed 11-11 13:00 60m - Design crit",
            "Wed 11-11 14:00 30m meet Planning",
            "Wed 11-11 15:00 30m - Coffee walk",
            "Wed 11-11 16:00 60m - Dentist",
            "Thu 11-12 11:30 15m zoom Standup (later today)",
            "Thu 11-12 14:00 30m meet Vendor call",
            "Mon 11-16 09:30 15m zoom Standup",
            "Mon 11-16 10:00 30m slack Kyiv sync",
            "Tue 11-17 09:30 15m zoom Standup",
            "Thu 11-19 09:30 15m zoom Standup",
            "Thu 11-19 10:00 30m slack Kyiv sync",
            "Fri 11-20 09:30 15m zoom Standup",
            "Mon 11-23 09:30 15m zoom Standup",
            "Mon 11-23 10:00 30m slack Kyiv sync",
            "Tue 11-24 09:30 15m zoom Standup",
        ]
    );
    // The alarm's Zoom link never replaces the event's own.
    let review = ev.iter().find(|e| e.title == "Sprint review").unwrap();
    assert!(review
        .link
        .as_deref()
        .unwrap()
        .starts_with("https://teams.microsoft.com/l/meetup-join/"));
}

#[test]
fn own_time_and_answers() {
    use timetable_core::model::Answer;
    let ev = agenda_of(EDGES, "2026-11-10 12:00");
    let get = |t: &str| ev.iter().find(|e| e.title == t).unwrap();
    // Only the owner and a room: own time. Marked free: own time. No
    // guests and no link: own time.
    assert!(get("Lunch").own);
    assert!(get("Coffee walk").own);
    assert!(get("Dentist").own);
    // Someone else is invited, or there is a link: a meeting.
    assert!(!get("Design crit").own);
    assert!(!get("Planning").own);
    assert!(!get("Sprint review").own);
    assert_eq!(get("Design crit").answer, Answer::Maybe);
    assert_eq!(get("Planning").answer, Answer::Unanswered);
    assert_eq!(get("Lunch").answer, Answer::Yes);
    // Own time is free: the 12:00 focus block runs to the crit at 13:00.
    let cfg = Config::default();
    let p = agenda::focus_plan(&ev, at("2026-11-11 11:40"), &cfg, Chicago).unwrap();
    assert_eq!(p.before.unwrap().title, "Design crit");
    let s = agenda::status(
        &ev,
        at("2026-11-11 11:40"),
        None,
        &[],
        true,
        true,
        &cfg,
        Chicago,
    );
    assert_eq!(s.state, agenda::State::Own);
    // Its start never turns the bar amber or red.
    let s = agenda::status(
        &ev,
        at("2026-11-11 11:25"),
        None,
        &[],
        true,
        true,
        &cfg,
        Chicago,
    );
    assert_eq!(s.state, agenda::State::Free);
    assert_eq!(s.free_until, Some(at("2026-11-11 13:00")));
    // And it gets no reminders.
    let sent = std::collections::HashSet::new();
    assert!(agenda::due_reminders(&ev, at("2026-11-11 11:25"), &[5], &sent, &[]).is_empty());
}

#[test]
fn edge_case_day() {
    let ev = agenda_of(EDGES, "2026-11-10 12:00");
    let cfg = Config::default();
    let today = agenda::remaining_today(&ev, at("2026-11-10 12:00"), Chicago);
    let t: Vec<&str> = today.iter().map(|e| e.title.as_str()).collect();
    assert_eq!(t, vec!["Out of office", "Sprint review"]);
    // The all-day event is never "next".
    let s = agenda::status(
        &ev,
        at("2026-11-10 12:00"),
        None,
        &[],
        true,
        true,
        &cfg,
        Chicago,
    );
    assert_eq!(s.state, agenda::State::Free);
    assert_eq!(s.free_until, Some(at("2026-11-10 14:00")));
    let j = agenda::join_target(&ev, at("2026-11-10 13:55"), &cfg, Chicago).unwrap();
    assert_eq!(j.title, "Sprint review");
}
