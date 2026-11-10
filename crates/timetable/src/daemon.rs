//! The daemon: reads the calendar on a background thread every few
//! minutes, keeps the agenda, serves the socket, feeds the bar module once
//! a minute (once a second only while a focus block runs), sends reminders
//! and the end of a focus block, and starts the drawer the first time it is
//! asked for.
//!
//! The fetch is the only network or process work, and it runs on its own
//! thread with a timeout; the bar line is computed from memory under a
//! short lock and nothing waits on the fetch. The address of the calendar
//! stays in this process's memory.

use anyhow::Result;
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Write};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime};
use timetable_core::config::Config;
use timetable_core::fetch::{self, Location};
use timetable_core::model::{Event, Focus, Snapshot, Source};
use timetable_core::protocol::{Event as BoardEvent, FocusAction, Reply, Request, SubKind};
use timetable_core::transport::{Listener, Stream};
use timetable_core::{agenda, expand, ical, now, now_secs, paths};

/// How often the config file is checked for changes.
const CONFIG_EVERY: Duration = Duration::from_secs(2);
/// After a failed fetch, try again sooner than the refresh interval.
const RETRY: Duration = Duration::from_secs(60);

struct State {
    cfg: Config,
    cfg_error: Option<String>,
    cfg_mtime: Option<SystemTime>,
    events: Vec<Event>,
    fetched_at: Option<f64>,
    attempted_at: Option<f64>,
    error: Option<String>,
    cached: bool,
    /// The address `url_command` printed, kept until it stops working.
    location: Option<Location>,
    focus: Option<Focus>,
    /// Reminders already sent: `uid@start#minutes`.
    reminded: HashSet<String>,
    /// A fetch was asked for (start, `timetable refresh`, a config change).
    fetch_now: bool,
    snap: Snapshot,
    /// The snapshot last sent to the drawer, without its time.
    board_sig: String,
    boards: Vec<Stream>,
    bars: Vec<Stream>,
    bar_line: String,
    pending: Option<BoardEvent>,
    board_started: Option<Instant>,
    /// The drawer says it is on screen.
    visible: bool,
}

pub struct Daemon {
    state: Mutex<State>,
    /// Wakes the ticker (a new agenda, a focus change).
    tick: Condvar,
    /// Wakes the fetcher.
    fetch: Condvar,
}

type Shared = Arc<Daemon>;

impl Daemon {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

fn send_line(s: &mut Stream, line: &str) -> bool {
    s.write_all(line.as_bytes())
        .and_then(|_| s.write_all(b"\n"))
        .and_then(|_| s.flush())
        .is_ok()
}

fn broadcast(subs: &mut Vec<Stream>, line: &str) -> usize {
    subs.retain_mut(|s| send_line(s, line));
    subs.len()
}

fn event_line(e: &BoardEvent) -> String {
    serde_json::to_string(e).unwrap_or_default()
}

fn config_mtime() -> Option<SystemTime> {
    std::fs::metadata(paths::config_file())
        .and_then(|m| m.modified())
        .ok()
}

// ---- persistence

#[derive(serde::Serialize, serde::Deserialize)]
struct Cache {
    fetched_at: f64,
    events: Vec<Event>,
}

fn load_cache() -> Option<Cache> {
    let text = std::fs::read_to_string(paths::agenda_cache()).ok()?;
    serde_json::from_str(&text).ok()
}

fn save_cache(fetched_at: f64, events: &[Event]) {
    let body = serde_json::json!({ "fetched_at": fetched_at, "events": events });
    if let Err(e) = paths::write_private(&paths::agenda_cache(), body.to_string().as_bytes()) {
        eprintln!("timetable: cannot write the agenda cache: {e}");
    }
}

fn load_focus() -> Option<Focus> {
    let text = std::fs::read_to_string(paths::focus_file()).ok()?;
    serde_json::from_str::<Focus>(&text).ok()
}

fn save_focus(f: Option<&Focus>) {
    match f {
        Some(f) => {
            let _ = paths::write_private(
                &paths::focus_file(),
                serde_json::to_string(f).unwrap_or_default().as_bytes(),
            );
        }
        None => {
            let _ = std::fs::remove_file(paths::focus_file());
        }
    }
}

fn load_reminded() -> HashSet<String> {
    std::fs::read_to_string(paths::reminded_file())
        .ok()
        .and_then(|t| serde_json::from_str::<Vec<String>>(&t).ok())
        .unwrap_or_default()
        .into_iter()
        .collect()
}

/// The start time inside a reminder key (`uid@start#minutes`).
fn key_start(k: &str) -> Option<i64> {
    let (rest, _) = k.rsplit_once('#')?;
    rest.rsplit_once('@')?.1.parse().ok()
}

fn save_reminded(set: &mut HashSet<String>, now: i64) {
    set.retain(|k| key_start(k).is_some_and(|s| s > now - 86400));
    let mut v: Vec<&String> = set.iter().collect();
    v.sort();
    let _ = paths::write_private(
        &paths::reminded_file(),
        serde_json::to_string(&v).unwrap_or_default().as_bytes(),
    );
}

// ---- the snapshot and what it feeds

fn snapshot(st: &State) -> Snapshot {
    let n = now_secs();
    Snapshot {
        version: env!("CARGO_PKG_VERSION").into(),
        now: now(),
        source: fetch::source(&st.cfg),
        events: st.events.clone(),
        fetched_at: st.fetched_at,
        attempted_at: st.attempted_at,
        error: st.error.clone(),
        cached: st.cached,
        focus: st.focus.filter(|f| f.running(n)),
        config_error: st.cfg_error.clone(),
    }
}

/// Recomputes the snapshot and pushes what changed: the bar line to bar
/// subscribers, the snapshot to the drawer.
fn refresh(st: &mut State) {
    let snap = snapshot(st);
    let bar = serde_json::to_string(&crate::bar::line(&snap, &st.cfg, snap.now as i64))
        .unwrap_or_default();
    if bar != st.bar_line {
        st.bar_line = bar.clone();
        broadcast(&mut st.bars, &bar);
    }
    let sig = serde_json::to_string(&Snapshot {
        now: 0.0,
        ..snap.clone()
    })
    .unwrap_or_default();
    if sig != st.board_sig {
        st.board_sig = sig;
        let ev = event_line(&BoardEvent::Snapshot {
            snapshot: Box::new(snap.clone()),
        });
        broadcast(&mut st.boards, &ev);
    }
    st.snap = snap;
}

// ---- reading the calendar

/// One fetch, run on the fetcher thread without the lock.
fn fetch_once(d: &Shared) {
    let (cfg, known) = {
        let st = d.lock();
        (st.cfg.clone(), st.location.clone())
    };
    let src = fetch::source(&cfg);
    let finish = |st: &mut State| {
        st.attempted_at = Some(now());
        refresh(st);
        d.tick.notify_all();
    };
    if src == Source::None {
        let mut st = d.lock();
        st.events.clear();
        st.fetched_at = None;
        st.error = None;
        st.location = None;
        finish(&mut st);
        return;
    }
    let loc = match known.filter(|_| src == Source::Command) {
        Some(l) => l,
        None => match fetch::locate(&cfg) {
            Ok(l) => {
                if src == Source::Command {
                    d.lock().location = Some(l.clone());
                }
                l
            }
            Err(e) => {
                let mut st = d.lock();
                st.error = Some(format!("{e:#}"));
                finish(&mut st);
                return;
            }
        },
    };
    let timeout = Duration::from_secs(cfg.calendar.timeout.max(5));
    let started = Instant::now();
    match fetch::read(&loc, timeout) {
        Ok(text) => {
            let zone = cfg.local_zone();
            let feed = ical::parse(&text, fetch::owner(&cfg, &loc).as_deref());
            drop(text);
            let n = now_secs();
            let (from, to) =
                expand::window(n, zone, cfg.calendar.past_days, cfg.calendar.future_days);
            let x = expand::expand(&feed, zone, from, to);
            eprintln!(
                "timetable: read {} events, {} in the window, in {:.1} s{}",
                feed.events.len(),
                x.events.len(),
                started.elapsed().as_secs_f64(),
                if x.bad_rules > 0 {
                    format!(" ({} rules unreadable)", x.bad_rules)
                } else {
                    String::new()
                }
            );
            let at = now();
            save_cache(at, &x.events);
            let mut st = d.lock();
            st.events = x.events;
            st.fetched_at = Some(at);
            st.error = None;
            st.cached = false;
            if let Some(f) = st.focus {
                let c = agenda::clamp_focus(f, &st.events, n, &st.cfg);
                if c != f {
                    st.focus = Some(c);
                    save_focus(Some(&c));
                }
            }
            finish(&mut st);
        }
        Err(e) => {
            eprintln!("timetable: fetch failed: {e}");
            let mut st = d.lock();
            st.error = Some(e.message.clone());
            if e.auth {
                // The address may have changed: ask the command again.
                st.location = None;
            }
            finish(&mut st);
        }
    }
}

fn fetcher(d: Shared) {
    loop {
        fetch_once(&d);
        let mut st = d.lock();
        let every = if st.error.is_some() {
            RETRY.min(Duration::from_secs(st.cfg.calendar.refresh.max(1) * 60))
        } else {
            Duration::from_secs(st.cfg.calendar.refresh.max(1) * 60)
        };
        let deadline = Instant::now() + every;
        while !st.fetch_now {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            st = d
                .fetch
                .wait_timeout(st, left)
                .map(|(g, _)| g)
                .unwrap_or_else(|e| e.into_inner().0);
        }
        st.fetch_now = false;
    }
}

// ---- the ticker: the clock, focus blocks, reminders, config

fn housekeeping(d: &Shared, st: &mut State) {
    let n = now_secs();
    let zone = st.cfg.local_zone();
    // A focus block that ended.
    if let Some(f) = st.focus.filter(|f| !f.running(n)) {
        st.focus = None;
        save_focus(None);
        // Do not announce one that ended long ago (the machine slept).
        if n - f.end < 120 {
            let next = agenda::upcoming(&st.events, n)
                .filter(|e| e.start - n < 3600)
                .map(|e| {
                    format!(
                        "{} at {}",
                        e.title_or_default(),
                        crate::bar::hhmm(e.start, zone)
                    )
                });
            act_notify(
                "Focus block over",
                &match next {
                    Some(t) => format!("Next: {t}"),
                    None => format!("{} min of focus", (f.end - f.start) / 60),
                },
                None,
                d,
            );
        }
    }
    // Reminders.
    let due = agenda::due_reminders(&st.events, n, &st.cfg.reminders.minutes, &st.reminded);
    if !due.is_empty() {
        for r in &due {
            st.reminded.insert(r.key.clone());
            let e = &r.event;
            let mut body = format!(
                "{} to {}",
                crate::bar::hhmm(e.start, zone),
                crate::bar::hhmm(e.end, zone)
            );
            if let Some(k) = &e.kind {
                body.push_str(&format!(" \u{b7} {}", timetable_core::links::label(k)));
            }
            let summary = format!("{} in {} min", e.title_or_default(), r.minutes);
            let join = e.link.as_ref().map(|_| e.key());
            act_notify(&summary, &body, join, d);
        }
        save_reminded(&mut st.reminded, n);
    }
}

/// A notification; with a key, its Join button joins that event.
fn act_notify(summary: &str, body: &str, join_key: Option<String>, d: &Shared) {
    let action = join_key.map(|key| {
        let d = d.clone();
        let f: crate::act::OnAction = Box::new(move || {
            let _ = do_join(&d, Some(&key), false);
        });
        ("Join", f)
    });
    crate::act::notify(summary, body, action);
}

fn ticker(d: Shared) {
    let mut config_at = Instant::now();
    loop {
        let mut st = d.lock();
        let n = now();
        // The next minute (the clock), or the next second while a focus
        // block counts down.
        let focus = st.focus.is_some_and(|f| f.running(n as i64));
        let step = if focus { 1.0 } else { 60.0 };
        let next = ((n / step).floor() + 1.0) * step + 0.02;
        let wait = Duration::from_secs_f64((next - n).clamp(0.01, 60.0)).min(CONFIG_EVERY);
        st = d
            .tick
            .wait_timeout(st, wait)
            .map(|(g, _)| g)
            .unwrap_or_else(|e| e.into_inner().0);
        if config_at.elapsed() >= CONFIG_EVERY {
            config_at = Instant::now();
            let m = config_mtime();
            if m != st.cfg_mtime {
                st.cfg_mtime = m;
                let (cfg, err) = Config::load();
                if let Some(e) = &err {
                    eprintln!("timetable: {e}");
                }
                let source_changed =
                    cfg.calendar != st.cfg.calendar || cfg.clock.zone != st.cfg.clock.zone;
                st.cfg = cfg;
                st.cfg_error = err;
                if source_changed {
                    st.location = None;
                    st.fetch_now = true;
                    d.fetch.notify_all();
                }
            }
        }
        housekeeping(&d, &mut st);
        refresh(&mut st);
    }
}

// ---- requests

fn spawn_board() {
    let exe = std::env::current_exe().ok();
    let local = exe
        .as_ref()
        .and_then(|e| e.parent())
        .map(|dir| dir.join("timetable-board"))
        .filter(|p| p.exists());
    let bin = local
        .map(|p| p.into_os_string())
        .unwrap_or_else(|| "timetable-board".into());
    let log = paths::board_log();
    if let Some(dir) = log.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut cmd = std::process::Command::new(&bin);
    cmd.stdin(std::process::Stdio::null());
    if let Ok(f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)
    {
        if let Ok(f2) = f.try_clone() {
            cmd.stdout(f).stderr(f2);
        }
    }
    match cmd.spawn() {
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
        Err(e) => eprintln!("timetable: cannot start {}: {e}", bin.to_string_lossy()),
    }
}

/// Delivers a show or toggle to the drawer, starting it when none is
/// connected.
fn to_board(st: &mut State, ev: BoardEvent) {
    if broadcast(&mut st.boards, &event_line(&ev)) > 0 {
        return;
    }
    let show = match ev {
        BoardEvent::Toggle { anchor } | BoardEvent::Show { anchor } => BoardEvent::Show { anchor },
        other => other,
    };
    if matches!(show, BoardEvent::Hide) {
        return;
    }
    st.pending = Some(show);
    if st
        .board_started
        .is_none_or(|t| t.elapsed() > Duration::from_secs(5))
    {
        st.board_started = Some(Instant::now());
        spawn_board();
    }
}

/// Joins the event with `key`, or the current or next meeting.
fn do_join(d: &Shared, key: Option<&str>, dry_run: bool) -> Result<String, String> {
    let (cfg, target) = {
        let st = d.lock();
        let n = now_secs();
        let zone = st.cfg.local_zone();
        let target = match key {
            Some(k) => st.events.iter().find(|e| e.key() == k).cloned(),
            None => agenda::join_target(&st.events, n, &st.cfg, zone).cloned(),
        };
        (st.cfg.clone(), target)
    };
    let Some(e) = target else {
        return Err(match key {
            Some(_) => "that event is no longer in the agenda".into(),
            None => "no meeting with a link is on now or later today".into(),
        });
    };
    if dry_run {
        return Ok(format!(
            "Would join {} ({}) at {}",
            e.title_or_default(),
            timetable_core::links::label(e.kind.as_deref().unwrap_or("link")),
            e.link.as_deref().unwrap_or_default()
        ));
    }
    // An open drawer holds the keyboard on its monitor, and Hyprland
    // keeps that workspace focused while it does: close it first.
    let open = {
        let mut st = d.lock();
        if st.visible {
            st.pending = None;
            broadcast(&mut st.boards, &event_line(&BoardEvent::Hide));
        }
        st.visible
    };
    if open {
        std::thread::sleep(Duration::from_millis(450));
    }
    crate::act::join(&cfg, &e).map_err(|e| format!("{e:#}"))
}

fn focus(d: &Shared, action: FocusAction) -> Reply {
    let mut st = d.lock();
    let n = now_secs();
    let zone = st.cfg.local_zone();
    let running = st.focus.is_some_and(|f| f.running(n));
    let start = match action {
        FocusAction::Start => true,
        FocusAction::Stop => false,
        FocusAction::Toggle => !running,
    };
    let reply = if start {
        match agenda::focus_plan(&st.events, n, &st.cfg, zone) {
            Ok(p) => {
                let f = Focus {
                    start: p.start,
                    end: p.end,
                };
                st.focus = Some(f);
                save_focus(Some(&f));
                Reply {
                    message: Some(format!(
                        "Focus until {} ({} min{})",
                        crate::bar::hhmm(p.end, zone),
                        p.minutes(),
                        p.before
                            .as_ref()
                            .map(|e| format!(", before {}", e.title_or_default()))
                            .unwrap_or_default()
                    )),
                    ..Reply::ok()
                }
            }
            Err(e) => Reply::err(e),
        }
    } else {
        let was = st.focus.take();
        save_focus(None);
        Reply {
            message: Some(match was.filter(|f| f.running(n)) {
                Some(f) => format!("Focus stopped after {} min", (n - f.start) / 60),
                None => "No focus block was running".into(),
            }),
            ..Reply::ok()
        }
    };
    refresh(&mut st);
    d.tick.notify_all();
    reply
}

fn handle(d: Shared, stream: Stream) {
    let Ok(read_half) = stream.try_clone() else {
        return;
    };
    let mut writer = stream;
    let reader = BufReader::new(read_half);
    for line in reader.lines() {
        let Ok(line) = line else { return };
        if line.trim().is_empty() {
            continue;
        }
        let req: Request = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let _ = send_line(
                    &mut writer,
                    &serde_json::to_string(&Reply::err(format!("bad request: {e}")))
                        .unwrap_or_default(),
                );
                continue;
            }
        };
        let reply = match req {
            Request::Ping => Reply {
                version: Some(env!("CARGO_PKG_VERSION").into()),
                ..Reply::ok()
            },
            Request::Snapshot => Reply {
                snapshot: Some(Box::new(snapshot(&d.lock()))),
                ..Reply::ok()
            },
            Request::Subscribe { kind } => {
                let _ = writer.set_write_timeout(Some(Duration::from_secs(2)));
                let mut st = d.lock();
                match kind {
                    SubKind::Board => {
                        let mut ok = send_line(
                            &mut writer,
                            &event_line(&BoardEvent::Snapshot {
                                snapshot: Box::new(snapshot(&st)),
                            }),
                        );
                        if let Some(p) = st.pending.take() {
                            ok &= send_line(&mut writer, &event_line(&p));
                        }
                        if ok {
                            st.boards.push(writer);
                        }
                    }
                    SubKind::Bar => {
                        let line = st.bar_line.clone();
                        if line.is_empty() || send_line(&mut writer, &line) {
                            st.bars.push(writer);
                        }
                    }
                }
                return;
            }
            Request::Toggle { anchor } => {
                to_board(&mut d.lock(), BoardEvent::Toggle { anchor });
                Reply::ok()
            }
            Request::Show { anchor } => {
                to_board(&mut d.lock(), BoardEvent::Show { anchor });
                Reply::ok()
            }
            Request::Hide => {
                let mut st = d.lock();
                st.pending = None;
                broadcast(&mut st.boards, &event_line(&BoardEvent::Hide));
                Reply::ok()
            }
            Request::BoardVisible { visible } => {
                d.lock().visible = visible;
                Reply::ok()
            }
            Request::Join { key, dry_run } => match do_join(&d, key.as_deref(), dry_run) {
                Ok(m) => Reply {
                    message: Some(m),
                    ..Reply::ok()
                },
                Err(e) => Reply::err(e),
            },
            Request::Refresh => {
                let mut st = d.lock();
                st.fetch_now = true;
                // A refresh also asks the command for the address again.
                st.location = None;
                d.fetch.notify_all();
                Reply::ok()
            }
            Request::Focus { action } => focus(&d, action),
        };
        if !send_line(
            &mut writer,
            &serde_json::to_string(&reply).unwrap_or_default(),
        ) {
            return;
        }
    }
}

pub fn run() -> Result<i32> {
    let listener = Listener::bind()?;
    let (cfg, cfg_error) = Config::load();
    if let Some(e) = &cfg_error {
        eprintln!("timetable: {e}");
    }
    let n = now_secs();
    let cache = load_cache().filter(|c| n - (c.fetched_at as i64) < 7 * 86400);
    let focus = load_focus().filter(|f| f.running(n));
    if focus.is_none() {
        save_focus(None);
    }
    let has_source = fetch::source(&cfg) != Source::None;
    let mut st = State {
        cfg,
        cfg_error,
        cfg_mtime: config_mtime(),
        events: vec![],
        fetched_at: None,
        attempted_at: None,
        error: None,
        cached: false,
        location: None,
        focus,
        reminded: load_reminded(),
        fetch_now: false,
        snap: Snapshot::default(),
        board_sig: String::new(),
        boards: vec![],
        bars: vec![],
        bar_line: String::new(),
        pending: None,
        board_started: None,
        visible: false,
    };
    if let (Some(c), true) = (cache, has_source) {
        st.events = c.events;
        st.fetched_at = Some(c.fetched_at);
        st.cached = true;
    }
    refresh(&mut st);
    let d: Shared = Arc::new(Daemon {
        state: Mutex::new(st),
        tick: Condvar::new(),
        fetch: Condvar::new(),
    });
    {
        let d = d.clone();
        std::thread::spawn(move || fetcher(d));
    }
    {
        let d = d.clone();
        std::thread::spawn(move || ticker(d));
    }
    eprintln!("timetable: listening on {}", paths::socket_path().display());
    loop {
        match listener.accept() {
            Ok(stream) => {
                let d = d.clone();
                std::thread::spawn(move || handle(d, stream));
            }
            Err(e) => {
                eprintln!("timetable: accept failed: {e}");
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reminder_keys() {
        assert_eq!(key_start("abc@google.com@1794333600#5"), Some(1794333600));
        assert_eq!(key_start("bad"), None);
    }
}
