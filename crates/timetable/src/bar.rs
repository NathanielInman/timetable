//! The waybar line: what is next, then the clock. The first class is the
//! state (`free`, `next`, `soon`, `now`, `in`, `focus`, `offline`); `pulse`
//! joins `now` for an animation, and `stale` means the calendar has not
//! been read for a while and the agenda may be out of date.

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use timetable_core::agenda::{self, State};
use timetable_core::config::Config;
use timetable_core::links;
use timetable_core::model::{Event, Snapshot, Source};
use timetable_core::protocol::BarLine;
use timetable_core::text::{ago, countdown, ellipsize, escape, minutes};

/// The agenda is stale after this many missed refreshes (and at least 15
/// minutes).
const STALE_REFRESHES: i64 = 3;

pub fn local(t: i64, zone: Tz) -> DateTime<Tz> {
    DateTime::<Utc>::from_timestamp(t, 0)
        .unwrap_or_default()
        .with_timezone(&zone)
}

pub fn hhmm(t: i64, zone: Tz) -> String {
    local(t, zone).format("%H:%M").to_string()
}

/// Whether the agenda is older than a few refreshes.
pub fn stale(s: &Snapshot, cfg: &Config, now: i64) -> bool {
    let limit = (cfg.calendar.refresh as i64 * 60 * STALE_REFRESHES).max(15 * 60);
    s.source != Source::None && s.fetched_at.is_some_and(|f| now - f as i64 > limit)
}

fn with_icon(icon: &str, text: &str) -> String {
    if icon.is_empty() {
        text.to_string()
    } else {
        format!("{icon} {text}")
    }
}

fn title(e: &Event, cfg: &Config) -> String {
    escape(&ellipsize(e.title_or_default(), cfg.bar.max_chars))
}

/// The status the bar shows.
pub fn status(s: &Snapshot, cfg: &Config, now: i64) -> agenda::Status {
    agenda::status(
        &s.events,
        now,
        s.focus.as_ref(),
        s.has_calendar(),
        s.source != Source::None,
        cfg,
        cfg.local_zone(),
    )
}

pub fn line(s: &Snapshot, cfg: &Config, now: i64) -> BarLine {
    let zone = cfg.local_zone();
    let st = status(s, cfg, now);
    let b = &cfg.bar;
    let segment = match (st.state, &st.event) {
        (State::Next | State::Soon, Some(e)) => with_icon(
            &b.icon_event,
            &format!(
                "{} in {}",
                title(e, cfg),
                minutes(agenda::minutes_until(e.start, now))
            ),
        ),
        (State::Now, Some(e)) => with_icon(
            &b.icon_now,
            &format!(
                "{} \u{b7} {}",
                title(e, cfg),
                if e.link.is_some() { "Join" } else { "now" }
            ),
        ),
        (State::In, Some(e)) => with_icon(
            &b.icon_in,
            &format!(
                "{} \u{b7} {} left",
                title(e, cfg),
                minutes(agenda::minutes_until(e.end, now))
            ),
        ),
        (State::Focus, _) => with_icon(
            &b.icon_focus,
            &format!(
                "Focus {}",
                countdown(s.focus.map(|f| f.end - now).unwrap_or_default())
            ),
        ),
        (State::Free, _) if b.free_until => match st.free_until {
            Some(t) => format!("free until {}", hhmm(t, zone)),
            None => String::new(),
        },
        _ => String::new(),
    };
    let is_stale = stale(s, cfg, now);
    let mut clock = escape(&local(now, zone).format(&cfg.clock.format).to_string());
    if !b.clock_color.trim().is_empty() {
        clock = format!(
            "<span color='{}'>{clock}</span>",
            escape(b.clock_color.trim())
        );
    }
    if (st.state == State::Offline || is_stale) && !b.icon_offline.is_empty() {
        clock = format!("<span alpha='55%'>{}</span> {clock}", b.icon_offline);
    }
    let text = if segment.is_empty() {
        clock
    } else {
        format!("{segment}{}{clock}", escape(&b.separator))
    };
    let mut class = vec![st.state.as_str().to_string()];
    if st.state == State::Now {
        class.push("pulse".into());
    }
    if is_stale {
        class.push("stale".into());
    }
    BarLine {
        text,
        class,
        alt: st.state.as_str().into(),
        tooltip: escape(&tooltip(s, cfg, now)),
    }
}

/// `09:30  Standup · Zoom`.
pub fn row(e: &Event, zone: Tz) -> String {
    let when = if e.all_day {
        "all day".to_string()
    } else {
        hhmm(e.start, zone)
    };
    let kind = e
        .kind
        .as_deref()
        .map(|k| format!(" \u{b7} {}", links::label(k)))
        .unwrap_or_default();
    format!("{when:<9}{}{kind}", e.title_or_default())
}

/// The zones, as `Chicago 12:50 CST · EET 20:50 · GMT 18:50`.
pub fn zones_line(cfg: &Config, now: i64) -> String {
    cfg.zones()
        .iter()
        .map(|(tz, label)| {
            let t = DateTime::<Utc>::from_timestamp(now, 0)
                .unwrap_or_default()
                .with_timezone(tz);
            let abbr = t.format("%Z").to_string();
            if abbr == *label {
                format!("{label} {}", t.format("%H:%M"))
            } else {
                format!("{label} {} {abbr}", t.format("%H:%M"))
            }
        })
        .collect::<Vec<_>>()
        .join(" \u{b7} ")
}

/// Where the calendar stands, for the tooltip, the drawer and status.
pub fn freshness(s: &Snapshot, cfg: &Config, now: i64) -> String {
    match (s.source, s.fetched_at, &s.error) {
        (Source::None, _, _) => "no calendar configured".into(),
        (_, None, Some(e)) => format!("calendar unreachable: {e}"),
        (_, None, None) => "reading the calendar".into(),
        (_, Some(f), Some(e)) if stale(s, cfg, now) || s.cached => {
            format!("refreshed {} \u{b7} now failing: {e}", ago(now - f as i64))
        }
        (_, Some(f), _) => format!("refreshed {}", ago(now - f as i64)),
    }
}

pub fn tooltip(s: &Snapshot, cfg: &Config, now: i64) -> String {
    let zone = cfg.local_zone();
    let mut out = vec![];
    let today = agenda::remaining_today(&s.events, now, zone);
    let timed = today.iter().filter(|e| !e.all_day).count();
    out.push(format!(
        "{} \u{b7} {}",
        local(now, zone).format("%a %b %-d"),
        match timed {
            0 => "nothing left today".to_string(),
            1 => "1 left today".to_string(),
            n => format!("{n} left today"),
        }
    ));
    for e in today.iter().take(10) {
        out.push(format!("  {}", row(e, zone)));
    }
    if let Some(f) = s.focus.filter(|f| f.running(now)) {
        out.push(format!(
            "Focus until {} ({} left)",
            hhmm(f.end, zone),
            countdown(f.end - now)
        ));
    }
    out.push(String::new());
    out.push(zones_line(cfg, now));
    out.push(freshness(s, cfg, now));
    out.join("\n")
}

/// What the bar shows while the daemon cannot be reached: still the clock.
pub fn offline(cfg: &Config, now: i64) -> BarLine {
    let zone = cfg.local_zone();
    BarLine {
        text: escape(&local(now, zone).format(&cfg.clock.format).to_string()),
        class: vec!["offline".into()],
        alt: "offline".into(),
        tooltip: "Timetable daemon is not running".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDateTime;
    use timetable_core::expand::local_instant;
    use timetable_core::model::Focus;

    fn at(s: &str) -> i64 {
        local_instant(
            NaiveDateTime::parse_from_str(&format!("2026-11-10 {s}"), "%Y-%m-%d %H:%M").unwrap(),
            chrono_tz::America::Chicago,
        )
    }

    fn cfg() -> Config {
        let mut c = Config::parse("[clock]\nzone = \"America/Chicago\"\n").unwrap();
        c.bar.icon_event = String::new();
        c.bar.icon_now = String::new();
        c.bar.icon_in = String::new();
        c.bar.icon_focus = String::new();
        c
    }

    fn snap() -> Snapshot {
        Snapshot {
            source: Source::Command,
            fetched_at: Some(at("12:48") as f64),
            events: vec![
                Event {
                    uid: "s".into(),
                    title: "Standup & sync".into(),
                    start: at("13:00"),
                    end: at("13:15"),
                    link: Some("https://meet.google.com/abc-defg-hij".into()),
                    kind: Some("meet".into()),
                    ..Default::default()
                },
                Event {
                    uid: "r".into(),
                    title: "Sprint review".into(),
                    start: at("14:00"),
                    end: at("15:00"),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
    }

    /// A line from an agenda read a minute ago.
    fn fresh(s: &Snapshot, c: &Config, now: i64) -> BarLine {
        let mut s = s.clone();
        s.fetched_at = Some((now - 60) as f64);
        line(&s, c, now)
    }

    #[test]
    fn soon_now_in() {
        let c = cfg();
        let l = fresh(&snap(), &c, at("12:53"));
        assert_eq!(l.text, "Standup &amp; sync in 7m  Tue Nov 10  12:53");
        assert_eq!(l.class, vec!["soon"]);
        let l = fresh(&snap(), &c, at("13:01"));
        assert_eq!(l.text, "Standup &amp; sync · Join  Tue Nov 10  13:01");
        assert_eq!(l.class, vec!["now", "pulse"]);
        let l = fresh(&snap(), &c, at("13:06"));
        assert_eq!(l.text, "Standup &amp; sync · 9m left  Tue Nov 10  13:06");
        assert_eq!(l.class, vec!["in"]);
    }

    #[test]
    fn free_next_and_focus() {
        let c = cfg();
        let mut s = snap();
        s.fetched_at = Some(at("11:00") as f64);
        let l = fresh(&s, &c, at("11:10"));
        assert_eq!(l.text, "free until 13:00  Tue Nov 10  11:10");
        assert_eq!(l.class, vec!["free"]);
        let l = fresh(&s, &c, at("12:10"));
        assert_eq!(l.text, "Standup &amp; sync in 50m  Tue Nov 10  12:10");
        assert_eq!(l.class, vec!["next"]);
        s.focus = Some(Focus {
            start: at("15:00"),
            end: at("16:28"),
        });
        let l = fresh(&s, &c, at("15:04") + 46);
        assert_eq!(l.text, "Focus 1:23:14  Tue Nov 10  15:04");
        assert_eq!(l.class, vec!["focus"]);
        let l = fresh(&s, &c, at("17:00"));
        assert_eq!(l.text, "Tue Nov 10  17:00");
    }

    #[test]
    fn offline_and_stale() {
        let mut c = cfg();
        c.bar.icon_offline = "!".into();
        let mut s = snap();
        s.fetched_at = None;
        s.error = Some("no answer within 60 s".into());
        let l = line(&s, &c, at("12:53"));
        assert_eq!(l.class, vec!["offline"]);
        assert_eq!(l.text, "<span alpha='55%'>!</span> Tue Nov 10  12:53");
        assert!(l
            .tooltip
            .contains("calendar unreachable: no answer within 60 s"));
        let mut s = snap();
        s.fetched_at = Some(at("12:00") as f64);
        let l = line(&s, &c, at("12:53"));
        assert_eq!(l.class, vec!["soon", "stale"]);
        // No calendar at all is just a clock.
        let l = line(&Snapshot::default(), &c, at("12:53"));
        assert_eq!(l.text, "Tue Nov 10  12:53");
        assert_eq!(l.class, vec!["free"]);
    }

    #[test]
    fn clock_color_and_tooltip() {
        let mut c = cfg();
        c.bar.clock_color = "#B48EAD".into();
        let l = line(&snap(), &c, at("12:53"));
        assert!(l
            .text
            .ends_with("<span color='#B48EAD'>Tue Nov 10  12:53</span>"));
        let t = tooltip(&snap(), &c, at("12:53"));
        assert!(t.starts_with("Tue Nov 10 · 2 left today\n  13:00    Standup & sync · Google Meet\n  14:00    Sprint review"));
        assert!(t.contains("Chicago 12:53 CST · EET 20:53 · GMT 18:53"));
        assert!(t.ends_with("refreshed 5 min ago"));
    }
}
