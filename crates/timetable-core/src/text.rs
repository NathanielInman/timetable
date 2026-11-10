//! Text for the bar and the drawer.

/// Shortens `text` to at most `max` characters, ending at a word boundary
/// with a real ellipsis, never in the middle of a word. Only a first word
/// longer than `max` on its own is cut.
pub fn ellipsize(text: &str, max: usize) -> String {
    let text = text.trim();
    let n = text.chars().count();
    if n <= max {
        return text.to_string();
    }
    if max == 0 {
        return String::new();
    }
    // Room for the ellipsis.
    let keep: String = text.chars().take(max - 1).collect();
    let next = text.chars().nth(max - 1);
    let mut cut = if next.is_some_and(char::is_whitespace) {
        // The cut lands exactly between words.
        keep.as_str()
    } else {
        match keep.rfind(char::is_whitespace) {
            Some(i) => &keep[..i],
            None => keep.as_str(),
        }
    };
    // Do not leave a dangling separator: "Artist ·…" or "Title,…".
    loop {
        let t = cut.trim_end();
        let t = t.trim_end_matches([',', ';', ':', '-', '\u{b7}', '|', '/', '(', '[', '&']);
        let t = t.trim_end();
        if t.len() == cut.len() {
            break;
        }
        cut = t;
    }
    if cut.is_empty() {
        cut = keep.as_str();
    }
    format!("{cut}\u{2026}")
}

/// Escapes text for Pango markup (waybar renders module text and tooltips
/// as markup).
pub fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// A span of minutes: `7m`, `1h 05m`.
pub fn minutes(m: i64) -> String {
    let m = m.max(0);
    if m >= 60 {
        format!("{}h {:02}m", m / 60, m % 60)
    } else {
        format!("{m}m")
    }
}

/// A countdown in seconds: `23:14`, `1:02:03`.
pub fn countdown(secs: i64) -> String {
    let s = secs.max(0);
    let (h, m, s) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

/// How long ago, for "refreshed 2 min ago".
pub fn ago(secs: i64) -> String {
    match secs.max(0) {
        0..=59 => "just now".into(),
        s if s < 3600 => format!("{} min ago", s / 60),
        s if s < 86400 => format!("{} h ago", s / 3600),
        s => format!("{} days ago", s / 86400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_is_untouched() {
        assert_eq!(
            ellipsize("Daft Punk · Get Lucky", 40),
            "Daft Punk · Get Lucky"
        );
    }

    #[test]
    fn never_cuts_mid_word() {
        let t = "StarTalk · Why We Haven\u{2019}t Reached Superintelligence, with David Krakauer";
        let e = ellipsize(t, 40);
        assert_eq!(e, "StarTalk · Why We Haven\u{2019}t Reached\u{2026}");
        assert!(e.chars().count() <= 40);
        // The cut falls on a word boundary of the original.
        let stem = e.trim_end_matches('\u{2026}');
        assert!(t[stem.len()..].starts_with(' '));
    }

    #[test]
    fn drops_a_dangling_separator() {
        assert_eq!(
            ellipsize("StarTalk · Superintelligence", 12),
            "StarTalk\u{2026}"
        );
        assert_eq!(ellipsize("One, two three", 9), "One, two\u{2026}");
        assert_eq!(ellipsize("One, two three", 8), "One\u{2026}");
    }

    #[test]
    fn exact_boundary() {
        // "Daft Punk" is 9 chars; the 10th is a space.
        assert_eq!(ellipsize("Daft Punk Get Lucky", 10), "Daft Punk\u{2026}");
    }

    #[test]
    fn one_long_word_is_cut() {
        assert_eq!(ellipsize("Supercalifragilistic", 8), "Superca\u{2026}");
        assert_eq!(ellipsize("abc", 0), "");
    }

    #[test]
    fn counts_characters_not_bytes() {
        let e = ellipsize("Sigur Rós · Hoppípolla með", 20);
        assert!(e.chars().count() <= 20);
        assert_eq!(e, "Sigur Rós\u{2026}");
    }

    #[test]
    fn spans() {
        assert_eq!(minutes(7), "7m");
        assert_eq!(minutes(65), "1h 05m");
        assert_eq!(countdown(23 * 60 + 14), "23:14");
        assert_eq!(countdown(3723), "1:02:03");
        assert_eq!(ago(30), "just now");
        assert_eq!(ago(150), "2 min ago");
    }
}
