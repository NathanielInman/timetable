//! `timetable`: the Timetable daemon, CLI and waybar module.

mod act;
mod bar;
mod cli;
mod daemon;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // `timetable status | head` should end quietly. The daemon keeps Rust's
    // ignored SIGPIPE: a drawer or bar that goes away must not kill it.
    if !matches!(args.first().map(String::as_str), Some("daemon")) {
        unsafe {
            libc::signal(libc::SIGPIPE, libc::SIG_DFL);
        }
    }
    let code = match cli::run(&args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("timetable: {e:#}");
            1
        }
    };
    std::process::exit(code);
}
