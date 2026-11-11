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
    /// During your own time (`Event::own`) and nothing else: relaxed.
    Own,
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
            State::Own => "own",
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

/// The first meeting starting after `now`: your own time is never next.
pub fn upcoming(events: &[Event], now: i64) -> Option<&Event> {
    events
        .iter()
        .filter(|e| e.meeting() && e.start > now)
        .min_by_key(|e| e.start)
}

/// Marks events whose title contains one of `titles` (any case) as your
/// own time, on top of what the feed says.
pub fn mark_own(events: &mut [Event], titles: &[String]) {
    let titles: Vec<String> = titles
        .iter()
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty())
        .collect();
    if titles.is_empty() {
        return;
    }
    for e in events.iter_mut() {
        let t = e.title.to_lowercase();
        if titles.iter().any(|p| t.contains(p.as_str())) {
            e.own = true;
        }
    }
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

/// What the bar shows. `joined` holds the keys of events Join opened: they
/// are never `soon` or `now`, since the meeting is already open. Your own
/// time is never `soon`, `now`, `in` or `next`; it is `own` when nothing
/// else is going on.
#[allow(clippy::too_many_arguments)]
pub fn status(
    events: &[Event],
    now: i64,
    focus: Option<&Focus>,
    joined: &[String],
    has_data: bool,
    configured: bool,
    cfg: &Config,
    zone: Tz,
) -> Status {
    let b = &cfg.bar;
    let all = current(events, now);
    let cur: Vec<&Event> = all.iter().copied().filter(|e| !e.own).collect();
    let next = upcoming(events, now);
    let make = |state, event: Option<&Event>| Status {
        state,
        event: event.cloned(),
        free_until: None,
    };
    let open = |e: &Event| joined.contains(&e.key());
    if let Some(c) = cur
        .iter()
        .find(|e| now < e.start + b.now_minutes * 60 && !open(e))
    {
        return make(State::Now, Some(c));
    }
    if focus.is_some_and(|f| f.running(now)) {
        return make(State::Focus, None);
    }
    if configured && !has_data {
        return make(State::Offline, None);
    }
    if let Some(n) = next.filter(|n| n.start - now <= b.soon_minutes * 60 && !open(n)) {
        return make(State::Soon, Some(n));
    }
    if let Some(c) = cur.first() {
        return make(State::In, Some(c));
    }
    if let Some(c) = all.iter().find(|e| e.own) {
        return make(State::Own, Some(c));
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

/// Side-by-side lanes for overlapping events, given sorted by start: for
/// each event its lane and how many lanes its overlapping group needs. An
/// event that overlaps nothing is lane 0 of 1.
pub fn lanes(events: &[&Event]) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::with_capacity(events.len());
    let mut ends: Vec<i64> = vec![];
    let mut group_from = 0;
    let mut group_end = i64::MIN;
    for (i, e) in events.iter().enumerate() {
        let end = e.end.max(e.start + 1);
        if e.start >= group_end {
            let n = ends.len().max(1);
            for o in &mut out[group_from..i] {
                o.1 = n;
            }
            ends.clear();
            group_from = i;
        }
        let lane = match ends.iter().position(|&x| x <= e.start) {
            Some(l) => {
                ends[l] = end;
                l
            }
            None => {
                ends.push(end);
                ends.len() - 1
            }
        };
        group_end = group_end.max(end);
        out.push((lane, 0));
    }
    let n = ends.len().max(1);
    for o in &mut out[group_from..] {
        o.1 = n;
    }
    out
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

/// Busy time in a span: meetings merged where they overlap. Your own time
/// is free.
pub fn busy(events: &[Event], span: Span) -> Vec<Span> {
    busy_where(events, span, Event::meeting)
}

fn busy_where(events: &[Event], span: Span, counts: impl Fn(&Event) -> bool) -> Vec<Span> {
    let mut v: Vec<Span> = events
        .iter()
        .filter(|e| counts(e) && e.end > span.0 && e.start < span.1)
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

/// Free gaps in a span at least `min_gap` minutes long. Your own time
/// counts as free.
pub fn gaps(events: &[Event], span: Span, min_gap: i64) -> Vec<Span> {
    gaps_in(busy(events, span), span, min_gap)
}

/// Gaps between all timed events, your own time included: where the day
/// strip draws nothing.
pub fn empty_gaps(events: &[Event], span: Span, min_gap: i64) -> Vec<Span> {
    gaps_in(busy_where(events, span, Event::timed), span, min_gap)
}

fn gaps_in(busy: Vec<Span>, span: Span, min_gap: i64) -> Vec<Span> {
    let mut out = vec![];
    let mut at = span.0;
    for (s, e) in busy {
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
    /// Its length in whole minutes.
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
    // On a whole minute, so the block ends when the clock turns: the
    // length rounds up to it, a deadline before an event rounds down.
    let up = |t: i64| (t + 59).div_euclid(60) * 60;
    let mut end = match next {
        Some(n) => (n.start - f.lead * 60).div_euclid(60) * 60,
        None => up(now + f.minutes * 60),
    };
    if f.max > 0 && end > now + f.max * 60 {
        end = up(now + f.max * 60);
    }
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
        .filter(|e| e.meeting() && e.start > focus.start + lead && e.start - lead < focus.end)
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
/// sent yet. Late ones (the machine was asleep) are skipped, not sent late,
/// and so are those for a meeting already joined and for your own time.
pub fn due_reminders(
    events: &[Event],
    now: i64,
    minutes: &[i64],
    sent: &HashSet<String>,
    joined: &[String],
) -> Vec<Reminder> {
    let mut out = vec![];
    for e in events
        .iter()
        .filter(|e| e.meeting() && !joined.contains(&e.key()))
    {
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
            &[],
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
    fn a_joined_meeting_stops_calling() {
        let d = day();
        let standup = vec![format!("standup@{}", at("13:00"))];
        let st = |now: &str| {
            let s = status(
                &d,
                at(now),
                None,
                &standup,
                true,
                true,
                &Config::default(),
                Chicago,
            );
            (s.state, s.event.map(|e| e.title))
        };
        // Joined early: a calm countdown, not amber.
        assert_eq!(st("12:57"), (State::Next, Some("Standup".into())));
        // Never red: running, with the time left.
        assert_eq!(st("13:00"), (State::In, Some("Standup".into())));
        assert_eq!(st("13:04"), (State::In, Some("Standup".into())));
        // The next meeting still calls.
        assert_eq!(st("13:55"), (State::Soon, Some("Sprint review".into())));
        assert_eq!(st("14:00"), (State::Now, Some("Sprint review".into())));
        // And no reminder for the one already open.
        assert!(due_reminders(&d, at("12:59"), &[5, 1], &HashSet::new(), &standup).is_empty());
    }

    #[test]
    fn free_until_and_all_day_never_next() {
        let s = status(
            &day(),
            at("10:30"),
            None,
            &[],
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
            &[],
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
            &[],
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
            &[],
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
            &[],
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
            &[],
            false,
            false,
            &Config::default(),
            Chicago,
        );
        assert_eq!(s.state, State::Free);
    }

    #[test]
    fn overlapping_events_share_lanes() {
        let d = [
            ev("All hands", "08:00", "09:00", true),
            ev("Stand-up", "08:30", "09:00", true),
            ev("Sprint", "09:00", "10:00", true),
            ev("A", "11:00", "12:00", false),
            ev("B", "11:15", "11:30", false),
            ev("C", "11:30", "11:45", false),
            ev("D", "11:40", "12:30", false),
        ];
        let refs: Vec<&Event> = d.iter().collect();
        assert_eq!(
            lanes(&refs),
            vec![(0, 2), (1, 2), (0, 1), (0, 3), (1, 3), (1, 3), (2, 3)]
        );
        assert!(lanes(&[]).is_empty());
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
        // Started mid-minute, it still runs the whole length.
        let p = focus_plan(&day(), at("17:10") + 40, &cfg, Chicago).unwrap();
        assert_eq!((p.minutes(), p.end), (50, at("18:01")));
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
        let r = due_reminders(&d, at("12:55"), &[5, 1], &sent, &[]);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].minutes, 5);
        assert_eq!(r[0].event.title, "Standup");
        sent.insert(r[0].key.clone());
        assert!(due_reminders(&d, at("12:55") + 30, &[5, 1], &sent, &[]).is_empty());
        assert_eq!(
            due_reminders(&d, at("12:59"), &[5, 1], &sent, &[])[0].minutes,
            1
        );
        // Woken at 12:57: the five-minute one is not sent late.
        assert!(due_reminders(&d, at("12:57"), &[5, 1], &HashSet::new(), &[]).is_empty());
    }
}
