//! Everything Timetable knows that is not the daemon or drawing: where
//! files live, the config, iCal parsing and recurrence, meeting links, the
//! agenda logic (states, gaps, focus blocks, reminders), fetching, the
//! socket protocol and the waybar reader.

pub mod config;
pub mod model;
pub mod paths;
pub mod protocol;
pub mod text;
pub mod transport;

pub use bar_anchor::{Anchor, Edge};

/// Unix time in seconds.
pub fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or_default()
}

/// Unix time in whole seconds.
pub fn now_secs() -> i64 {
    now() as i64
}
