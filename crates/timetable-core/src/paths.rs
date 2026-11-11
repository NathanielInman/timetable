//! Where Timetable keeps things. Every location can be moved with an
//! environment variable, which tests and development builds use to stay away
//! from an installed copy.

use std::path::PathBuf;

pub fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// Expands a leading `~`.
pub fn expand(s: &str) -> PathBuf {
    let s = s.trim();
    if s == "~" {
        home()
    } else if let Some(rest) = s.strip_prefix("~/") {
        home().join(rest)
    } else {
        PathBuf::from(s)
    }
}

/// A command line with a leading `~/` expanded, for `sh -c`.
pub fn expand_command(c: &str) -> String {
    let c = c.trim();
    match c.strip_prefix("~/") {
        Some(rest) => format!("{}/{rest}", home().display()),
        None => c.to_string(),
    }
}

fn env_dir(var: &str) -> Option<PathBuf> {
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn xdg_config() -> PathBuf {
    env_dir("XDG_CONFIG_HOME").unwrap_or_else(|| home().join(".config"))
}

pub fn config_dir() -> PathBuf {
    env_dir("TIMETABLE_CONFIG_DIR").unwrap_or_else(|| xdg_config().join("timetable"))
}

/// Logs, the running focus block and the reminders already sent.
pub fn state_dir() -> PathBuf {
    if let Some(d) = env_dir("TIMETABLE_STATE_DIR") {
        return d;
    }
    env_dir("XDG_STATE_HOME")
        .unwrap_or_else(|| home().join(".local/state"))
        .join("timetable")
}

/// The last good agenda, so a restart shows events before the first fetch.
pub fn cache_dir() -> PathBuf {
    if let Some(d) = env_dir("TIMETABLE_CACHE_DIR") {
        return d;
    }
    env_dir("XDG_CACHE_HOME")
        .unwrap_or_else(|| home().join(".cache"))
        .join("timetable")
}

/// Per-user directory for the socket, shared by the daemon, bar module and
/// drawer.
pub fn runtime_dir() -> PathBuf {
    if let Some(d) = env_dir("TIMETABLE_RUNTIME_DIR") {
        return d;
    }
    #[cfg(unix)]
    {
        if let Some(d) = env_dir("XDG_RUNTIME_DIR") {
            return d.join("timetable");
        }
        let base = env_dir("TMPDIR").unwrap_or_else(|| PathBuf::from("/tmp"));
        base.join(format!("timetable-{}", unsafe { libc::getuid() }))
    }
    #[cfg(not(unix))]
    {
        home().join(".timetable").join("run")
    }
}

pub fn socket_path() -> PathBuf {
    runtime_dir().join("timetable.sock")
}

/// Held (flock) by the running daemon, so two daemons started at once by
/// the two bars (one per monitor) cannot both bind the socket.
pub fn lock_path() -> PathBuf {
    runtime_dir().join("daemon.lock")
}

pub fn config_file() -> PathBuf {
    config_dir().join("config.toml")
}

/// The user's stylesheet for the drawer, loaded over the theme.
pub fn style_file() -> PathBuf {
    config_dir().join("style.css")
}

/// The running focus block survives a daemon restart here.
pub fn focus_file() -> PathBuf {
    state_dir().join("focus.json")
}

/// Reminders already sent, so a restart does not send them twice.
pub fn reminded_file() -> PathBuf {
    state_dir().join("reminded.json")
}

/// The drawer's day view (`strip` or `timeline`), as last toggled.
pub fn view_file() -> PathBuf {
    state_dir().join("view")
}

/// The expanded agenda from the last good fetch (never the address).
pub fn agenda_cache() -> PathBuf {
    cache_dir().join("agenda.json")
}

pub fn board_log() -> PathBuf {
    state_dir().join("board.log")
}

pub fn daemon_log() -> PathBuf {
    state_dir().join("daemon.log")
}

/// Waybar's config directory.
pub fn waybar_dir() -> PathBuf {
    xdg_config().join("waybar")
}

/// Shows a path under the home directory as `~/...`.
pub fn tilde(p: &std::path::Path) -> String {
    let home = home();
    match p.strip_prefix(&home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".into(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => p.display().to_string(),
    }
}

/// Writes a file only the user can read, through a temporary file so a
/// reader never sees half of it.
pub fn write_private(path: &std::path::Path, body: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    let mut opts = std::fs::OpenOptions::new();
    opts.create(true).write(true).truncate(true);
    #[cfg(unix)]
    opts.mode(0o600);
    let mut f = opts.open(&tmp)?;
    f.write_all(body)?;
    f.sync_all().ok();
    std::fs::rename(&tmp, path)
}
