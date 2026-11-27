//! The three UI surfaces: control center, tray menu and settings window.

pub mod control_center;
pub mod menu;
pub mod settings;

use std::fmt;
use std::time::Duration;

use crate::cmd::Cmd;
use crate::config::{Config, HotkeyAction};
use crate::power::PowerStatus;
use crate::ui::{FontKind, Ui, style};
use crate::x11::Monitor;

/// Why the overlay is hidden even though it's switched on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pause {
    NoCompositor,
    Snoozed,
    Schedule,
    OnBattery,
    BatteryLow,
    /// The focused app (by WM_CLASS) is on the exclude list.
    Excluded(String),
    /// "Only listed" mode and the focused app isn't listed.
    NotListed,
    /// Every covered monitor is showing a fullscreen window.
    Fullscreen,
    /// `paperlike hide -- <command>` is running.
    Hidden,
}

impl fmt::Display for Pause {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Pause::NoCompositor => f.write_str("no compositor"),
            Pause::Snoozed => f.write_str("snoozed"),
            Pause::Schedule => f.write_str("schedule"),
            Pause::OnBattery => f.write_str("on battery"),
            Pause::BatteryLow => f.write_str("battery low"),
            Pause::Excluded(app) => write!(f, "{app} excluded"),
            Pause::NotListed => f.write_str("app not listed"),
            Pause::Fullscreen => f.write_str("fullscreen app"),
            Pause::Hidden => f.write_str("hidden for a command"),
        }
    }
}

/// A snapshot of runtime state for the views to display.
pub struct Status {
    pub compositor: bool,
    pub tray: bool,
    pub snooze_left: Option<Duration>,
    pub paused: Option<Pause>,
    /// Whether the primary display is using its dark-screen texture.
    pub dark_now: bool,
    pub system_dark: bool,
    /// WM_CLASS of the last focused window that isn't ours.
    pub focused_app: Option<String>,
    pub power: PowerStatus,
    pub monitors: Vec<Monitor>,
    pub clients: Vec<String>,
    pub hotkeys_failed: Vec<HotkeyAction>,
    pub autostart: bool,
    /// Today's sunrise/sunset in local minutes, when a location is set.
    pub sun: Option<(f64, f64)>,
    pub config_path: String,
}

/// What a view works with during a frame.
pub struct Ctx<'a> {
    pub cfg: &'a mut Config,
    pub st: &'a Status,
    pub cmds: &'a mut Vec<Cmd>,
    /// Set this after modifying `cfg` so it gets applied and saved.
    pub changed: bool,
}

impl Ctx<'_> {
    pub fn send(&mut self, c: Cmd) {
        self.cmds.push(c);
    }
}

pub fn fmt_duration(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// Draws the "paperlike" wordmark centred on `cx`.
pub fn wordmark(ui: &mut Ui, cx: f32, top: f32, size: f32) {
    let w = ui.p.text_w("paperlike", FontKind::Brand, size);
    ui.p.text(cx - w * 0.5, top, "paperlike", FontKind::Brand, size, style::CREAM);
}

/// The app mark: a cream "p" in a deep green circle.
pub fn app_mark(ui: &mut Ui, cx: f32, cy: f32, r: f32, on: bool) {
    ui.p.circle(cx, cy, r, if on { style::DEEP_GREEN } else { crate::ui::Col(0, 0, 0, 70) });
    let size = r * 1.45;
    let w = ui.p.text_w("p", FontKind::Brand, size);
    let col = if on { style::CREAM } else { style::CREAM.fade(0.5) };
    ui.p.text_vc(cx - w * 0.5, cy - r * 0.12, "p", FontKind::Brand, size, col);
}
