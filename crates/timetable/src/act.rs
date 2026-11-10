//! What Timetable does to the desktop: switch to the meeting workspace and
//! open a link (Join), and send notifications (reminders, the end of a
//! focus block). These run on helper threads, never in the bar's path.

use anyhow::{anyhow, Result};
use std::process::{Command, Stdio};
use timetable_core::config::Config;
use timetable_core::links;
use timetable_core::model::Event;

/// The Hyprland dispatch for a workspace: numbers stay bare, names are
/// quoted for Lua.
pub fn dispatch_for(template: &str, workspace: &str) -> String {
    let ws = workspace.trim();
    let value = if ws.parse::<i64>().is_ok() {
        ws.to_string()
    } else {
        format!("\"{}\"", ws.replace('\\', "\\\\").replace('"', "\\\""))
    };
    template.replace("{workspace}", &value)
}

/// Runs a command detached, in its own session, and reaps it.
fn spawn_detached(mut cmd: Command) -> Result<()> {
    use std::os::unix::process::CommandExt;
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let mut child = cmd.spawn()?;
    std::thread::spawn(move || child.wait());
    Ok(())
}

/// Switches to `join.workspace` (when Hyprland is there) and opens the
/// event's link with `join.opener`. Returns what happened, for the CLI.
pub fn join(cfg: &Config, e: &Event) -> Result<String> {
    let link = e
        .link
        .as_deref()
        .ok_or_else(|| anyhow!("{} has no meeting link", e.title_or_default()))?;
    let mut switched = String::new();
    let ws = cfg.join.workspace.trim();
    if !ws.is_empty() && bar_anchor::hypr::socket().is_some() {
        let d = dispatch_for(&cfg.join.dispatch, ws);
        match bar_anchor::hypr::query(&format!("dispatch {d}")) {
            Ok(r) if r.trim() == "ok" => switched = format!(" on workspace {ws}"),
            Ok(r) => eprintln!("timetable: workspace switch: {}", r.trim()),
            Err(e) => eprintln!("timetable: workspace switch: {e:#}"),
        }
    }
    let opener = timetable_core::paths::expand_command(&cfg.join.opener);
    let mut cmd = Command::new("sh");
    cmd.arg("-c")
        .arg(format!("{opener} \"$1\""))
        .arg("timetable")
        .arg(link);
    spawn_detached(cmd)?;
    Ok(format!(
        "Joining {} ({}){switched}",
        e.title_or_default(),
        links::label(e.kind.as_deref().unwrap_or("link"))
    ))
}

/// Called when a notification's button is pressed.
pub type OnAction = Box<dyn FnOnce() + Send>;

/// A desktop notification through `notify-send`. With `action`, waits on
/// a helper thread for the button and calls `on_action` when it is
/// pressed.
pub fn notify(summary: &str, body: &str, action: Option<(&str, OnAction)>) {
    let mut cmd = Command::new("notify-send");
    cmd.arg("--app-name=Timetable")
        .arg("--icon=x-office-calendar")
        .arg("--urgency=normal");
    let mut on = None;
    if let Some((label, f)) = action {
        cmd.arg(format!("--action=go={label}"));
        on = Some(f);
    }
    cmd.arg(summary).arg(body);
    cmd.stdin(Stdio::null()).stderr(Stdio::null());
    std::thread::spawn(move || match on {
        Some(f) => {
            if let Ok(out) = cmd.stdout(Stdio::piped()).output() {
                if String::from_utf8_lossy(&out.stdout).trim() == "go" {
                    f();
                }
            }
        }
        None => {
            let _ = cmd.stdout(Stdio::null()).status();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatches() {
        let t = Config::default().join.dispatch;
        assert_eq!(dispatch_for(&t, "2"), "hl.dsp.focus({ workspace = 2 })");
        assert_eq!(
            dispatch_for(&t, "name:work"),
            "hl.dsp.focus({ workspace = \"name:work\" })"
        );
    }
}
