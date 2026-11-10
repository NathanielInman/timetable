//! `~/.config/timetable/config.toml`. Every key is optional; an empty or
//! missing file gives a working clock, and one `[calendar]` key adds the
//! agenda.

use chrono::NaiveTime;
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub calendar: CalendarConfig,
    pub clock: ClockConfig,
    pub bar: BarConfig,
    pub join: JoinConfig,
    pub reminders: RemindersConfig,
    pub focus: FocusConfig,
    pub day: DayConfig,
    pub drawer: DrawerConfig,
    pub theme: ThemeConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct CalendarConfig {
    /// A command (run with `sh -c`) that prints the calendar's address,
    /// e.g. `broker get TIMETABLE_ICAL_URL`. Run when the daemon starts and
    /// again when the address stops working; never in between.
    pub url_command: String,
    /// The address itself. `url_command` keeps it out of this file.
    pub url: String,
    /// A local `.ics` file, read instead of fetching.
    pub file: String,
    /// Your address on the calendar, for skipping events you declined.
    /// Derived from a Google address (`/ical/<you>/private-...`) or the
    /// feed's `X-WR-CALNAME` when empty.
    pub owner: String,
    /// Minutes between fetches.
    pub refresh: u64,
    /// Seconds one fetch may take.
    pub timeout: u64,
    /// Days of the past kept in the agenda.
    pub past_days: u32,
    /// Days ahead expanded.
    pub future_days: u32,
}

impl Default for CalendarConfig {
    fn default() -> Self {
        CalendarConfig {
            url_command: String::new(),
            url: String::new(),
            file: String::new(),
            owner: String::new(),
            refresh: 5,
            timeout: 60,
            past_days: 1,
            future_days: 14,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Zone {
    /// An IANA name: `America/Chicago`, `EET`, `Europe/London`.
    pub zone: String,
    /// What the drawer and tooltip call it; the zone's name when empty.
    #[serde(default)]
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct ClockConfig {
    /// strftime format of the bar clock (chrono's: `%a %b %-d  %H:%M`).
    pub format: String,
    /// Your zone, an IANA name. Empty reads the system's (`TZ`, then
    /// `/etc/localtime`).
    pub zone: String,
    /// The zones in the drawer and the tooltip, in order.
    pub zones: Vec<Zone>,
}

impl Default for ClockConfig {
    fn default() -> Self {
        ClockConfig {
            format: "%a %b %-d  %H:%M".into(),
            zone: String::new(),
            zones: vec![
                Zone {
                    zone: "America/Chicago".into(),
                    label: "Chicago".into(),
                },
                Zone {
                    zone: "EET".into(),
                    label: "EET".into(),
                },
                Zone {
                    zone: "GMT".into(),
                    label: "GMT".into(),
                },
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct BarConfig {
    /// Longest event title in the bar, cut at a word boundary.
    pub max_chars: usize,
    /// Within this many minutes the bar names the next event (`next`).
    pub next_minutes: i64,
    /// Within this many minutes it turns amber (`soon`).
    pub soon_minutes: i64,
    /// From the start for this many minutes it is red and says Join (`now`).
    pub now_minutes: i64,
    /// While free, say until when if something is left today.
    pub free_until: bool,
    /// Between the event and the clock.
    pub separator: String,
    /// Pango color for the clock while an event shows (`#B48EAD`); empty
    /// leaves it the module's color.
    pub clock_color: String,
    /// Glyphs before the event text, by state; empty for none.
    pub icon_event: String,
    pub icon_now: String,
    pub icon_in: String,
    pub icon_focus: String,
    /// Shown before the clock when the calendar cannot be reached.
    pub icon_offline: String,
    /// The bar's layer-shell namespace, for anchoring the drawer.
    pub namespace: String,
}

impl Default for BarConfig {
    fn default() -> Self {
        BarConfig {
            max_chars: 24,
            next_minutes: 60,
            soon_minutes: 10,
            now_minutes: 5,
            free_until: true,
            separator: "  ".into(),
            clock_color: String::new(),
            icon_event: "\u{f00ed}".into(),
            icon_now: "\u{f0567}".into(),
            icon_in: "\u{f0849}".into(),
            icon_focus: "\u{f051b}".into(),
            icon_offline: "\u{f0164}".into(),
            namespace: "waybar".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct JoinConfig {
    /// Hyprland workspace to switch to before opening the link; empty
    /// stays put.
    pub workspace: String,
    /// The Hyprland dispatch, with `{workspace}` replaced. The default is
    /// the Lua config's `hl.dsp.focus`.
    pub dispatch: String,
    /// Opens the link, which is passed as one more argument (`sh -c
    /// '<opener> "$1"'`).
    pub opener: String,
}

impl Default for JoinConfig {
    fn default() -> Self {
        JoinConfig {
            workspace: "2".into(),
            dispatch: "hl.dsp.focus({ workspace = {workspace} })".into(),
            opener: "xdg-open".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct RemindersConfig {
    /// Minutes before each timed event to notify; empty sends none.
    pub minutes: Vec<i64>,
}

impl Default for RemindersConfig {
    fn default() -> Self {
        RemindersConfig {
            minutes: vec![5, 1],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct FocusConfig {
    /// Length of a block when nothing else is on today.
    pub minutes: i64,
    /// A block ends this many minutes before the next event.
    pub lead: i64,
    /// Longest block in minutes; 0 is no limit.
    pub max: i64,
    /// Shortest block worth starting.
    pub min: i64,
}

impl Default for FocusConfig {
    fn default() -> Self {
        FocusConfig {
            minutes: 50,
            lead: 2,
            max: 0,
            min: 5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct DayConfig {
    /// The day strip and free gaps run from `start` to `end` (`HH:MM`).
    pub start: String,
    pub end: String,
    /// Shortest gap drawn as free, in minutes.
    pub min_gap: i64,
}

impl Default for DayConfig {
    fn default() -> Self {
        DayConfig {
            start: "08:00".into(),
            end: "18:00".into(),
            min_gap: 15,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct DrawerConfig {
    /// Panel width in logical pixels.
    pub width: f64,
    /// Largest height as a fraction of the monitor.
    pub max_height: f64,
    /// Gap between the bar and the panel.
    pub gap: f64,
    /// Shown in the drawer's footer: what right-clicking the clock opens.
    pub calendar_hint: String,
}

impl Default for DrawerConfig {
    fn default() -> Self {
        DrawerConfig {
            width: 420.0,
            max_height: 0.85,
            gap: 8.0,
            calendar_hint: "right-click the clock for the calendar".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct ThemeConfig {
    /// `waybar` reads the bar's style.css for font and colors; `none` uses
    /// the built-in theme.
    pub from: String,
    pub accent: Option<String>,
    pub font: Option<String>,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        ThemeConfig {
            from: "waybar".into(),
            accent: None,
            font: None,
        }
    }
}

/// The system zone: `TZ`, then the `/etc/localtime` link.
pub fn system_zone() -> Option<Tz> {
    if let Ok(tz) = std::env::var("TZ") {
        if let Ok(z) = tz.trim_start_matches(':').parse() {
            return Some(z);
        }
    }
    let link = std::fs::read_link("/etc/localtime").ok()?;
    let s = link.to_string_lossy();
    let name = s.split("zoneinfo/").nth(1)?;
    name.parse().ok()
}

fn hhmm(s: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(s.trim(), "%H:%M").ok()
}

impl Config {
    pub fn parse(text: &str) -> Result<Config, String> {
        let c: Config =
            toml::from_str(text).map_err(|e| e.to_string().trim().replace('\n', " "))?;
        c.check()?;
        Ok(c)
    }

    /// Values serde cannot check.
    fn check(&self) -> Result<(), String> {
        if !self.clock.zone.is_empty() && self.clock.zone.parse::<Tz>().is_err() {
            return Err(format!("clock.zone: unknown zone `{}`", self.clock.zone));
        }
        for z in &self.clock.zones {
            if z.zone.parse::<Tz>().is_err() {
                return Err(format!("clock.zones: unknown zone `{}`", z.zone));
            }
        }
        for (k, v) in [("day.start", &self.day.start), ("day.end", &self.day.end)] {
            if hhmm(v).is_none() {
                return Err(format!("{k}: `{v}` is not HH:MM"));
            }
        }
        if self.day_span().0 >= self.day_span().1 {
            return Err("day.start must be before day.end".into());
        }
        Ok(())
    }

    /// The config and, when the file is broken, why (the defaults are used).
    pub fn load() -> (Config, Option<String>) {
        let path = crate::paths::config_file();
        match std::fs::read_to_string(&path) {
            Ok(text) => match Config::parse(&text) {
                Ok(c) => (c, None),
                Err(e) => (Config::default(), Some(format!("{}: {e}", path.display()))),
            },
            Err(_) => (Config::default(), None),
        }
    }

    /// The zone times are shown in.
    pub fn local_zone(&self) -> Tz {
        self.clock
            .zone
            .parse()
            .ok()
            .or_else(system_zone)
            .unwrap_or(chrono_tz::UTC)
    }

    /// The day strip's start and end.
    pub fn day_span(&self) -> (NaiveTime, NaiveTime) {
        (
            hhmm(&self.day.start).unwrap_or(NaiveTime::from_hms_opt(8, 0, 0).unwrap_or_default()),
            hhmm(&self.day.end).unwrap_or(NaiveTime::from_hms_opt(18, 0, 0).unwrap_or_default()),
        )
    }

    /// The zones with labels filled in.
    pub fn zones(&self) -> Vec<(Tz, String)> {
        self.clock
            .zones
            .iter()
            .filter_map(|z| {
                let tz: Tz = z.zone.parse().ok()?;
                let label = if z.label.trim().is_empty() {
                    z.zone
                        .rsplit('/')
                        .next()
                        .unwrap_or(&z.zone)
                        .replace('_', " ")
                } else {
                    z.label.clone()
                };
                Some((tz, label))
            })
            .collect()
    }

    /// Whether any calendar source is set.
    pub fn has_calendar(&self) -> bool {
        let c = &self.calendar;
        !(c.url_command.trim().is_empty() && c.url.trim().is_empty() && c.file.trim().is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_is_default() {
        let c = Config::parse("").unwrap();
        assert_eq!(c, Config::default());
        assert!(!c.has_calendar());
        assert_eq!(c.zones().len(), 3);
        assert_eq!(c.zones()[0].1, "Chicago");
    }

    #[test]
    fn partial_tables() {
        let c = Config::parse(
            "[calendar]\nurl_command = \"broker get TIMETABLE_ICAL_URL\"\n[clock]\nzones = [{ zone = \"Asia/Tokyo\" }]\n[reminders]\nminutes = [10]\n",
        )
        .unwrap();
        assert!(c.has_calendar());
        assert_eq!(c.calendar.refresh, 5);
        assert_eq!(
            c.zones(),
            vec![(chrono_tz::Asia::Tokyo, "Tokyo".to_string())]
        );
        assert_eq!(c.reminders.minutes, vec![10]);
        assert_eq!(c.focus.lead, 2);
    }

    #[test]
    fn mistakes_are_errors() {
        assert!(Config::parse("[bar]\nmax_char = 3\n").is_err());
        assert!(Config::parse("[clock]\nzone = \"Mars/Olympus\"\n").is_err());
        assert!(Config::parse("[clock]\nzones = [{ zone = \"Nowhere\" }]\n").is_err());
        assert!(Config::parse("[day]\nstart = \"9am\"\n").is_err());
        assert!(Config::parse("[day]\nstart = \"18:00\"\nend = \"08:00\"\n").is_err());
    }

    #[test]
    fn zones_resolve() {
        let c = Config::parse("[clock]\nzone = \"America/Chicago\"\n").unwrap();
        assert_eq!(c.local_zone(), chrono_tz::America::Chicago);
    }
}
