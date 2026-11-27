//! Commands, and the parser for the command line / socket protocol.
//!
//! Everything that changes state goes through [`Cmd`], whether it came from
//! the CLI, a hotkey, the tray icon or a click in the UI.

use crate::config::ThemeMode;
use crate::lamp::LampPreset;
use crate::texture::TextureId;
use crate::views::settings::Page;

#[derive(Debug, Clone, PartialEq)]
pub enum Cmd {
    ToggleEffect,
    SetEffect(bool),
    /// Next (+1) or previous (−1) texture for the current theme.
    CycleTexture(i32),
    /// Selects a texture; `dark: None` means "for the theme currently shown".
    SetTexture {
        tex: TextureId,
        dark: Option<bool>,
    },
    IntensityDelta(i32),
    IntensitySet(u8),
    ToggleLamp,
    SetLamp(bool),
    LampPreset(LampPreset),
    Snooze(u32),
    ToggleSnooze,
    Unsnooze,
    Theme(ThemeMode),
    CycleTheme,
    TogglePanel,
    ClosePanel,
    OpenMenu {
        x: i16,
        y: i16,
    },
    CloseMenu,
    OpenSettings(Option<Page>),
    CloseSettings,
    RefreshDisplays,
    RefreshClients,
    ToggleExclusion(String),
    SetAutostart(bool),
    RegrabHotkeys,
    ResetDefaults,
    ReloadConfig,
    Quit,
}

/// What a CLI line asks for.
pub enum Request {
    Run(Cmd),
    Status {
        json: bool,
    },
    /// Hide the overlay until the client disconnects (`paperlike hide`).
    Hide,
    Help,
}

pub const HELP: &str = "\
usage: paperlike [command]

  (no command)          start the daemon, or open the control center if running
  daemon                start the daemon (fails if one is already running)
  toggle | on | off     switch the texture
  panel                 toggle the control center
  settings [page]       open settings (general, texture, lamp, schedule,
                        exceptions, display, hotkeys, about)
  next | prev           switch to the next/previous texture
  texture <name> [light|dark]
  intensity <N|+N|-N>   set or nudge the intensity, in percent
  lamp [on|off|toggle|<preset>]
  snooze <minutes>|off
  theme <light|dark|system|screen|sun>
  hide -- <command>     hide the texture while a command runs, e.g. a
                        screenshot tool
  status [--json]       print the current state (handy for status bars)
  reload                re-read the config file (edits also apply on save)
  quit                  stop the daemon
";

pub fn parse(line: &str) -> Result<Request, String> {
    let words: Vec<&str> = line.split_whitespace().collect();
    let arg = |i: usize| words.get(i).copied().unwrap_or("");
    let run = |c: Cmd| Ok(Request::Run(c));
    match arg(0) {
        "" | "panel" | "control-center" => run(Cmd::TogglePanel),
        "toggle" => run(Cmd::ToggleEffect),
        "on" => run(Cmd::SetEffect(true)),
        "off" => run(Cmd::SetEffect(false)),
        "next" | "cycle" => run(Cmd::CycleTexture(1)),
        "prev" | "previous" => run(Cmd::CycleTexture(-1)),
        "settings" | "prefs" => {
            let page = if arg(1).is_empty() {
                None
            } else {
                Some(
                    Page::parse(arg(1))
                        .ok_or_else(|| format!("unknown settings page '{}'", arg(1)))?,
                )
            };
            run(Cmd::OpenSettings(page))
        }
        "texture" => {
            let tex = TextureId::parse(arg(1)).ok_or_else(|| {
                let names: Vec<_> = TextureId::ALL.iter().map(|t| t.slug()).collect();
                format!("unknown texture '{}'. Try: {}", arg(1), names.join(", "))
            })?;
            let dark = match arg(2) {
                "" => None,
                "light" => Some(false),
                "dark" => Some(true),
                other => return Err(format!("expected light or dark, got '{other}'")),
            };
            run(Cmd::SetTexture { tex, dark })
        }
        "intensity" => {
            let a = arg(1);
            let n: i32 = a
                .trim_start_matches('+')
                .trim_end_matches('%')
                .parse()
                .map_err(|_| "intensity needs a number")?;
            if a.starts_with('+') || a.starts_with('-') {
                run(Cmd::IntensityDelta(n))
            } else {
                run(Cmd::IntensitySet(n.clamp(0, 100) as u8))
            }
        }
        "lamp" => match arg(1) {
            "" | "toggle" => run(Cmd::ToggleLamp),
            "on" => run(Cmd::SetLamp(true)),
            "off" => run(Cmd::SetLamp(false)),
            p => run(Cmd::LampPreset(LampPreset::parse(p).ok_or_else(|| {
                let names: Vec<_> = LampPreset::ALL.iter().map(|p| p.slug()).collect();
                format!("unknown lamp preset '{p}'. Try: {}", names.join(", "))
            })?)),
        },
        "snooze" => match arg(1) {
            "off" | "resume" | "cancel" => run(Cmd::Unsnooze),
            "" => run(Cmd::Snooze(5)),
            m => run(Cmd::Snooze(
                m.parse::<u32>().map_err(|_| "snooze needs minutes")?.clamp(1, 24 * 60),
            )),
        },
        "theme" => run(Cmd::Theme(
            ThemeMode::parse(arg(1)).ok_or("theme must be light, dark, system, screen or sun")?,
        )),
        "status" => Ok(Request::Status { json: arg(1) == "--json" || arg(1) == "json" }),
        "hide" => Ok(Request::Hide),
        "reload" => run(Cmd::ReloadConfig),
        "quit" | "exit" => run(Cmd::Quit),
        "help" | "--help" | "-h" => Ok(Request::Help),
        other => Err(format!("unknown command '{other}'\n\n{HELP}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(s: &str) -> Cmd {
        match parse(s) {
            Ok(Request::Run(c)) => c,
            _ => panic!("{s} did not parse to a command"),
        }
    }

    #[test]
    fn parses_commands() {
        assert_eq!(cmd("intensity +3"), Cmd::IntensityDelta(3));
        assert_eq!(cmd("intensity -2"), Cmd::IntensityDelta(-2));
        assert_eq!(cmd("intensity 25"), Cmd::IntensitySet(25));
        assert_eq!(
            cmd("texture linen dark"),
            Cmd::SetTexture { tex: TextureId::Linen, dark: Some(true) }
        );
        assert_eq!(cmd("lamp sunset"), Cmd::LampPreset(LampPreset::Sunset));
        assert_eq!(cmd("settings hotkeys"), Cmd::OpenSettings(Some(Page::Hotkeys)));
        assert_eq!(cmd("snooze 10"), Cmd::Snooze(10));
        assert!(parse("bogus").is_err());
        assert!(parse("texture velvet").is_err());
    }
}
