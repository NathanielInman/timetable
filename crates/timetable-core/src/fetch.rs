//! Reading the calendar: from a file, from an address in the config, or
//! from an address a command prints (`broker get TIMETABLE_ICAL_URL`).
//!
//! The address of a Google calendar's secret iCal feed is a password: it
//! is kept in memory only, never logged, cached or put in an error message.
//! [`redact`] scrubs it from anything that might carry it.

use crate::config::Config;
use crate::model::Source;
use anyhow::{anyhow, Result};
use std::io::Read;
use std::time::{Duration, Instant};

/// The largest feed read. Google's carries years of history; 11 MB is a
/// busy work calendar.
pub const MAX_BYTES: u64 = 96 * 1024 * 1024;

pub fn source(cfg: &Config) -> Source {
    let c = &cfg.calendar;
    if !c.file.trim().is_empty() {
        Source::File
    } else if !c.url.trim().is_empty() {
        Source::Url
    } else if !c.url_command.trim().is_empty() {
        Source::Command
    } else {
        Source::None
    }
}

/// Where the feed is, once the command has run.
#[derive(Clone, PartialEq)]
pub enum Location {
    File(std::path::PathBuf),
    Url(String),
}

// Never print the address, even in a debug dump.
impl std::fmt::Debug for Location {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Location::File(p) => write!(f, "File({})", p.display()),
            Location::Url(_) => write!(f, "Url(<hidden>)"),
        }
    }
}

/// `webcal://` is `https://`.
pub fn normalize(url: &str) -> Result<String> {
    let u = url.trim();
    let u = match u.strip_prefix("webcal://") {
        Some(rest) => format!("https://{rest}"),
        None => u.to_string(),
    };
    if u.starts_with("https://") || u.starts_with("http://") {
        Ok(u)
    } else {
        Err(anyhow!("the calendar address is not an http(s) address"))
    }
}

/// Runs `url_command` with `sh -c` and reads the address from the first
/// non-empty line it prints. At most 20 seconds.
pub fn run_url_command(cmd: &str) -> Result<String> {
    let cmd = crate::paths::expand_command(cmd);
    let mut child = std::process::Command::new("sh")
        .arg("-c")
        .arg(&cmd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| anyhow!("cannot run calendar.url_command: {e}"))?;
    let deadline = Instant::now() + Duration::from_secs(20);
    let status = loop {
        if let Some(s) = child.try_wait()? {
            break s;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(anyhow!("calendar.url_command took longer than 20 seconds"));
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let mut out = String::new();
    if let Some(mut o) = child.stdout.take() {
        o.read_to_string(&mut out)?;
    }
    if !status.success() {
        let mut err = String::new();
        if let Some(mut e) = child.stderr.take() {
            let _ = e.read_to_string(&mut err);
        }
        let first = err
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .trim();
        let first: String = first.chars().take(160).collect();
        return Err(anyhow!(
            "calendar.url_command failed ({}){}",
            status,
            if first.is_empty() {
                String::new()
            } else {
                format!(": {}", redact(&first, &out))
            }
        ));
    }
    let line = out
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .ok_or_else(|| anyhow!("calendar.url_command printed nothing"))?;
    normalize(line)
}

/// Finds the feed: a file, the configured address, or the command's.
pub fn locate(cfg: &Config) -> Result<Location> {
    let c = &cfg.calendar;
    match source(cfg) {
        Source::File => Ok(Location::File(crate::paths::expand(&c.file))),
        Source::Url => Ok(Location::Url(normalize(&c.url)?)),
        Source::Command => Ok(Location::Url(run_url_command(&c.url_command)?)),
        Source::None => Err(anyhow!("no calendar is configured")),
    }
}

fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hex = std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("");
            if let Ok(v) = u8::from_str_radix(hex, 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The calendar id in a Google secret address, which for a primary
/// calendar is the owner's address:
/// `https://calendar.google.com/calendar/ical/<id>/private-<key>/basic.ics`.
pub fn owner_from_url(url: &str) -> Option<String> {
    let i = url.find("/ical/")?;
    let rest = &url[i + 6..];
    let j = rest.find("/private-").or_else(|| rest.find("/public/"))?;
    let id = decode(&rest[..j]).trim().to_ascii_lowercase();
    (id.contains('@')).then_some(id)
}

/// Removes the address, and the secret part of a Google address, from a
/// message.
pub fn redact(msg: &str, url: &str) -> String {
    let mut m = msg.to_string();
    let url = url.trim();
    if url.len() > 8 {
        m = m.replace(url, "<calendar address>");
    }
    // Any "private-<key>" left over, whatever the address looked like.
    let mut out = String::with_capacity(m.len());
    let mut rest = m.as_str();
    while let Some(i) = rest.find("private-") {
        out.push_str(&rest[..i]);
        out.push_str("private-<hidden>");
        let after = &rest[i + "private-".len()..];
        let end = after
            .find(|c: char| c == '/' || c.is_whitespace() || c == '"' || c == '\'')
            .unwrap_or(after.len());
        rest = &after[end..];
    }
    out.push_str(rest);
    m = out;
    m
}

/// Why a fetch failed, with nothing secret in it. `auth` says whether the
/// address itself may be stale (so the command should run again).
#[derive(Debug, Clone, PartialEq)]
pub struct FetchError {
    pub message: String,
    pub auth: bool,
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// Downloads a feed.
pub fn download(url: &str, timeout: Duration) -> std::result::Result<String, FetchError> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .user_agent(concat!("timetable/", env!("CARGO_PKG_VERSION")))
        .build()
        .into();
    let fail = |message: String, auth: bool| FetchError { message, auth };
    let mut resp = match agent.get(url).call() {
        Ok(r) => r,
        Err(ureq::Error::StatusCode(code)) => {
            return Err(fail(
                format!("the calendar answered HTTP {code}"),
                matches!(code, 401 | 403 | 404 | 410),
            ))
        }
        Err(ureq::Error::Timeout(_)) => {
            return Err(fail(
                format!("no answer within {} s", timeout.as_secs()),
                false,
            ))
        }
        Err(e) => return Err(fail(redact(&e.to_string(), url), false)),
    };
    resp.body_mut()
        .with_config()
        .limit(MAX_BYTES)
        .read_to_string()
        .map_err(|e| match e {
            ureq::Error::Timeout(_) => {
                fail(format!("no answer within {} s", timeout.as_secs()), false)
            }
            e => fail(redact(&e.to_string(), url), false),
        })
}

/// Reads a feed from wherever it is.
pub fn read(loc: &Location, timeout: Duration) -> std::result::Result<String, FetchError> {
    match loc {
        Location::File(p) => std::fs::read_to_string(p).map_err(|e| FetchError {
            message: format!("{}: {e}", p.display()),
            auth: false,
        }),
        Location::Url(u) => download(u, timeout),
    }
}

/// The owner to match declines against: the config, else the address.
pub fn owner(cfg: &Config, loc: &Location) -> Option<String> {
    let o = cfg.calendar.owner.trim();
    if !o.is_empty() {
        return Some(o.to_ascii_lowercase());
    }
    match loc {
        Location::Url(u) => owner_from_url(u),
        Location::File(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOGLE: &str =
        "https://calendar.google.com/calendar/ical/jane.doe%40example.com/private-0123456789abcdef0123456789abcdef/basic.ics";

    #[test]
    fn owner_comes_from_the_address() {
        assert_eq!(
            owner_from_url(GOOGLE).as_deref(),
            Some("jane.doe@example.com")
        );
        assert_eq!(owner_from_url("https://example.com/cal.ics"), None);
        // A shared calendar's id is not an address.
        assert_eq!(
            owner_from_url("https://calendar.google.com/calendar/ical/c_abc123%40group.calendar.google.com/private-x/basic.ics").as_deref(),
            Some("c_abc123@group.calendar.google.com")
        );
    }

    #[test]
    fn the_address_never_survives_redaction() {
        let m = format!("io error connecting to {GOOGLE}: refused");
        let r = redact(&m, GOOGLE);
        assert!(!r.contains("0123456789abcdef"), "{r}");
        assert!(r.contains("<calendar address>"));
        let other =
            "https://calendar.google.com/calendar/ical/x/private-deadbeef00/basic.ics failed";
        let r = redact(other, GOOGLE);
        assert!(!r.contains("deadbeef00"), "{r}");
        assert!(r.contains("private-<hidden>"));
    }

    #[test]
    fn addresses() {
        assert_eq!(
            normalize("webcal://example.com/a.ics").unwrap(),
            "https://example.com/a.ics"
        );
        assert!(normalize("ftp://x").is_err());
        assert_eq!(
            format!("{:?}", Location::Url(GOOGLE.into())),
            "Url(<hidden>)"
        );
    }

    #[test]
    fn sources() {
        let mut c = Config::default();
        assert_eq!(source(&c), Source::None);
        c.calendar.url_command = "broker get X".into();
        assert_eq!(source(&c), Source::Command);
        c.calendar.file = "/tmp/a.ics".into();
        assert_eq!(source(&c), Source::File);
    }

    #[test]
    fn a_command_supplies_the_address() {
        assert_eq!(
            run_url_command("echo; echo https://example.com/a.ics").unwrap(),
            "https://example.com/a.ics"
        );
        assert!(run_url_command("exit 3").is_err());
        assert!(run_url_command("true").is_err());
    }
}
