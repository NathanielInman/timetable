//! Finding the meeting link in an event: Google's `X-GOOGLE-CONFERENCE`,
//! or a Zoom, Meet, Teams, Webex or Slack huddle address in the location
//! or the description.
//!
//! The location wins over Google's own conference: when an organizer pastes
//! a Zoom link into the location of an invite that also got an automatic
//! Meet, the Zoom is the meeting people actually join.

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
}
