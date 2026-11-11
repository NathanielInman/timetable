//! What the daemon knows and sends to the bar, the drawer and the CLI.

use serde::{Deserialize, Serialize};

/// One occurrence of an event, placed in time. Times are Unix seconds; an
/// all-day event runs from local midnight to local midnight.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Event {
    pub uid: String,
    pub title: String,
    pub start: i64,
    pub end: i64,
    #[serde(default)]
    pub all_day: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub location: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    /// `zoom`, `meet`, `teams`, `slack`, `webex`, `whereby` or `link`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

impl Event {
    /// Identifies this occurrence: the same event on another day is another
    /// key.
    pub fn key(&self) -> String {
        format!("{}@{}", self.uid, self.start)
    }

    pub fn title_or_default(&self) -> &str {
        if self.title.trim().is_empty() {
            "Busy"
        } else {
            self.title.trim()
        }
    }

    pub fn timed(&self) -> bool {
        !self.all_day
    }

    pub fn running(&self, now: i64) -> bool {
        self.start <= now && now < self.end
    }
}

/// A focus block: started at `start`, ends (and notifies) at `end`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Focus {
    pub start: i64,
    pub end: i64,
}

impl Focus {
    pub fn running(&self, now: i64) -> bool {
        now < self.end
    }
}

/// Where the calendar comes from.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    #[default]
    None,
    Command,
    Url,
    File,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::None => "none",
            Source::Command => "url_command",
            Source::Url => "url",
            Source::File => "file",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Snapshot {
    pub version: String,
    /// Unix seconds when this snapshot was made.
    pub now: f64,
    pub source: Source,
    /// Occurrences in the window (yesterday to two weeks out), sorted by
    /// start. Declined and cancelled ones are already gone.
    pub events: Vec<Event>,
    /// When the calendar was last read successfully.
    pub fetched_at: Option<f64>,
    /// When the last attempt ended, successful or not.
    pub attempted_at: Option<f64>,
    /// Why the last attempt failed. Never contains the address.
    pub error: Option<String>,
    /// The agenda came from the cache and no fetch has worked since start.
    #[serde(default)]
    pub cached: bool,
    pub focus: Option<Focus>,
    /// Keys (`uid@start`) of events Join opened, so the bar stops calling
    /// for them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub joined: Vec<String>,
    pub config_error: Option<String>,
}

impl Snapshot {
    /// Whether there is any calendar to show.
    pub fn has_calendar(&self) -> bool {
        self.fetched_at.is_some()
    }
}
