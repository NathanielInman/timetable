//! Places a feed's events in time: recurrence (`RRULE` through the `rrule`
//! crate, plus `RDATE` and `EXDATE`), moved and cancelled instances
//! (`RECURRENCE-ID`), zones, floating and all-day times, and the owner's
//! declines. Only occurrences that overlap a window are produced, so years
//! of history cost one walk through each rule and nothing more.

use crate::ical::{Feed, RawEvent, When};
use crate::model::Event;
use chrono::{DateTime, Days, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use std::collections::{HashMap, HashSet};

/// Most occurrences one rule may walk through before the window ends. A
/// daily rule from ten years ago is under 4000.
const MAX_STEPS: usize = 50_000;

/// What expanding produced.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Expanded {
    pub events: Vec<Event>,
    /// Rules the `rrule` crate refused; their events show once, at DTSTART.
    pub bad_rules: usize,
}

/// A local wall-clock time as an instant. A time skipped by a DST change
/// moves forward an hour; a repeated one takes the first.
pub fn local_instant(t: NaiveDateTime, zone: Tz) -> i64 {
    match zone.from_local_datetime(&t).earliest() {
        Some(d) => d.timestamp(),
        None => zone
            .from_local_datetime(&(t + chrono::Duration::hours(1)))
            .earliest()
            .map(|d| d.timestamp())
            .unwrap_or_else(|| t.and_utc().timestamp()),
    }
}

/// Local midnight starting a date.
pub fn midnight(d: NaiveDate, zone: Tz) -> i64 {
    local_instant(d.and_time(NaiveTime::MIN), zone)
}

/// The instant a value names; dates and floating times are local.
pub fn instant(w: &When, local: Tz) -> i64 {
    match *w {
        When::Date(d) => midnight(d, local),
        When::Utc(t) => t.and_utc().timestamp(),
        When::Zoned(t, z) => local_instant(t, z),
        When::Floating(t) => local_instant(t, local),
    }
}

/// The local date an instant falls on.
pub fn local_date(t: i64, zone: Tz) -> NaiveDate {
    DateTime::<Utc>::from_timestamp(t, 0)
        .unwrap_or_default()
        .with_timezone(&zone)
        .date_naive()
}

fn rrule_tz(w: &When, local: Tz) -> rrule::Tz {
    match *w {
        When::Utc(_) => rrule::Tz::UTC,
        When::Zoned(_, z) => rrule::Tz::Tz(z),
        When::Date(_) | When::Floating(_) => rrule::Tz::Tz(local),
    }
}

/// Splits `UNTIL=` out of a rule, since the crate insists it be UTC when
/// DTSTART has a zone and feeds do not always comply.
fn split_until(rule: &str) -> (String, Option<String>) {
    let mut until = None;
    let rest: Vec<&str> = rule
        .split(';')
        .filter(|part| match part.split_once('=') {
            Some((k, v)) if k.trim().eq_ignore_ascii_case("UNTIL") => {
                until = Some(v.trim().to_string());
                false
            }
            _ => !part.trim().is_empty(),
        })
        .collect();
    (rest.join(";"), until)
}

/// `UNTIL` as an instant: a `Z` time, a date (through the end of that day
/// in the series' zone), or a local time in the series' zone.
fn until_instant(v: &str, start: &When, local: Tz) -> Option<i64> {
    let zone = match *start {
        When::Zoned(_, z) => z,
        When::Utc(_) => chrono_tz::UTC,
        _ => local,
    };
    if v.len() == 8 {
        let d = NaiveDate::parse_from_str(v, "%Y%m%d").ok()?;
        return Some(midnight(d.checked_add_days(Days::new(1))?, zone) - 1);
    }
    let t = NaiveDateTime::parse_from_str(v.get(..15)?, "%Y%m%dT%H%M%S").ok()?;
    if v.ends_with('Z') {
        Some(t.and_utc().timestamp())
    } else {
        Some(local_instant(t, zone))
    }
}

/// Occurrence starts of a recurring event up to `to`, or `None` when the
/// rule cannot be read.
fn rule_starts(e: &RawEvent, rule: &str, local: Tz, from: i64, to: i64) -> Option<Vec<i64>> {
    let start = e.dtstart?;
    let tz = rrule_tz(&start, local);
    let naive = start.naive();
    let dt_start = match tz {
        rrule::Tz::Tz(z) => {
            let t = local_instant(naive, z);
            DateTime::<Utc>::from_timestamp(t, 0)?.with_timezone(&tz)
        }
        rrule::Tz::Local(_) => return None,
    };
    let (rest, until) = split_until(rule);
    let mut r: rrule::RRule<rrule::Unvalidated> = rest.parse().ok()?;
    if let Some(u) = until {
        let u = until_instant(&u, &start, local)?;
        if u < dt_start.timestamp() {
            // Ends before it starts: only DTSTART itself.
            return Some(vec![dt_start.timestamp()]);
        }
        r = r.until(DateTime::<Utc>::from_timestamp(u, 0)?.with_timezone(&rrule::Tz::UTC));
    }
    let set = r.build(dt_start).ok()?;
    let mut out = vec![];
    for (n, dt) in set.into_iter().enumerate() {
        let t = dt.timestamp();
        if t >= to || n > MAX_STEPS {
            break;
        }
        if t >= from {
            out.push(t);
        }
    }
    Some(out)
}

/// The occurrence's length: all-day events in whole days, timed ones in
/// seconds.
enum Length {
    Days(u64),
    Secs(i64),
}

fn length(e: &RawEvent, local: Tz) -> Length {
    let Some(start) = e.dtstart else {
        return Length::Secs(0);
    };
    if let When::Date(d) = start {
        let days = match (e.dtend, e.duration) {
            (Some(When::Date(end)), _) => (end - d).num_days(),
            (_, Some(secs)) => secs / 86400,
            _ => 1,
        };
        return Length::Days(days.max(1) as u64);
    }
    match (e.dtend, e.duration) {
        (Some(end), _) => Length::Secs((instant(&end, local) - instant(&start, local)).max(0)),
        (None, Some(secs)) => Length::Secs(secs.max(0)),
        (None, None) => Length::Secs(0),
    }
}

fn occurrence(
    e: &RawEvent,
    start: i64,
    len: &Length,
    local: Tz,
    fallback: Option<&RawEvent>,
) -> Event {
    let (start, end, all_day) = match *len {
        Length::Days(n) => {
            let d = local_date(start, local);
            let end = d.checked_add_days(Days::new(n)).unwrap_or(d);
            (midnight(d, local), midnight(end, local), true)
        }
        Length::Secs(s) => (start, start + s, false),
    };
    let pick = |own: &str, other: Option<&str>| {
        if own.is_empty() {
            other.unwrap_or_default().to_string()
        } else {
            own.to_string()
        }
    };
    let (link, kind) = match (&e.link, fallback) {
        (Some(l), _) => (Some(l.clone()), e.kind.clone()),
        (None, Some(m)) => (m.link.clone(), m.kind.clone()),
        (None, None) => (None, None),
    };
    Event {
        uid: if e.uid.is_empty() {
            format!("{}-{}", e.summary, start)
        } else {
            e.uid.clone()
        },
        title: pick(&e.summary, fallback.map(|m| m.summary.as_str())),
        start,
        end,
        all_day,
        location: pick(&e.location, fallback.map(|m| m.location.as_str())),
        link,
        kind,
    }
}

fn overlaps(ev: &Event, from: i64, to: i64) -> bool {
    ev.end.max(ev.start + 1) > from && ev.start < to
}

/// Every occurrence overlapping `[from, to)`, sorted by start.
pub fn expand(feed: &Feed, local: Tz, from: i64, to: i64) -> Expanded {
    let mut out = Expanded::default();
    // Moved or cancelled instances, by series and original start.
    let mut overridden: HashMap<&str, HashSet<i64>> = HashMap::new();
    let mut masters: HashMap<&str, &RawEvent> = HashMap::new();
    for e in &feed.events {
        match &e.recurrence_id {
            Some(r) => {
                overridden
                    .entry(e.uid.as_str())
                    .or_default()
                    .insert(instant(r, local));
            }
            None => {
                masters.insert(e.uid.as_str(), e);
            }
        }
    }

    for e in &feed.events {
        let Some(start) = e.dtstart else { continue };
        let len = length(e, local);
        let longest = match len {
            Length::Days(n) => n as i64 * 86400 + 7200,
            Length::Secs(s) => s,
        };
        let skip = e.status == "CANCELLED" || e.declined;

        if e.recurrence_id.is_some() {
            // An instance moved, renamed or cancelled: it stands on its own.
            if skip {
                continue;
            }
            let ev = occurrence(
                e,
                instant(&start, local),
                &len,
                local,
                masters.get(e.uid.as_str()).copied(),
            );
            if overlaps(&ev, from, to) {
                out.events.push(ev);
            }
            continue;
        }
        if skip {
            continue;
        }

        let first = instant(&start, local);
        let mut starts: Vec<i64> = match &e.rrule {
            Some(rule) => match rule_starts(e, rule, local, from - longest, to) {
                Some(s) => s,
                None => {
                    out.bad_rules += 1;
                    vec![first]
                }
            },
            None => vec![first],
        };
        starts.extend(e.rdates.iter().map(|r| instant(r, local)));
        if starts.is_empty() {
            continue;
        }
        let ex: HashSet<i64> = e.exdates.iter().map(|x| instant(x, local)).collect();
        let ex_dates: HashSet<NaiveDate> = e
            .exdates
            .iter()
            .filter_map(|x| match x {
                When::Date(d) => Some(*d),
                _ => None,
            })
            .collect();
        let moved = overridden.get(e.uid.as_str());
        starts.sort_unstable();
        starts.dedup();
        for s in starts {
            if ex.contains(&s) || ex_dates.contains(&local_date(s, local)) {
                continue;
            }
            if moved.is_some_and(|m| m.contains(&s)) {
                continue;
            }
            let ev = occurrence(e, s, &len, local, None);
            if overlaps(&ev, from, to) {
                out.events.push(ev);
            }
        }
    }
    out.events.sort_by(|a, b| {
        (a.start, !a.all_day, &a.title, &a.uid).cmp(&(b.start, !b.all_day, &b.title, &b.uid))
    });
    out
}

/// The window an agenda covers around `now`: from local midnight `past`
/// days back to local midnight `future` days ahead.
pub fn window(now: i64, local: Tz, past: u32, future: u32) -> (i64, i64) {
    let today = local_date(now, local);
    let from = today
        .checked_sub_days(Days::new(past as u64))
        .unwrap_or(today);
    let to = today
        .checked_add_days(Days::new(future as u64 + 1))
        .unwrap_or(today);
    (midnight(from, local), midnight(to, local))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ical::parse;
    use chrono_tz::America::Chicago;

    fn at(s: &str) -> i64 {
        let t = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap();
        local_instant(t, Chicago)
    }

    fn cal(body: &str) -> String {
        format!("BEGIN:VCALENDAR\nVERSION:2.0\n{body}\nEND:VCALENDAR\n")
    }

    fn titles(x: &Expanded) -> Vec<(String, String)> {
        x.events
            .iter()
            .map(|e| {
                let t = DateTime::<Utc>::from_timestamp(e.start, 0)
                    .unwrap()
                    .with_timezone(&Chicago);
                (t.format("%m-%d %H:%M").to_string(), e.title.clone())
            })
            .collect()
    }

    #[test]
    fn weekly_rule_with_exdate_and_override() {
        let f = parse(
            &cal("BEGIN:VEVENT\nUID:standup\nSUMMARY:Standup\nDTSTART;TZID=America/Chicago:20261102T093000\nDTEND;TZID=America/Chicago:20261102T094500\nRRULE:FREQ=WEEKLY;UNTIL=20261231T000000Z;BYDAY=MO,TU,WE,TH,FR\nEXDATE;TZID=America/Chicago:20261111T093000\nEND:VEVENT\nBEGIN:VEVENT\nUID:standup\nSUMMARY:Standup (moved)\nRECURRENCE-ID;TZID=America/Chicago:20261112T093000\nDTSTART;TZID=America/Chicago:20261112T110000\nDTEND;TZID=America/Chicago:20261112T111500\nEND:VEVENT\nBEGIN:VEVENT\nUID:standup\nRECURRENCE-ID;TZID=America/Chicago:20261113T093000\nDTSTART;TZID=America/Chicago:20261113T093000\nSTATUS:CANCELLED\nEND:VEVENT"),
            None,
        );
        let x = expand(&f, Chicago, at("2026-11-10 00:00"), at("2026-11-17 00:00"));
        assert_eq!(
            titles(&x),
            vec![
                ("11-10 09:30".into(), "Standup".into()),
                ("11-12 11:00".into(), "Standup (moved)".into()),
                ("11-16 09:30".into(), "Standup".into()),
            ]
        );
        assert_eq!(x.events[0].end - x.events[0].start, 15 * 60);
    }

    #[test]
    fn other_zones_follow_their_own_dst() {
        // New York 10:00 is Chicago 09:00 on both sides of a DST change.
        let f = parse(
            &cal("BEGIN:VEVENT\nUID:ny\nSUMMARY:NY sync\nDTSTART;TZID=America/New_York:20261026T100000\nDTEND;TZID=America/New_York:20261026T103000\nRRULE:FREQ=WEEKLY;BYDAY=MO\nEND:VEVENT"),
            None,
        );
        let x = expand(&f, Chicago, at("2026-10-26 00:00"), at("2026-11-10 00:00"));
        assert_eq!(
            titles(&x),
            vec![
                ("10-26 09:00".into(), "NY sync".into()),
                ("11-02 09:00".into(), "NY sync".into()),
                ("11-09 09:00".into(), "NY sync".into()),
            ]
        );
    }

    #[test]
    fn utc_floating_and_all_day() {
        let f = parse(
            &cal("BEGIN:VEVENT\nUID:u\nSUMMARY:UTC\nDTSTART:20261110T180000Z\nDTEND:20261110T183000Z\nEND:VEVENT\nBEGIN:VEVENT\nUID:f\nSUMMARY:Floating\nDTSTART:20261110T080000\nDURATION:PT45M\nEND:VEVENT\nBEGIN:VEVENT\nUID:a\nSUMMARY:Holiday\nDTSTART;VALUE=DATE:20261110\nDTEND;VALUE=DATE:20261112\nEND:VEVENT"),
            None,
        );
        let x = expand(&f, Chicago, at("2026-11-10 00:00"), at("2026-11-11 00:00"));
        assert_eq!(
            titles(&x),
            vec![
                ("11-10 00:00".into(), "Holiday".into()),
                ("11-10 08:00".into(), "Floating".into()),
                ("11-10 12:00".into(), "UTC".into()),
            ]
        );
        assert!(x.events[0].all_day);
        assert_eq!(x.events[0].end, at("2026-11-12 00:00"));
        assert_eq!(x.events[1].end - x.events[1].start, 45 * 60);
    }

    #[test]
    fn cancelled_and_declined_are_dropped() {
        let f = parse(
            &cal("X-WR-CALNAME:me@example.com\nBEGIN:VEVENT\nUID:c\nSUMMARY:Gone\nSTATUS:CANCELLED\nDTSTART:20261110T180000Z\nEND:VEVENT\nBEGIN:VEVENT\nUID:d\nSUMMARY:Declined\nDTSTART:20261110T190000Z\nATTENDEE;PARTSTAT=DECLINED:mailto:me@example.com\nEND:VEVENT\nBEGIN:VEVENT\nUID:k\nSUMMARY:Kept\nDTSTART:20261110T200000Z\nATTENDEE;PARTSTAT=DECLINED:mailto:you@example.com\nEND:VEVENT"),
            None,
        );
        let x = expand(&f, Chicago, at("2026-11-10 00:00"), at("2026-11-11 00:00"));
        assert_eq!(titles(&x), vec![("11-10 14:00".into(), "Kept".into())]);
    }

    #[test]
    fn declining_one_instance_keeps_the_series() {
        let f = parse(
            &cal("X-WR-CALNAME:me@example.com\nBEGIN:VEVENT\nUID:s\nSUMMARY:Sync\nDTSTART;TZID=America/Chicago:20261109T130000\nDTEND;TZID=America/Chicago:20261109T133000\nRRULE:FREQ=DAILY;COUNT=3\nEND:VEVENT\nBEGIN:VEVENT\nUID:s\nSUMMARY:Sync\nRECURRENCE-ID;TZID=America/Chicago:20261110T130000\nDTSTART;TZID=America/Chicago:20261110T130000\nDTEND;TZID=America/Chicago:20261110T133000\nATTENDEE;PARTSTAT=DECLINED:mailto:me@example.com\nEND:VEVENT"),
            None,
        );
        let x = expand(&f, Chicago, at("2026-11-09 00:00"), at("2026-11-12 00:00"));
        assert_eq!(
            titles(&x),
            vec![
                ("11-09 13:00".into(), "Sync".into()),
                ("11-11 13:00".into(), "Sync".into())
            ]
        );
    }

    #[test]
    fn until_in_local_time_and_dates() {
        // A local UNTIL and an all-day series ending on a date.
        let f = parse(
            &cal("BEGIN:VEVENT\nUID:l\nSUMMARY:Local until\nDTSTART;TZID=America/Chicago:20261109T100000\nRRULE:FREQ=DAILY;UNTIL=20261110T100000\nEND:VEVENT\nBEGIN:VEVENT\nUID:d\nSUMMARY:Trip\nDTSTART;VALUE=DATE:20261109\nRRULE:FREQ=DAILY;UNTIL=20261110\nEND:VEVENT"),
            None,
        );
        let x = expand(&f, Chicago, at("2026-11-09 00:00"), at("2026-11-13 00:00"));
        assert_eq!(
            titles(&x),
            vec![
                ("11-09 00:00".into(), "Trip".into()),
                ("11-09 10:00".into(), "Local until".into()),
                ("11-10 00:00".into(), "Trip".into()),
                ("11-10 10:00".into(), "Local until".into()),
            ]
        );
    }

    #[test]
    fn an_old_series_still_reaches_today() {
        let f = parse(
            &cal("BEGIN:VEVENT\nUID:o\nSUMMARY:Old daily\nDTSTART:20160104T150000Z\nDTEND:20160104T151500Z\nRRULE:FREQ=DAILY\nEND:VEVENT"),
            None,
        );
        let x = expand(&f, Chicago, at("2026-11-10 00:00"), at("2026-11-11 00:00"));
        assert_eq!(titles(&x), vec![("11-10 09:00".into(), "Old daily".into())]);
    }

    #[test]
    fn a_bad_rule_shows_once() {
        let f = parse(
            &cal("BEGIN:VEVENT\nUID:b\nSUMMARY:Odd\nDTSTART:20261110T150000Z\nRRULE:FREQ=SOMETIMES\nEND:VEVENT"),
            None,
        );
        let x = expand(&f, Chicago, at("2026-11-10 00:00"), at("2026-11-11 00:00"));
        assert_eq!(x.bad_rules, 1);
        assert_eq!(x.events.len(), 1);
    }

    #[test]
    fn windows() {
        let (from, to) = window(at("2026-11-10 12:50"), Chicago, 1, 14);
        assert_eq!(from, at("2026-11-09 00:00"));
        assert_eq!(to, at("2026-11-25 00:00"));
    }
}
