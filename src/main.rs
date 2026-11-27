//! paperlike puts a paper-like texture over your X11 desktop.
//!
//! `paperlike` with no arguments starts the daemon, or opens the control
//! center if one is already running. Anything else is sent to the running
//! daemon as a command; see `paperlike help`.

mod app;
mod cmd;
mod config;
mod ipc;
mod lamp;
mod power;
mod preview;
mod schedule;
mod svgfx;
mod texture;
mod theme;
mod ui;
mod views;
mod watch;
mod x11;

use std::io::ErrorKind;
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, ExitCode};
use std::time::Duration;

/// How long the compositor gets to repaint the screen without the overlay
/// before `paperlike hide` runs its command. A frame or two at most.
const COMPOSITOR_SETTLE: Duration = Duration::from_millis(100);

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("-h" | "--help" | "help") => {
            print!("{}", cmd::HELP);
            return ExitCode::SUCCESS;
        }
        Some("-V" | "--version" | "version") => {
            println!("paperlike {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Some("daemon") => return daemon(),
        Some("hide") => return hide_while(&args[1..]),
        Some("preview") => return preview::run(&args[1..]),
        _ => {}
    }

    let line = args.join(" ");
    let request = if line.is_empty() { "panel" } else { line.as_str() };
    if !line.is_empty()
        && let Err(e) = cmd::parse(&line)
    {
        eprintln!("paperlike: {e}");
        return ExitCode::from(2);
    }
    match ipc::send(request) {
        Ok(reply) => {
            let reply = reply.trim_end();
            if let Some(err) = reply.strip_prefix("error: ") {
                eprintln!("paperlike: {err}");
                return ExitCode::FAILURE;
            }
            if !reply.is_empty() && !line.is_empty() {
                println!("{reply}");
            }
            ExitCode::SUCCESS
        }
        Err(_) if line.is_empty() => daemon(),
        Err(_) if args[0] == "status" => {
            println!(
                "{}",
                if args.iter().any(|a| a.contains("json")) {
                    "{\"running\":false}"
                } else {
                    "not running"
                }
            );
            ExitCode::from(3)
        }
        Err(_) => {
            eprintln!("paperlike: the daemon is not running (start it with `paperlike`)");
            ExitCode::FAILURE
        }
    }
}

/// `paperlike hide -- <command>`: runs a command (say, a screenshot tool)
/// with the texture hidden, and brings the texture back when it exits.
fn hide_while(args: &[String]) -> ExitCode {
    let args = args.strip_prefix(&["--".to_string()]).unwrap_or(args);
    let Some((program, rest)) = args.split_first() else {
        eprintln!("usage: paperlike hide -- <command> [args...]");
        return ExitCode::from(2);
    };
    // The overlay stays hidden while this connection is open. If we die,
    // the daemon sees it close and shows the overlay again.
    let hold = match ipc::hold_hidden() {
        Ok(hold) => {
            std::thread::sleep(COMPOSITOR_SETTLE);
            Some(hold)
        }
        // No daemon, so nothing to hide.
        Err(e) if matches!(e.kind(), ErrorKind::NotFound | ErrorKind::ConnectionRefused) => None,
        Err(e) => {
            eprintln!("paperlike: could not hide the texture: {e}");
            None
        }
    };
    let mut child = match Command::new(program).args(rest).spawn() {
        Ok(child) => child,
        Err(e) => {
            eprintln!("paperlike: cannot run {program}: {e}");
            return ExitCode::from(127);
        }
    };
    // Like a shell running a command: Ctrl-C is for the command, and we
    // wait for it to finish before bringing the texture back.
    // SAFETY: setting signal dispositions to SIG_IGN has no preconditions.
    unsafe {
        libc::signal(libc::SIGINT, libc::SIG_IGN);
        libc::signal(libc::SIGQUIT, libc::SIG_IGN);
    }
    let status = child.wait();
    drop(hold);
    match status {
        Ok(s) => match (s.code(), s.signal()) {
            (Some(code), _) => ExitCode::from(code as u8),
            (None, Some(sig)) => ExitCode::from(128 + sig as u8),
            _ => ExitCode::FAILURE,
        },
        Err(e) => {
            eprintln!("paperlike: {e}");
            ExitCode::FAILURE
        }
    }
}

fn daemon() -> ExitCode {
    let server = match ipc::Server::bind() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("paperlike: {e}");
            return ExitCode::FAILURE;
        }
    };
    let result = app::App::new(server).and_then(|mut a| a.run());
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("paperlike: {e}");
            ExitCode::FAILURE
        }
    }
}
