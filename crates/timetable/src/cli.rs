//! Command-line entry points.

use anyhow::{anyhow, Result};
use std::io::Write;
use std::time::{Duration, Instant};
use timetable_core::config::Config;
use timetable_core::model::{Snapshot, Source};
use timetable_core::protocol::{FocusAction, Reply, Request, SubKind};
use timetable_core::text::countdown;
use timetable_core::{agenda, expand, fetch, links, now_secs, paths};

const HELP: &str = "\
timetable: your calendar in the waybar clock

Usage:
  timetable bar                     the waybar module (prints JSON lines)
  timetable toggle [--center]       open or close the drawer from the clicked clock
  timetable show [--center] | hide  open or close the drawer
  timetable join [--dry-run]        switch to the meeting workspace and open the
                                    current or next meeting's link
  timetable focus start|stop|toggle a focus block until just before the next event
  timetable focus status            the running block
  timetable agenda [--days N] [--json]
                                    today's events (and N-1 more days) as text
  timetable refresh                 read the calendar again now
  timetable status [--json]         where the calendar stands
  timetable doctor                  check the setup
  timetable daemon                  run the daemon in the foreground
  timetable --version

Config: ~/.config/timetable/config.toml
";

fn flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

fn value<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

pub fn run(args: &[String]) -> Result<i32> {
    let rest = args.get(1..).unwrap_or(&[]);
    match args.first().map(String::as_str) {
        None | Some("help" | "-h" | "--help") => {
            print!("{HELP}");
            Ok(0)
        }
        Some("-V" | "--version" | "version") => {
            println!("timetable {}", env!("CARGO_PKG_VERSION"));
            Ok(0)
        }
        Some("daemon") => crate::daemon::run(),
        Some("bar") => bar(),
        Some(cmd @ ("toggle" | "show")) => open(cmd == "toggle", rest),
        Some("hide") => {
            if timetable_core::transport::daemon_alive() {
                send(&Request::Hide)?;
            }
            Ok(0)
        }
        Some("join") => command(Request::Join {
            key: value(rest, "--key").map(String::from),
            dry_run: flag(rest, "--dry-run"),
        }),
        Some("focus") => match rest.first().map(String::as_str) {
            Some("start") => command(Request::Focus {
                action: FocusAction::Start,
            }),
            Some("stop") => command(Request::Focus {
                action: FocusAction::Stop,
            }),
            Some("toggle") | None => command(Request::Focus {
                action: FocusAction::Toggle,
            }),
            Some("status") => focus_status(),
            Some(o) => Err(anyhow!(
                "unknown focus command `{o}` (start, stop, toggle, status)"
            )),
        },
        Some("refresh") => refresh(),
        Some("agenda") => {
            let days = value(rest, "--days")
                .map(|d| {
                    d.parse::<u32>()
                        .map_err(|_| anyhow!("--days takes a number"))
                })
                .transpose()?
                .unwrap_or(1);
            agenda_cmd(days.clamp(1, 14), flag(rest, "--json"))
        }
        Some("status") => status(flag(rest, "--json")),
        Some("doctor") => doctor(),
        Some(other) => Err(anyhow!(
            "unknown command `{other}` (try `timetable --help`)"
        )),
    }
}

fn reply(req: &Request) -> Result<Reply> {
    let r = timetable_core::protocol::request(req)?;
    match r.error {
        Some(e) => Err(anyhow!(e)),
        None => Ok(r),
    }
}

fn send(req: &Request) -> Result<()> {
    reply(req).map(|_| ())
}

/// Bar clicks and keys: start the daemon if needed, then ask it.
fn command(req: Request) -> Result<i32> {
    ensure_daemon()?;
    if let Some(m) = reply(&req)?.message {
        println!("{m}");
    }
    Ok(0)
}

/// Starts the daemon in the background when it is not running, and waits
/// for its socket.
fn ensure_daemon() -> Result<()> {
    if timetable_core::transport::daemon_alive() {
        return Ok(());
    }
    let exe = std::env::current_exe()?;
    let log = paths::daemon_log();
    std::fs::create_dir_all(paths::state_dir())?;
    let out = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)?;
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("daemon")
        .stdin(std::process::Stdio::null())
        .stdout(out.try_clone()?)
        .stderr(out);
    {
        use std::os::unix::process::CommandExt;
        // Its own session, so restarting the bar does not take the daemon
        // with it.
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }
    let mut child = cmd.spawn()?;
    for _ in 0..60 {
        std::thread::sleep(Duration::from_millis(50));
        if timetable_core::transport::daemon_alive() {
            std::thread::spawn(move || child.wait());
            return Ok(());
        }
        // Lost the race to the other bar's daemon: it answers shortly.
        if let Ok(Some(_)) = child.try_wait() {
            continue;
        }
    }
    Err(anyhow!("the daemon did not start; see {}", log.display()))
}

fn open(toggle: bool, args: &[String]) -> Result<i32> {
    let (cfg, _) = Config::load();
    // Ask the compositor first: the cursor is still on the clock.
    let anchor = bar_anchor::hypr::anchor(&cfg.bar.namespace, flag(args, "--center"));
    ensure_daemon()?;
    send(&if toggle {
        Request::Toggle { anchor }
    } else {
        Request::Show { anchor }
    })?;
    Ok(0)
}

fn bar() -> Result<i32> {
    let mut out = std::io::stdout().lock();
    let mut last = String::new();
    // Starts the daemon when it is missing (first run, a crash, an
    // upgrade), at most every 10 seconds, and keeps trying.
    let mut started: Option<Instant> = None;
    loop {
        match timetable_core::protocol::subscribe_raw(SubKind::Bar) {
            Ok(lines) => {
                for line in lines {
                    let Ok(line) = line else { break };
                    if line != last {
                        writeln!(out, "{line}")?;
                        out.flush()?;
                        last = line;
                    }
                }
            }
            Err(_) if started.is_none_or(|t| t.elapsed() > Duration::from_secs(10)) => {
                started = Some(Instant::now());
                let _ = ensure_daemon();
                continue;
            }
            Err(_) => {}
        }
        // Without the daemon the clock still runs.
        let (cfg, _) = Config::load();
        let offline = serde_json::to_string(&crate::bar::offline(&cfg, now_secs()))?;
        if last != offline {
            writeln!(out, "{offline}")?;
            out.flush()?;
            last = offline;
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

fn snapshot() -> Option<Snapshot> {
    timetable_core::protocol::request(&Request::Snapshot)
        .ok()?
        .snapshot
        .map(|s| *s)
}

/// A snapshot with a calendar read (or an error), waiting up to `wait`
/// for the first fetch.
fn snapshot_with_data(wait: Duration) -> Result<Snapshot> {
    ensure_daemon()?;
    let deadline = Instant::now() + wait;
    loop {
        let s = snapshot().ok_or_else(|| anyhow!("the daemon did not answer"))?;
        if s.source == Source::None || s.fetched_at.is_some() || s.attempted_at.is_some() {
            return Ok(s);
        }
        if Instant::now() > deadline {
            return Ok(s);
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn focus_status() -> Result<i32> {
    let (cfg, _) = Config::load();
    let zone = cfg.local_zone();
    let s = snapshot_with_data(Duration::from_secs(5))?;
    let n = now_secs();
    match s.focus.filter(|f| f.running(n)) {
        Some(f) => println!(
            "Focus until {} ({} left)",
            crate::bar::hhmm(f.end, zone),
            countdown(f.end - n)
        ),
        None => match agenda::focus_plan(&s.events, n, &cfg, zone) {
            Ok(p) => println!(
                "No focus block. Starting one now runs {} min, until {}{}.",
                p.minutes(),
                crate::bar::hhmm(p.end, zone),
                p.before
                    .map(|e| format!(", before {}", e.title_or_default()))
                    .unwrap_or_default()
            ),
            Err(e) => println!("No focus block ({e})."),
        },
    }
    Ok(0)
}

fn refresh() -> Result<i32> {
    ensure_daemon()?;
    let before = snapshot().and_then(|s| s.attempted_at);
    send(&Request::Refresh)?;
    let (cfg, _) = Config::load();
    let deadline = Instant::now() + Duration::from_secs(cfg.calendar.timeout.max(5) + 25);
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(250));
        let Some(s) = snapshot() else { continue };
        if s.attempted_at != before && s.attempted_at.is_some() {
            return Ok(match (&s.error, s.source) {
                (_, Source::None) => {
                    println!("No calendar is configured.");
                    0
                }
                (Some(e), _) => {
                    println!("Refresh failed: {e}");
                    1
                }
                (None, _) => {
                    println!(
                        "Refreshed: {} events in the next {} days.",
                        s.events.len(),
                        cfg.calendar.future_days
                    );
                    0
                }
            });
        }
    }
    println!("Still reading the calendar; `timetable status` shows when it is done.");
    Ok(1)
}

fn agenda_cmd(days: u32, json: bool) -> Result<i32> {
    let (cfg, _) = Config::load();
    let zone = cfg.local_zone();
    let s = snapshot_with_data(Duration::from_secs(cfg.calendar.timeout.max(5) + 15))?;
    let n = now_secs();
    let today = expand::local_date(n, zone);
    let mut shown = vec![];
    for i in 0..days {
        let Some(day) = today.checked_add_days(chrono::Days::new(i as u64)) else {
            break;
        };
        shown.push((day, agenda::on_day(&s.events, day, zone)));
    }
    if json {
        let v: Vec<serde_json::Value> = shown
            .iter()
            .map(|(d, evs)| serde_json::json!({ "date": d.to_string(), "events": evs }))
            .collect();
        println!("{}", serde_json::to_string_pretty(&v)?);
        return Ok(0);
    }
    if s.source == Source::None {
        println!("No calendar is configured; see `timetable doctor`.");
        return Ok(1);
    }
    for (i, (day, evs)) in shown.iter().enumerate() {
        if i > 0 {
            println!();
        }
        println!("{}", day.format("%a %b %-d"));
        if evs.is_empty() {
            println!("  nothing");
        }
        for e in evs {
            let when = if e.all_day {
                format!("{:<13}", "all day")
            } else {
                format!(
                    "{} to {}",
                    crate::bar::hhmm(e.start, zone),
                    crate::bar::hhmm(e.end, zone)
                )
            };
            let mark = if !e.all_day && e.running(n) { "*" } else { " " };
            let kind = e
                .kind
                .as_deref()
                .map(|k| format!("  [{}]", links::label(k)))
                .unwrap_or_default();
            println!("{mark} {when}  {}{kind}", e.title_or_default());
        }
    }
    println!("\n{}", crate::bar::freshness(&s, &cfg, n));
    Ok(0)
}

fn status(json: bool) -> Result<i32> {
    let Some(s) = snapshot() else {
        if json {
            println!("null");
        } else {
            println!("The Timetable daemon is not running (`timetable bar` or `timetable daemon` starts it).");
        }
        return Ok(1);
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&s)?);
        return Ok(0);
    }
    let (cfg, _) = Config::load();
    let n = now_secs();
    let st = crate::bar::status(&s, &cfg, n);
    println!("{:<10}{}", "state", st.state.as_str());
    println!("{:<10}{}", "calendar", s.source.as_str());
    println!("{:<10}{}", "agenda", crate::bar::freshness(&s, &cfg, n));
    println!("{:<10}{} events in the window", "events", s.events.len());
    println!("{:<10}{}", "zones", crate::bar::zones_line(&cfg, n));
    if let Some(f) = s.focus.filter(|f| f.running(n)) {
        println!(
            "{:<10}until {}",
            "focus",
            crate::bar::hhmm(f.end, cfg.local_zone())
        );
    }
    if let Some(e) = &s.config_error {
        println!("\nconfig: {e}");
    }
    Ok(0)
}

fn check(ok: Option<bool>, what: &str, detail: &str) {
    let tag = match ok {
        Some(true) => "ok  ",
        Some(false) => "fail",
        None => "info",
    };
    if detail.is_empty() {
        println!("{tag} {what}");
    } else {
        println!("{tag} {what}: {detail}");
    }
}

fn on_path(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file()))
        .unwrap_or(false)
}

fn doctor() -> Result<i32> {
    let mut failed = false;
    let (cfg, err) = Config::load();
    match err {
        Some(e) => {
            failed = true;
            check(Some(false), "config", &e);
        }
        None if paths::config_file().exists() => {
            check(Some(true), "config", &paths::tilde(&paths::config_file()))
        }
        None => check(None, "config", "no config file, using defaults"),
    }
    let zone = cfg.local_zone();
    check(Some(true), "zone", zone.name());
    match fetch::source(&cfg) {
        Source::None => check(
            None,
            "calendar",
            "none configured; the bar is a clock (set [calendar] url_command, url or file)",
        ),
        src => match fetch::locate(&cfg) {
            Ok(loc) => {
                let owner = fetch::owner(&cfg, &loc).is_some();
                let what = match (&loc, src) {
                    (fetch::Location::File(p), _) => format!("file {}", p.display()),
                    (fetch::Location::Url(u), Source::Command) => format!(
                        "url_command gives an {} address{}",
                        if u.starts_with("https://") {
                            "https"
                        } else {
                            "http"
                        },
                        if u.contains("calendar.google.com") {
                            " for Google Calendar"
                        } else {
                            ""
                        }
                    ),
                    (fetch::Location::Url(_), _) => "url".into(),
                };
                check(
                    Some(true),
                    "calendar",
                    &format!(
                        "{what}; declines are matched {}",
                        if owner {
                            "against your address"
                        } else {
                            "against the feed's X-WR-CALNAME, if it is an address"
                        }
                    ),
                );
            }
            Err(e) => {
                failed = true;
                check(Some(false), "calendar", &format!("{e:#}"));
            }
        },
    }
    let alive = timetable_core::transport::daemon_alive();
    check(
        alive.then_some(true),
        "daemon",
        if alive {
            "running"
        } else {
            "not running (`timetable bar` or a command starts it)"
        },
    );
    if let Some(s) = alive.then(snapshot).flatten() {
        let n = now_secs();
        let ok =
            s.source == Source::None || (s.fetched_at.is_some() && !crate::bar::stale(&s, &cfg, n));
        check(
            Some(ok),
            "agenda",
            &format!(
                "{} ({} events in the window)",
                crate::bar::freshness(&s, &cfg, n),
                s.events.len()
            ),
        );
    }
    match bar_anchor::hypr::socket() {
        Some(_) => check(
            Some(true),
            "Hyprland IPC",
            "the drawer opens from the clicked clock; Join switches workspace",
        ),
        None => check(None, "Hyprland IPC", "not found; the drawer opens centered"),
    }
    for (bin, why) in [
        ("notify-send", "reminders and the end of a focus block"),
        ("xdg-open", "the default opener for Join"),
    ] {
        check(
            Some(on_path(bin)),
            bin,
            if on_path(bin) { why } else { "not on PATH" },
        );
    }
    match timetable_core::waybar::config_path() {
        Some(p) => {
            let text = std::fs::read_to_string(&p).unwrap_or_default();
            let has = text.contains("timetable bar");
            check(
                Some(has),
                "waybar config",
                &format!(
                    "{}: {}",
                    paths::tilde(&p),
                    if has {
                        "has `timetable bar`"
                    } else {
                        "no module runs `timetable bar`"
                    }
                ),
            );
        }
        None => check(None, "waybar config", "not found"),
    }
    let board = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join("timetable-board")))
        .filter(|p| p.exists());
    match board {
        Some(p) => check(Some(true), "drawer", &p.display().to_string()),
        None => check(
            None,
            "drawer",
            "timetable-board not next to timetable; it must be on PATH",
        ),
    }
    Ok(if failed { 1 } else { 0 })
}
