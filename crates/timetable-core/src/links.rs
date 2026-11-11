//! Finding the meeting link in an event: Google's `X-GOOGLE-CONFERENCE`,
//! or a Zoom, Meet, Teams, Webex or Slack huddle address in the location
//! or the description. Also the event's own page in Google Calendar, where
//! it is edited.
//!
//! The location wins over Google's own conference: when an organizer pastes
//! a Zoom link into the location of an invite that also got an automatic
//! Meet, the Zoom is the meeting people actually join.

use crate::model::Event;
use chrono::TimeZone;
use chrono::Utc;
use chrono_tz::Tz;

/// Kinds of meeting link, by host.
fn kind_of(url: &str) -> Option<&'static str> {
    let lower = url.to_ascii_lowercase();
    let rest = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))?;
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let path = &rest[host.len()..];
    let on = |h: &str| host == h || host.ends_with(&format!(".{h}"));
    if on("zoom.us") || on("zoomgov.com") {
        // Meetings and personal rooms, not the marketing site.
        return (path.starts_with("/j/")
            || path.starts_with("/my/")
            || path.starts_with("/w/")
            || path.starts_with("/s/"))
        .then_some("zoom");
    }
    if host == "meet.google.com" {
        return (path.len() > 1).then_some("meet");
    }
    if on("teams.microsoft.com") || on("teams.live.com") {
        return (path.contains("meet") || path.starts_with("/l/")).then_some("teams");
    }
    if on("webex.com") {
        return Some("webex");
    }
    if on("slack.com") && path.contains("huddle") {
        return Some("slack");
    }
    if host == "whereby.com" {
        return Some("whereby");
    }
    None
}

/// Every `http(s)://` URL in a text, trimmed of the punctuation and markup
/// that usually follows one.
pub fn urls(text: &str) -> Vec<&str> {
    let mut out = vec![];
    let mut rest = text;
    while let Some(i) = rest.find("http") {
        let tail = &rest[i..];
        if !(tail.starts_with("https://") || tail.starts_with("http://")) {
            rest = &rest[i + 4..];
            continue;
        }
        let end = tail
            .find(|c: char| {
                c.is_whitespace() || matches!(c, '<' | '>' | '"' | '\'' | '`' | '\\' | '|' | ']')
            })
            .unwrap_or(tail.len());
        let url = tail[..end].trim_end_matches(['.', ',', ';', ':', ')', '!', '?', '*']);
        if url.len() > 8 {
            out.push(url);
        }
        rest = &tail[end..];
    }
    out
}

fn first_meeting(text: &str) -> Option<(String, &'static str)> {
    urls(text)
        .into_iter()
        .find_map(|u| kind_of(u).map(|k| (u.to_string(), k)))
}

/// The link to join and its kind. A location that is just a URL but not a
/// known meeting host still counts (`link`): it was put there to be opened.
pub fn find(
    conference: Option<&str>,
    location: &str,
    description: &str,
) -> Option<(String, &'static str)> {
    if let Some(found) = first_meeting(location) {
        return Some(found);
    }
    if let Some(c) = conference.map(str::trim).filter(|c| c.starts_with("http")) {
        return Some((c.to_string(), kind_of(c).unwrap_or("link")));
    }
    if let Some(found) = first_meeting(description) {
        return Some(found);
    }
    let loc = location.trim();
    if (loc.starts_with("https://") || loc.starts_with("http://")) && !loc.contains(' ') {
        return Some((loc.to_string(), "link"));
    }
    None
}

/// What the drawer calls a kind.
pub fn label(kind: &str) -> &str {
    match kind {
        "meet" => "Google Meet",
        "zoom" => "Zoom",
        "teams" => "Teams",
        "slack" => "Slack huddle",
        "webex" => "Webex",
        "whereby" => "Whereby",
        _ => "link",
    }
}

/// Google gives the events it creates the UID `<id>@google.com`.
pub fn google_id(uid: &str) -> Option<&str> {
    uid.strip_suffix("@google.com").filter(|id| !id.is_empty())
}

/// The event's page in Google Calendar, where it is edited. Google's link
/// is `event?eid=` with base64 of `<event id> <calendar id>`; an occurrence
/// of a recurring event adds its original start to the id, as UTC time, or
/// as a date when it is all day. An event Google did not create (an
/// invitation from another calendar) has no id Timetable knows, and
/// neither does an event read before the calendar id: those open on their
/// day.
pub fn google_page(e: &Event, calendar: Option<&str>, zone: Tz) -> String {
    if let (Some(id), Some(cal)) = (google_id(&e.uid), calendar) {
        let id = match e.recur.and_then(|r| Utc.timestamp_opt(r, 0).single()) {
            Some(r) if e.all_day => format!("{id}_{}", r.with_timezone(&zone).format("%Y%m%d")),
            Some(r) => format!("{id}_{}", r.format("%Y%m%dT%H%M%SZ")),
            None => id.to_string(),
        };
        return format!(
            "https://calendar.google.com/calendar/event?eid={}",
            base64url(format!("{id} {cal}").as_bytes())
        );
    }
    google_day(e.start, zone)
}

/// A day in Google Calendar.
pub fn google_day(t: i64, zone: Tz) -> String {
    let d = Utc
        .timestamp_opt(t, 0)
        .single()
        .unwrap_or_default()
        .with_timezone(&zone);
    format!(
        "https://calendar.google.com/calendar/r/day/{}",
        d.format("%Y/%-m/%-d")
    )
}

/// URL-safe base64 without padding.
fn base64url(b: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(b.len().div_ceil(3) * 4);
    for c in b.chunks(3) {
        let n = c
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, x)| n | (*x as u32) << (16 - 8 * i));
        for i in 0..=c.len() {
            out.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn google_conference() {
        let f = find(Some("https://meet.google.com/abc-defg-hij"), "", "");
        assert_eq!(
            f,
            Some(("https://meet.google.com/abc-defg-hij".into(), "meet"))
        );
    }

    #[test]
    fn zoom_in_location_beats_meet() {
        let f = find(
            Some("https://meet.google.com/abc-defg-hij"),
            "https://us02web.zoom.us/j/81234567890?pwd=AbC123",
            "",
        );
        assert_eq!(
            f,
            Some((
                "https://us02web.zoom.us/j/81234567890?pwd=AbC123".into(),
                "zoom"
            ))
        );
    }

    #[test]
    fn zoom_in_a_google_description() {
        let d = "Hi all\n\n───\nJoin Zoom Meeting\nhttps://example.zoom.us/j/81234567890?pwd=xyz.1\n\nMeeting ID: 812 3456 7890\n<a href=\"https://zoom.us/support\">help</a>";
        let f = find(None, "", d);
        assert_eq!(
            f,
            Some((
                "https://example.zoom.us/j/81234567890?pwd=xyz.1".into(),
                "zoom"
            ))
        );
    }

    #[test]
    fn markup_and_punctuation() {
        let d = "<p>Teams: <a href=\"https://teams.microsoft.com/l/meetup-join/19%3ameeting_x%40thread.v2/0\">Join</a>.</p>";
        assert_eq!(find(None, "", d).unwrap().1, "teams");
        let d = "huddle (https://app.slack.com/huddle/T0123/C0456).";
        assert_eq!(
            find(None, "", d),
            Some(("https://app.slack.com/huddle/T0123/C0456".into(), "slack"))
        );
    }

    #[test]
    fn not_meetings() {
        assert_eq!(
            find(
                None,
                "Room 4",
                "See https://zoom.us/support and https://example.com"
            ),
            None
        );
        assert_eq!(find(None, "Conference room B", ""), None);
        assert_eq!(
            find(None, "https://example.com/room", ""),
            Some(("https://example.com/room".into(), "link"))
        );
    }

    #[test]
    fn base64_is_url_safe_and_unpadded() {
        assert_eq!(base64url(b""), "");
        assert_eq!(base64url(b"ab"), "YWI");
        assert_eq!(base64url(b"abc"), "YWJj");
        assert_eq!(base64url(b"hello"), "aGVsbG8");
        assert_eq!(base64url(&[0xfb, 0xff]), "-_8");
    }

    #[test]
    fn google_pages() {
        let chi: Tz = "America/Chicago".parse().unwrap();
        let cal = Some("owner@example.com");
        let page = "https://calendar.google.com/calendar/event?eid=";
        let mut e = Event {
            uid: "abc123@google.com".into(),
            start: 1794411000,
            end: 1794412800,
            ..Event::default()
        };
        assert_eq!(
            google_page(&e, cal, chi),
            format!("{page}YWJjMTIzIG93bmVyQGV4YW1wbGUuY29t")
        );
        // An occurrence: its original start in UTC.
        e.recur = Some(1794411000);
        assert_eq!(
            google_page(&e, cal, chi),
            format!("{page}YWJjMTIzXzIwMjYxMTExVDE1MzAwMFogb3duZXJAZXhhbXBsZS5jb20")
        );
        // All day: the local date.
        e.all_day = true;
        e.recur = Some(1794290400);
        assert_eq!(
            google_page(&e, cal, chi),
            format!("{page}YWJjMTIzXzIwMjYxMTEwIG93bmVyQGV4YW1wbGUuY29t")
        );
        // Without the calendar id, or not Google's own event: its day.
        assert_eq!(
            google_page(&e, None, chi),
            "https://calendar.google.com/calendar/r/day/2026/11/11"
        );
        e.uid = "040000008200E00074C5B7101A82E008@outlook.com".into();
        assert_eq!(
            google_page(&e, cal, chi),
            "https://calendar.google.com/calendar/r/day/2026/11/11"
        );
    }
}
