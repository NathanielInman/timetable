//! A small iCalendar (RFC 5545) reader, enough for a calendar feed: it
//! unfolds lines, splits properties into name, parameters and value, and
//! collects each top-level `VEVENT` into a [`RawEvent`]. `VTIMEZONE` blocks
//! are skipped (zones come from their IANA names), and so are components
//! nested in an event (a `VALARM` has its own `DESCRIPTION`).
//!
//! Everything here is a pure function over text, tested against samples in
//! `fixtures/`. A Google feed is several megabytes with years of history,
//! so the reader keeps only what the agenda needs: the description is
//! searched for a meeting link and then dropped.

use crate::links;
use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use chrono_tz::Tz;

/// A date or time as the feed wrote it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum When {
    /// `VALUE=DATE`: an all-day date.
    Date(NaiveDate),
    /// A time ending in `Z`.
    Utc(NaiveDateTime),
    /// A time with a `TZID` the zone database knows.
    Zoned(NaiveDateTime, Tz),
    /// A time with neither: the viewer's local time.
    Floating(NaiveDateTime),
}

impl When {
    pub fn is_date(&self) -> bool {
        matches!(self, When::Date(_))
    }

    /// The wall-clock time, ignoring the zone.
    pub fn naive(&self) -> NaiveDateTime {
        match *self {
            When::Date(d) => d.and_time(NaiveTime::MIN),
            When::Utc(t) | When::Zoned(t, _) | When::Floating(t) => t,
        }
    }
}

/// One `VEVENT`, with values decoded but not yet placed in time.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RawEvent {
    pub uid: String,
    pub summary: String,
    pub location: String,
    /// `CONFIRMED`, `TENTATIVE` or `CANCELLED`, upper case.
    pub status: String,
    pub dtstart: Option<When>,
    pub dtend: Option<When>,
    /// `DURATION` in seconds, when there is no `DTEND`.
    pub duration: Option<i64>,
    /// The `RRULE` value, without the `RRULE:` name.
    pub rrule: Option<String>,
    pub rdates: Vec<When>,
    pub exdates: Vec<When>,
    pub recurrence_id: Option<When>,
    /// The owner's attendee entry says `PARTSTAT=DECLINED`.
    pub declined: bool,
    /// The meeting link and its kind (`zoom`, `meet`, ...).
    pub link: Option<String>,
    pub kind: Option<String>,
}

/// What the reader found besides the events.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Feed {
    pub events: Vec<RawEvent>,
    /// `X-WR-CALNAME`: for a Google primary calendar, the owner's address.
    pub calname: Option<String>,
    /// `X-WR-TIMEZONE`: the calendar's own zone.
    pub timezone: Option<String>,
    /// TZIDs that are not IANA names and were read as local time.
    pub unknown_zones: Vec<String>,
}

/// Joins folded lines (a line starting with a space or a tab continues the
/// previous one) and normalizes line ends to `\n`.
pub fn unfold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut lines = text.split('\n').peekable();
    while let Some(line) = lines.next() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        let cont = line.starts_with(' ') || line.starts_with('\t');
        if cont && !out.is_empty() {
            // Continuation: drop the one leading whitespace character and
            // the line break before it.
            if out.ends_with('\n') {
                out.pop();
            }
            out.push_str(&line[1..]);
        } else {
            out.push_str(line);
        }
        if lines.peek().is_some() {
            out.push('\n');
        }
    }
    out
}

/// One content line: `NAME;PARAM=VALUE;PARAM="Q:V":value`.
#[derive(Debug, Clone, PartialEq)]
pub struct Prop<'a> {
    pub name: &'a str,
    pub params: Vec<(&'a str, &'a str)>,
    pub value: &'a str,
}

impl<'a> Prop<'a> {
    pub fn param(&self, name: &str) -> Option<&'a str> {
        self.params
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| *v)
    }
}

/// Splits a content line. Parameter values may be quoted and contain `:`
/// and `;`, so the split respects quotes.
pub fn prop(line: &str) -> Option<Prop<'_>> {
    let bytes = line.as_bytes();
    let mut in_q = false;
    let mut colon = None;
    let mut semis = vec![];
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'"' => in_q = !in_q,
            b':' if !in_q => {
                colon = Some(i);
                break;
            }
            b';' if !in_q => semis.push(i),
            _ => {}
        }
    }
    let colon = colon?;
    let name_end = semis.first().copied().unwrap_or(colon);
    let name = &line[..name_end];
    if name.is_empty() {
        return None;
    }
    let mut params = vec![];
    let mut bounds = semis.clone();
    bounds.push(colon);
    for w in bounds.windows(2) {
        let p = &line[w[0] + 1..w[1]];
        if let Some((k, v)) = p.split_once('=') {
            params.push((k, v.trim_matches('"')));
        }
    }
    Some(Prop {
        name,
        params,
        value: &line[colon + 1..],
    })
}

/// Decodes a TEXT value: `\n`, `\,`, `\;` and `\\`.
pub fn unescape(s: &str) -> String {
    if !s.contains('\\') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('n' | 'N') => out.push('\n'),
                Some(o) => out.push(o),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// A few Windows zone names Outlook invites carry, mapped to IANA.
fn windows_zone(name: &str) -> Option<Tz> {
    let iana = match name {
        "Central Standard Time" => "America/Chicago",
        "Eastern Standard Time" => "America/New_York",
        "Mountain Standard Time" => "America/Denver",
        "US Mountain Standard Time" => "America/Phoenix",
        "Pacific Standard Time" => "America/Los_Angeles",
        "GMT Standard Time" => "Europe/London",
        "Greenwich Standard Time" => "Atlantic/Reykjavik",
        "W. Europe Standard Time" => "Europe/Berlin",
        "Central European Standard Time" => "Europe/Warsaw",
        "Central Europe Standard Time" => "Europe/Budapest",
        "Romance Standard Time" => "Europe/Paris",
        "FLE Standard Time" => "Europe/Kyiv",
        "E. Europe Standard Time" => "Europe/Chisinau",
        "GTB Standard Time" => "Europe/Bucharest",
        "Israel Standard Time" => "Asia/Jerusalem",
        "India Standard Time" => "Asia/Kolkata",
        "China Standard Time" => "Asia/Shanghai",
        "Tokyo Standard Time" => "Asia/Tokyo",
        "AUS Eastern Standard Time" => "Australia/Sydney",
        "UTC" | "Coordinated Universal Time" => "UTC",
        _ => return None,
    };
    iana.parse().ok()
}

/// An IANA zone from a TZID, tolerating a leading `/` (`/mozilla.org/...`
/// style prefixes keep the IANA name at the end) and Windows names.
pub fn zone(tzid: &str) -> Option<Tz> {
    let t = tzid.trim().trim_matches('"');
    if let Ok(z) = t.parse::<Tz>() {
        return Some(z);
    }
    if let Some(z) = windows_zone(t) {
        return Some(z);
    }
    // "/citadel.org/20250101_1/America/Chicago": try the last two parts.
    let parts: Vec<&str> = t.split('/').filter(|p| !p.is_empty()).collect();
    for n in [3, 2] {
        if parts.len() >= n {
            if let Ok(z) = parts[parts.len() - n..].join("/").parse::<Tz>() {
                return Some(z);
            }
        }
    }
    None
}

fn date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.get(..8)?, "%Y%m%d").ok()
}

fn datetime(s: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(s.get(..15)?, "%Y%m%dT%H%M%S").ok()
}

/// One date or date-time value with its `TZID` and `VALUE` parameters.
pub fn when(
    value: &str,
    tzid: Option<&str>,
    is_date: bool,
    unknown: &mut Vec<String>,
) -> Option<When> {
    let v = value.trim();
    if is_date || (v.len() == 8 && !v.contains('T')) {
        return date(v).map(When::Date);
    }
    let t = datetime(v)?;
    if v.ends_with('Z') || v.ends_with('z') {
        return Some(When::Utc(t));
    }
    match tzid {
        Some(id) => match zone(id) {
            Some(z) => Some(When::Zoned(t, z)),
            None => {
                if !unknown.iter().any(|u| u == id) {
                    unknown.push(id.to_string());
                }
                Some(When::Floating(t))
            }
        },
        None => Some(When::Floating(t)),
    }
}

/// A list property (`EXDATE`, `RDATE`): comma-separated values sharing the
/// parameters.
fn whens(p: &Prop, unknown: &mut Vec<String>) -> Vec<When> {
    let is_date = p
        .param("VALUE")
        .is_some_and(|v| v.eq_ignore_ascii_case("DATE"));
    if p.param("VALUE")
        .is_some_and(|v| v.eq_ignore_ascii_case("PERIOD"))
    {
        // RDATE periods: keep the start.
        return p
            .value
            .split(',')
            .filter_map(|v| when(v.split('/').next()?, p.param("TZID"), false, unknown))
            .collect();
    }
    p.value
        .split(',')
        .filter_map(|v| when(v, p.param("TZID"), is_date, unknown))
        .collect()
}

/// `PT1H30M`, `P1D`, `-PT15M`: seconds.
pub fn duration(s: &str) -> Option<i64> {
    let s = s.trim();
    let (neg, s) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let s = s.strip_prefix('P')?;
    let mut total = 0i64;
    let mut num = String::new();
    let mut in_time = false;
    for c in s.chars() {
        match c {
            'T' => in_time = true,
            '0'..='9' => num.push(c),
            _ => {
                let n: i64 = num.parse().ok()?;
                num.clear();
                total += n * match (c, in_time) {
                    ('W', _) => 7 * 86400,
                    ('D', _) => 86400,
                    ('H', true) => 3600,
                    ('M', true) => 60,
                    ('S', true) => 1,
                    _ => return None,
                };
            }
        }
    }
    if !num.is_empty() {
        return None;
    }
    Some(if neg { -total } else { total })
}

/// The address in an attendee or organizer value, lower case.
pub fn address(value: &str) -> String {
    let v = value.trim();
    let v = v
        .get(..7)
        .filter(|p| p.eq_ignore_ascii_case("mailto:"))
        .map(|_| &v[7..])
        .unwrap_or(v);
    v.to_ascii_lowercase()
}

/// Reads a feed. `owner` is the calendar owner's address: their
/// `ATTENDEE` entry with `PARTSTAT=DECLINED` marks the event declined.
/// Without one, `X-WR-CALNAME` is used when it looks like an address.
pub fn parse(text: &str, owner: Option<&str>) -> Feed {
    let text = unfold(text);
    let mut feed = Feed::default();
    let mut owner = owner
        .map(|o| o.trim().to_ascii_lowercase())
        .filter(|o| !o.is_empty());
    // Components open around the current line, innermost last.
    let mut stack: Vec<&str> = vec![];
    let mut ev: Option<RawEvent> = None;
    let mut description = String::new();
    let mut conference: Option<String> = None;
    let mut unknown = vec![];

    for line in text.split('\n') {
        if line.is_empty() {
            continue;
        }
        let Some(p) = prop(line) else { continue };
        let name = p.name;
        if name.eq_ignore_ascii_case("BEGIN") {
            let comp = p.value.trim();
            if comp.eq_ignore_ascii_case("VEVENT") && stack.len() == 1 {
                ev = Some(RawEvent::default());
                description.clear();
                conference = None;
            }
            stack.push(comp);
            continue;
        }
        if name.eq_ignore_ascii_case("END") {
            let comp = stack.pop().unwrap_or_default();
            if comp.eq_ignore_ascii_case("VEVENT") && stack.len() == 1 {
                if let Some(mut e) = ev.take() {
                    if let Some((link, kind)) =
                        links::find(conference.as_deref(), &e.location, &description)
                    {
                        e.link = Some(link);
                        e.kind = Some(kind.to_string());
                    }
                    feed.events.push(e);
                }
            }
            continue;
        }
        // Calendar-level properties.
        if stack.len() == 1 {
            if name.eq_ignore_ascii_case("X-WR-CALNAME") {
                feed.calname = Some(unescape(p.value));
            } else if name.eq_ignore_ascii_case("X-WR-TIMEZONE") {
                feed.timezone = Some(p.value.trim().to_string());
            }
            continue;
        }
        // Only the event's own properties, never a VALARM's.
        if stack.len() != 2 {
            continue;
        }
        let Some(e) = ev.as_mut() else { continue };
        let upper = name.to_ascii_uppercase();
        let is_date = p
            .param("VALUE")
            .is_some_and(|v| v.eq_ignore_ascii_case("DATE"));
        match upper.as_str() {
            "UID" => e.uid = p.value.trim().to_string(),
            "SUMMARY" => e.summary = unescape(p.value).trim().to_string(),
            "LOCATION" => e.location = unescape(p.value).trim().to_string(),
            "DESCRIPTION" => description = unescape(p.value),
            "STATUS" => e.status = p.value.trim().to_ascii_uppercase(),
            "DTSTART" => e.dtstart = when(p.value, p.param("TZID"), is_date, &mut unknown),
            "DTEND" => e.dtend = when(p.value, p.param("TZID"), is_date, &mut unknown),
            "DURATION" => e.duration = duration(p.value),
            "RRULE" => e.rrule = Some(p.value.trim().to_string()),
            "RDATE" => e.rdates.extend(whens(&p, &mut unknown)),
            "EXDATE" => e.exdates.extend(whens(&p, &mut unknown)),
            "RECURRENCE-ID" => {
                e.recurrence_id = when(p.value, p.param("TZID"), is_date, &mut unknown)
            }
            "X-GOOGLE-CONFERENCE" => conference = Some(p.value.trim().to_string()),
            "ATTENDEE" => {
                if owner.is_none() {
                    // A Google primary calendar is named after its owner.
                    owner = feed
                        .calname
                        .as_deref()
                        .filter(|c| c.contains('@'))
                        .map(|c| c.trim().to_ascii_lowercase());
                }
                if let Some(o) = owner.as_deref() {
                    if address(p.value) == o
                        && p.param("PARTSTAT")
                            .is_some_and(|s| s.eq_ignore_ascii_case("DECLINED"))
                    {
                        e.declined = true;
                    }
                }
            }
            _ => {}
        }
    }
    feed.unknown_zones = unknown;
    feed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unfolds_crlf_and_tabs() {
        let t = "SUMMARY:Long\r\n  title\r\n\t continued\r\nUID:1\r\n";
        assert_eq!(unfold(t), "SUMMARY:Long title continued\nUID:1\n");
    }

    #[test]
    fn splits_quoted_params() {
        let p = prop(r#"ATTENDEE;CN="Doe, Jane: PM";PARTSTAT=DECLINED:mailto:jane@example.com"#)
            .unwrap();
        assert_eq!(p.name, "ATTENDEE");
        assert_eq!(p.param("cn"), Some("Doe, Jane: PM"));
        assert_eq!(p.param("PARTSTAT"), Some("DECLINED"));
        assert_eq!(p.value, "mailto:jane@example.com");
        let p = prop("DTSTART;TZID=America/Chicago:20261110T090000").unwrap();
        assert_eq!(p.param("TZID"), Some("America/Chicago"));
        assert!(prop("garbage").is_none());
    }

    #[test]
    fn text_escapes() {
        assert_eq!(unescape(r"a\, b\; c\\d\ne"), "a, b; c\\d\ne");
    }

    #[test]
    fn values() {
        let mut u = vec![];
        assert_eq!(
            when("20261110", None, true, &mut u),
            Some(When::Date(NaiveDate::from_ymd_opt(2026, 11, 10).unwrap()))
        );
        assert!(matches!(
            when("20261110T180000Z", None, false, &mut u),
            Some(When::Utc(_))
        ));
        assert!(matches!(
            when("20261110T090000", Some("Europe/Kiev"), false, &mut u),
            Some(When::Zoned(_, _))
        ));
        assert!(matches!(
            when(
                "20261110T090000",
                Some("Central Standard Time"),
                false,
                &mut u
            ),
            Some(When::Zoned(_, chrono_tz::America::Chicago))
        ));
        assert!(matches!(
            when("20261110T090000", Some("Mars/Olympus"), false, &mut u),
            Some(When::Floating(_))
        ));
        assert_eq!(u, vec!["Mars/Olympus".to_string()]);
        assert_eq!(
            zone("/citadel.org/20250101_1/America/Chicago"),
            Some(chrono_tz::America::Chicago)
        );
    }

    #[test]
    fn durations() {
        assert_eq!(duration("PT1H30M"), Some(5400));
        assert_eq!(duration("P1D"), Some(86400));
        assert_eq!(duration("P1W"), Some(604800));
        assert_eq!(duration("-PT15M"), Some(-900));
        assert_eq!(duration("PT"), Some(0));
        assert_eq!(duration("1H"), None);
    }

    #[test]
    fn alarms_do_not_leak_into_the_event() {
        let t = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:a\nSUMMARY:Real\nDTSTART:20261110T150000Z\nBEGIN:VALARM\nDESCRIPTION:https://zoom.us/j/999\nSUMMARY:Alarm\nEND:VALARM\nEND:VEVENT\nEND:VCALENDAR\n";
        let f = parse(t, None);
        assert_eq!(f.events.len(), 1);
        assert_eq!(f.events[0].summary, "Real");
        assert_eq!(f.events[0].link, None);
    }

    #[test]
    fn timezones_are_skipped() {
        let t = "BEGIN:VCALENDAR\nX-WR-TIMEZONE:America/Chicago\nBEGIN:VTIMEZONE\nTZID:America/Chicago\nBEGIN:DAYLIGHT\nDTSTART:19700308T020000\nRRULE:FREQ=YEARLY;BYMONTH=3;BYDAY=2SU\nEND:DAYLIGHT\nEND:VTIMEZONE\nEND:VCALENDAR\n";
        let f = parse(t, None);
        assert!(f.events.is_empty());
        assert_eq!(f.timezone.as_deref(), Some("America/Chicago"));
    }

    #[test]
    fn owner_declines() {
        let t = "BEGIN:VCALENDAR\nX-WR-CALNAME:owner@example.com\nBEGIN:VEVENT\nUID:a\nATTENDEE;CN=Other;PARTSTAT=DECLINED:mailto:other@example.com\nATTENDEE;CN=Me;PARTSTAT=ACCEPTED:mailto:owner@example.com\nEND:VEVENT\nBEGIN:VEVENT\nUID:b\nATTENDEE;CN=Me;PARTSTAT=DECLINED:mailto:Owner@Example.com\nEND:VEVENT\nEND:VCALENDAR\n";
        let f = parse(t, None);
        assert!(!f.events[0].declined);
        assert!(f.events[1].declined);
        // An explicit owner wins over the calendar name.
        let f = parse(t, Some("other@example.com"));
        assert!(f.events[0].declined);
        assert!(!f.events[1].declined);
    }
}
