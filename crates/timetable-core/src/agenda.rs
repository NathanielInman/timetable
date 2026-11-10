//! The day as the bar and the drawer see it: what is on now, what is next,
//! which state the bar is in, the free gaps, the focus block a click would
//! start, which meeting Join opens and which reminders are due. All pure
//! functions of the events, the time and the config.

use crate::config::Config;
use crate::expand::{local_date, local_instant};
use crate::model::{Event, Focus};
use chrono::{Days, NaiveDate};
use chrono_tz::Tz;
use std::collections::HashSet;

/// The bar's states, from calm to urgent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Nothing within `bar.next_minutes`: the clock, maybe "free until".
    Free,
    /// The next event is within `bar.next_minutes`.
    Next,
    /// Within `bar.soon_minutes`.
    Soon,
    /// From its start for `bar.now_minutes`: time to join.
    Now,
    /// During an event.
    In,
    /// A focus block is running.
    Focus,
    /// A calendar is set but has never been read.
    Offline,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            State::Free => "free",
            State::Next => "next",
            State::Soon => "soon",
            State::Now => "now",
            State::In => "in",
            State::Focus => "focus",
            State::Offline => "offline",
        }
    }
}

/// What the bar shows and why.
#[derive(Debug, Clone, PartialEq)]
pub struct Status {
    pub state: State,
    /// The event the state is about (the next one, the current one).
    pub event: Option<Event>,
    /// While free: when the next event today starts.
    pub free_until: Option<i64>,
}

/// Timed events running at `now`, latest start first.
pub fn current(events: &[Event], now: i64) -> Vec<&Event> {
    let mut v: Vec<&Event> = events
        .iter()
        .filter(|e| e.timed() && e.running(now))
        .collect();
    v.sort_by_key(|e| std::cmp::Reverse(e.start));
    v
}

/// The first timed event starting after `now`.
pub fn upcoming(events: &[Event], now: i64) -> Option<&Event> {
    events
        .iter()
        .filter(|e| e.timed() && e.start > now)
        .min_by_key(|e| e.start)
}

/// Events overlapping a local date, all-day ones first.
pub fn on_day(events: &[Event], day: NaiveDate, zone: Tz) -> Vec<&Event> {
    let start = crate::expand::midnight(day, zone);
    let end = day
        .checked_add_days(Days::new(1))
        .map(|d| crate::expand::midnight(d, zone))
        .unwrap_or(start + 86400);
    let mut v: Vec<&Event> = events
        .iter()
        .filter(|e| e.start < end && e.end.max(e.start + 1) > start)
        .collect();
    v.sort_by_key(|e| (!e.all_day, e.start));
    v
}

/// Today's events that have not ended yet.
pub fn remaining_today(events: &[Event], now: i64, zone: Tz) -> Vec<&Event> {
    on_day(events, local_date(now, zone), zone)
        .into_iter()
        .filter(|e| e.all_day || e.end > now)
        .collect()
}

pub fn status(
    events: &[Event],
    now: i64,
    focus: Option<&Focus>,
    has_data: bool,
    configured: bool,
    cfg: &Config,
    zone: Tz,
) -> Status {
    let b = &cfg.bar;
    let cur = current(events, now);
    let next = upcoming(events, now);
    let make = |state, event: Option<&Event>| Status {
        state,
        event: event.cloned(),
        free_until: None,
    };
    if let Some(c) = cur.iter().find(|e| now < e.start + b.now_minutes * 60) {
        return make(State::Now, Some(c));
    }
    if focus.is_some_and(|f| f.running(now)) {
        return make(State::Focus, None);
    }
    if configured && !has_data {
        return make(State::Offline, None);
    }
    if let Some(n) = next.filter(|n| n.start - now <= b.soon_minutes * 60) {
        return make(State::Soon, Some(n));
    }
    if let Some(c) = cur.first() {
        return make(State::In, Some(c));
    }
    if let Some(n) = next.filter(|n| n.start - now <= b.next_minutes * 60) {
        return make(State::Next, Some(n));
    }
    let today = local_date(now, zone);
    Status {
        state: State::Free,
        event: next.cloned(),
        free_until: next
            .filter(|n| local_date(n.start, zone) == today)
            .map(|n| n.start),
    }
}

/// Whole minutes until `t`, rounded up: at 12:53:00 a 13:00 start is 7.
pub fn minutes_until(t: i64, now: i64) -> i64 {
    ((t - now).max(0) + 59) / 60
}

/// `[start, end)` intervals.
pub type Span = (i64, i64);

/// The day strip's span for a date.
pub fn day_span(day: NaiveDate, cfg: &Config, zone: Tz) -> Span {
    let (s, e) = cfg.day_span();
    (
        local_instant(day.and_time(s), zone),
        local_instant(day.and_time(e), zone),
    )
}

/// Busy time in a span: timed events merged where they overlap.
pub fn busy(events: &[Event], span: Span) -> Vec<Span> {
    let mut v: Vec<Span> = events
        .iter()
        .filter(|e| e.timed() && e.end > span.0 && e.start < span.1)
        .map(|e| (e.start.max(span.0), e.end.min(span.1)))
        .collect();
    v.sort_unstable();
    let mut out: Vec<Span> = vec![];
    for (s, e) in v {
        match out.last_mut() {
            Some(last) if s <= last.1 => last.1 = last.1.max(e),
            _ => out.push((s, e)),
        }
    }
    out
}

/// Free gaps in a span at least `min_gap` minutes long.
pub fn gaps(events: &[Event], span: Span, min_gap: i64) -> Vec<Span> {
    let mut out = vec![];
    let mut at = span.0;
    for (s, e) in busy(events, span) {
        if s - at >= min_gap * 60 {
            out.push((at, s));
        }
        at = at.max(e);
    }
    if span.1 - at >= min_gap * 60 {
        out.push((at, span.1));
    }
    out
}

/// What starting a focus block now would do.
#[derive(Debug, Clone, PartialEq)]
pub struct FocusPlan {
    pub start: i64,
    pub end: i64,
    /// The event it stops for, when one is coming today.
    pub before: Option<Event>,
}

impl FocusPlan {
    pub fn minutes(&self) -> i64 {
        (self.end - self.start) / 60
    }
}

/// A block from `now` until `focus.lead` minutes before the next event
/// today, or `focus.minutes` long when nothing else is on today. `Err`
/// says why no block fits.
pub fn focus_plan(events: &[Event], now: i64, cfg: &Config, zone: Tz) -> Result<FocusPlan, String> {
    let f = &cfg.focus;
    let today = local_date(now, zone);
    let next = upcoming(events, now).filter(|n| local_date(n.start, zone) == today);
    let mut end = match next {
        Some(n) => n.start - f.lead * 60,
        None => now + f.minutes * 60,
    };
    if f.max > 0 {
        end = end.min(now + f.max * 60);
    }
    // Whole minutes, so the block ends on the clock's minute.
    let end = now + (end - now).div_euclid(60) * 60;
    if end - now < f.min * 60 {
        return Err(match next {
            Some(n) => format!(
                "{} starts in {} min, too soon for a focus block",
                n.title_or_default(),
                minutes_until(n.start, now)
            ),
            None => "a focus block needs at least focus.min minutes".into(),
        });
    }
    Ok(FocusPlan {
        start: now,
        end,
        before: next.cloned(),
    })
}

/// A running block that a later-added event now cuts into ends earlier.
pub fn clamp_focus(focus: Focus, events: &[Event], now: i64, cfg: &Config) -> Focus {
    let lead = cfg.focus.lead * 60;
    let end = events
        .iter()
        .filter(|e| e.timed() && e.start > focus.start + lead && e.start - lead < focus.end)
        .map(|e| e.start - lead)
        .min()
        .map(|t| t.max(now.min(focus.end)))
        .unwrap_or(focus.end);
    Focus { end, ..focus }
}

/// The meeting Join opens: the one running (or starting within
/// `bar.soon_minutes`) with a link, latest start first, else the next one
/// today with a link.
pub fn join_target<'a>(events: &'a [Event], now: i64, cfg: &Config, zone: Tz) -> Option<&'a Event> {
    let soon = cfg.bar.soon_minutes * 60;
    let mut live: Vec<&Event> = events
        .iter()
        .filter(|e| e.timed() && e.link.is_some() && e.start - soon <= now && now < e.end)
        .collect();
    // A meeting about to start beats the tail of the one before it.
    live.sort_by_key(|e| {
        let starting = e.start > now;
        (std::cmp::Reverse(starting), std::cmp::Reverse(e.start))
    });
    if let Some(e) = live.first() {
        return Some(e);
    }
    let today = local_date(now, zone);
    events
        .iter()
        .filter(|e| {
            e.timed() && e.link.is_some() && e.start > now && local_date(e.start, zone) == today
        })
        .min_by_key(|e| e.start)
}

/// A reminder that should go out now.
#[derive(Debug, Clone, PartialEq)]
pub struct Reminder {
    pub key: String,
    pub event: Event,
    pub minutes: i64,
}

/// Reminders whose time has come in the last 90 seconds and that were not
/// sent yet. Late ones (the machine was asleep) are skipped, not sent late.
pub fn due_reminders(
    events: &[Event],
    now: i64,
    minutes: &[i64],
    sent: &HashSet<String>,
) -> Vec<Reminder> {
    let mut out = vec![];
    for e in events.iter().filter(|e| e.timed()) {
        for &m in minutes {
            let at = e.start - m * 60;
            let key = format!("{}#{m}", e.key());
            if now >= at && now < at + 90 && !sent.contains(&key) {
                out.push(Reminder {
                    key,
                    event: e.clone(),
                    minutes: m,
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDateTime;
    use chrono_tz::America::Chicago;

    fn at(s: &str) -> i64 {
        local_instant(
            NaiveDateTime::parse_from_str(&format!("2026-11-10 {s}"), "%Y-%m-%d %H:%M").unwrap(),
            Chicago,
        )
    }

    fn ev(title: &str, from: &str, to: &str, link: bool) -> Event {
        Event {
            uid: title.to_lowercase(),
            title: title.into(),
            start: at(from),
            end: at(to),
            link: link.then(|| "https://meet.google.com/abc-defg-hij".into()),
            kind: link.then(|| "meet".into()),
            ..Default::default()
        }
    }

    fn day() -> Vec<Event> {
        vec![
            Event {
                uid: "h".into(),
                title: "Holiday".into(),
                start: at("00:00"),
                end: at("00:00") + 86400,
                all_day: true,
                ..Default::default()
            },
            ev("1:1", "09:30", "10:00", true),
            ev("Standup", "13:00", "13:15", true),
            ev("Sprint review", "14:00", "15:00", true),
            ev("Interview", "16:30", "17:00", false),
        ]
    }

    fn st(now: &str) -> (State, Option<String>) {
        let s = status(
            &day(),
            at(now),
            None,
            true,
            true,
            &Config::default(),
            Chicago,
        );
        (s.state, s.event.map(|e| e.title))
    }

    #[test]
    fn states_through_the_day() {
        assert_eq!(st("11:00").0, State::Free);
        assert_eq!(st("12:10"), (State::Next, Some("Standup".into())));
        assert_eq!(st("12:53"), (State::Soon, Some("Standup".into())));
        assert_eq!(st("13:00"), (State::Now, Some("Standup".into())));
        assert_eq!(st("13:04"), (State::Now, Some("Standup".into())));
        assert_eq!(st("13:06"), (State::In, Some("Standup".into())));
        // Inside the review, the interview is far off.
        assert_eq!(st("14:30"), (State::In, Some("Sprint review".into())));
        assert_eq!(st("17:30").0, State::Free);
    }

    #[test]
    fn free_until_and_all_day_never_next() {
        let s = status(
            &day(),
            at("10:30"),
            None,
            true,
            true,
            &Config::default(),
            Chicago,
        );
        assert_eq!(s.state, State::Free);
        assert_eq!(s.free_until, Some(at("13:00")));
        let s = status(
            &day(),
            at("17:30"),
            None,
            true,
            true,
            &Config::default(),
            Chicago,
        );
        assert_eq!(s.free_until, None);
        assert!(upcoming(&day(), at("00:00") - 3600).is_some_and(|e| !e.all_day));
    }

    #[test]
    fn focus_offline_and_priority() {
        let f = Focus {
            start: at("10:00"),
            end: at("12:58"),
        };
        let s = status(
            &day(),
            at("12:55"),
            Some(&f),
            true,
            true,
            &Config::default(),
            Chicago,
        );
        assert_eq!(s.state, State::Focus);
        // A meeting starting beats a focus block that overran it.
        let f = Focus {
            start: at("12:00"),
            end: at("14:00"),
        };
        let s = status(
            &day(),
            at("13:01"),
            Some(&f),
            true,
            true,
            &Config::default(),
            Chicago,
        );
        assert_eq!(s.state, State::Now);
        let s = status(
            &[],
            at("12:00"),
            None,
            false,
            true,
            &Config::default(),
            Chicago,
        );
        assert_eq!(s.state, State::Offline);
        let s = status(
            &[],
            at("12:00"),
            None,
            false,
            false,
            &Config::default(),
            Chicago,
        );
        assert_eq!(s.state, State::Free);
    }

    #[test]
    fn rounding() {
        assert_eq!(minutes_until(at("13:00"), at("12:53")), 7);
        assert_eq!(minutes_until(at("13:00"), at("12:53") + 30), 7);
        assert_eq!(minutes_until(at("13:00"), at("13:00")), 0);
    }

    #[test]
    fn gaps_between_meetings() {
        let span = day_span(
            local_date(at("12:00"), Chicago),
            &Config::default(),
            Chicago,
        );
        let g = gaps(&day(), span, 15);
        assert_eq!(
            g,
            vec![
                (at("08:00"), at("09:30")),
                (at("10:00"), at("13:00")),
                (at("13:15"), at("14:00")),
                (at("15:00"), at("16:30")),
                (at("17:00"), at("18:00")),
            ]
        );
        // Overlaps merge.
        let mut d = day();
        d.push(ev("Overlap", "14:30", "15:20", false));
        let g = gaps(&d, span, 15);
        assert!(g.contains(&(at("15:20"), at("16:30"))));
    }

    #[test]
    fn focus_blocks_fit_the_gap() {
        let cfg = Config::default();
        let p = focus_plan(&day(), at("15:00"), &cfg, Chicago).unwrap();
        assert_eq!(p.end, at("16:28"));
        assert_eq!(p.minutes(), 88);
        assert_eq!(p.before.unwrap().title, "Interview");
        // Nothing left today: the default length.
        let p = focus_plan(&day(), at("17:10"), &cfg, Chicago).unwrap();
        assert_eq!(p.minutes(), 50);
        // Too close to the next one.
        assert!(focus_plan(&day(), at("12:55"), &cfg, Chicago).is_err());
        let mut cfg = Config::default();
        cfg.focus.max = 30;
        assert_eq!(
            focus_plan(&day(), at("15:00"), &cfg, Chicago)
                .unwrap()
                .minutes(),
            30
        );
    }

    #[test]
    fn a_new_event_cuts_a_block_short() {
        let cfg = Config::default();
        let f = Focus {
            start: at("15:00"),
            end: at("16:28"),
        };
        let mut d = day();
        assert_eq!(clamp_focus(f, &d, at("15:10"), &cfg), f);
        d.push(ev("Surprise", "15:45", "16:00", false));
        assert_eq!(clamp_focus(f, &d, at("15:10"), &cfg).end, at("15:43"));
    }

    #[test]
    fn join_prefers_the_meeting_about_to_start() {
        let cfg = Config::default();
        let t = |now: &str| join_target(&day(), at(now), &cfg, Chicago).map(|e| e.title.clone());
        assert_eq!(t("09:45").as_deref(), Some("1:1"));
        assert_eq!(t("11:00").as_deref(), Some("Standup"));
        assert_eq!(t("13:55").as_deref(), Some("Sprint review"));
        // The interview has no link and nothing with one is left.
        assert_eq!(t("16:40"), None);
    }

    #[test]
    fn reminders_once_and_never_late() {
        let d = day();
        let mut sent = HashSet::new();
        let r = due_reminders(&d, at("12:55"), &[5, 1], &sent);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].minutes, 5);
        assert_eq!(r[0].event.title, "Standup");
        sent.insert(r[0].key.clone());
        assert!(due_reminders(&d, at("12:55") + 30, &[5, 1], &sent).is_empty());
        assert_eq!(due_reminders(&d, at("12:59"), &[5, 1], &sent)[0].minutes, 1);
        // Woken at 12:57: the five-minute one is not sent late.
        assert!(due_reminders(&d, at("12:57"), &[5, 1], &HashSet::new()).is_empty());
    }
}
