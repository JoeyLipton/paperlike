//! Works out whether the desktop prefers a dark theme.
//!
//! i3 has no theme setting of its own, so this checks where GTK and GNOME
//! keep theirs: gsettings, then `gtk-3.0`/`gtk-4.0` `settings.ini`. It only
//! re-checks when one of those files has changed, so polling is cheap.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::SystemTime;

use crate::config::config_home;

pub struct SystemTheme {
    stamps: Option<Vec<Option<SystemTime>>>,
    pub dark: bool,
}

const INI_FILES: [&str; 2] = ["gtk-3.0/settings.ini", "gtk-4.0/settings.ini"];

fn watched() -> Vec<PathBuf> {
    let c = config_home();
    std::iter::once(c.join("dconf/user")).chain(INI_FILES.iter().map(|f| c.join(f))).collect()
}

impl SystemTheme {
    pub fn new() -> Self {
        SystemTheme { stamps: None, dark: false }
    }

    /// Re-checks if any settings file changed. Returns true if `dark` flipped.
    pub fn poll(&mut self) -> bool {
        let stamps: Vec<_> = watched()
            .iter()
            .map(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
            .collect();
        if self.stamps.as_ref() == Some(&stamps) {
            return false;
        }
        self.stamps = Some(stamps);
        let dark = detect();
        let changed = dark != self.dark;
        self.dark = dark;
        changed
    }
}

fn gsettings(key: &str) -> Option<String> {
    let out = Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", key])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_ascii_lowercase())
}

fn detect() -> bool {
    if let Some(s) = gsettings("color-scheme") {
        if s.contains("prefer-dark") {
            return true;
        }
        if s.contains("prefer-light") {
            return false;
        }
    }
    if gsettings("gtk-theme").is_some_and(|s| s.contains("dark")) {
        return true;
    }
    INI_FILES.iter().any(|f| {
        std::fs::read_to_string(config_home().join(f)).is_ok_and(|text| ini_prefers_dark(&text))
    })
}

/// Whether a GTK `settings.ini` asks for a dark theme.
fn ini_prefers_dark(text: &str) -> bool {
    text.lines().filter_map(|line| line.split_once('=')).any(|(k, v)| {
        let v = v.trim().to_ascii_lowercase();
        match k.trim() {
            "gtk-application-prefer-dark-theme" => v == "1" || v == "true",
            "gtk-theme-name" => v.contains("dark"),
            _ => false,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::ini_prefers_dark;

    #[test]
    fn reads_settings_ini() {
        assert!(ini_prefers_dark("[Settings]\ngtk-application-prefer-dark-theme=1\n"));
        assert!(ini_prefers_dark("[Settings]\ngtk-theme-name = Adwaita-dark\n"));
        assert!(!ini_prefers_dark("[Settings]\ngtk-theme-name=Arc\ngtk-font-name=Sans 10\n"));
    }
}
