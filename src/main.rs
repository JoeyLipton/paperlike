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
mod x11;

use std::process::ExitCode;

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
