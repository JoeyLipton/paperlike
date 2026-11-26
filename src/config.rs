//! Settings, stored as TOML in `$XDG_CONFIG_HOME/paperlike/paperlike.toml`.
//!
//! Every field has a default, so partial or hand-edited files are fine.
//! Unknown texture or preset names fall back to the default instead of
//! rejecting the whole file.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Deserializer, Serialize};

use crate::lamp::{LampParams, LampPreset};
use crate::texture::TextureId;

pub const MAX_PINNED: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeMode {
    /// Always use the light-screen texture.
    Light,
    /// Always use the dark-screen texture.
    Dark,
    /// Follow the GTK/GNOME colour scheme.
    System,
    /// Look at what each monitor is showing.
    Screen,
    /// Light texture by day, dark after sunset.
    Sun,
}

impl ThemeMode {
    pub const ALL: [ThemeMode; 5] =
        [ThemeMode::Light, ThemeMode::Dark, ThemeMode::System, ThemeMode::Screen, ThemeMode::Sun];

    pub fn label(self) -> &'static str {
        match self {
            ThemeMode::Light => "Light",
            ThemeMode::Dark => "Dark",
            ThemeMode::System => "System",
            ThemeMode::Screen => "Screen",
            ThemeMode::Sun => "Sun",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        ThemeMode::ALL.into_iter().find(|m| m.label().eq_ignore_ascii_case(s))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScheduleMode {
    Always,
    /// Sunrise to sunset.
    Day,
    /// Sunset to sunrise.
    Night,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BatteryMode {
    Never,
    OnBattery,
    BelowPercent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExceptionMode {
    /// Run everywhere except in the listed apps.
    Exclude,
    /// Run only while a listed app is focused.
    Only,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct LampConfig {
    pub enabled: bool,
    #[serde(deserialize_with = "lamp_preset_or_default")]
    pub preset: LampPreset,
    #[serde(flatten)]
    pub params: LampParams,
    /// Only light the lamp while the dark-screen texture is showing.
    pub dark_only: bool,
}

impl Default for LampConfig {
    fn default() -> Self {
        let preset = LampPreset::Ambient;
        LampConfig { enabled: false, preset, params: preset.params(), dark_only: false }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ScheduleConfig {
    pub mode: ScheduleMode,
    /// Start of the custom window, in minutes after midnight.
    pub start: u16,
    /// End of the custom window, in minutes after midnight.
    pub end: u16,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

impl Default for ScheduleConfig {
    fn default() -> Self {
        ScheduleConfig {
            mode: ScheduleMode::Always,
            start: 18 * 60,
            end: 7 * 60,
            latitude: None,
            longitude: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct BatteryConfig {
    pub mode: BatteryMode,
    pub threshold: u8,
}

impl Default for BatteryConfig {
    fn default() -> Self {
        BatteryConfig { mode: BatteryMode::Never, threshold: 20 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExceptionsConfig {
    pub mode: ExceptionMode,
    /// WM_CLASS names (class or instance, any case) to step aside for.
    pub exclude: Vec<String>,
    /// WM_CLASS names to run in when `mode = "only"`.
    pub only: Vec<String>,
}

impl Default for ExceptionsConfig {
    fn default() -> Self {
        ExceptionsConfig { mode: ExceptionMode::Exclude, exclude: Vec::new(), only: Vec::new() }
    }
}

impl ExceptionsConfig {
    /// The list that applies to the current mode.
    pub fn list(&self) -> &Vec<String> {
        match self.mode {
            ExceptionMode::Exclude => &self.exclude,
            ExceptionMode::Only => &self.only,
        }
    }

    pub fn list_mut(&mut self) -> &mut Vec<String> {
        match self.mode {
            ExceptionMode::Exclude => &mut self.exclude,
            ExceptionMode::Only => &mut self.only,
        }
    }

    pub fn contains(&self, class: &str) -> bool {
        self.list().iter().any(|c| c.eq_ignore_ascii_case(class))
    }

    /// Adds or removes `class` from the current list. Returns whether it is
    /// now listed.
    pub fn toggle(&mut self, class: &str) -> bool {
        let list = self.list_mut();
        if let Some(i) = list.iter().position(|c| c.eq_ignore_ascii_case(class)) {
            list.remove(i);
            false
        } else {
            list.push(class.to_string());
            true
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HotkeyAction {
    ToggleEffect,
    CycleTexture,
    ToggleLamp,
    Snooze,
    IntensityUp,
    IntensityDown,
    ControlCenter,
}

impl HotkeyAction {
    pub const ALL: [HotkeyAction; 7] = [
        HotkeyAction::ToggleEffect,
        HotkeyAction::CycleTexture,
        HotkeyAction::ToggleLamp,
        HotkeyAction::Snooze,
        HotkeyAction::IntensityUp,
        HotkeyAction::IntensityDown,
        HotkeyAction::ControlCenter,
    ];

    pub fn label(self) -> &'static str {
        match self {
            HotkeyAction::ToggleEffect => "Toggle effect",
            HotkeyAction::CycleTexture => "Next texture",
            HotkeyAction::ToggleLamp => "Toggle Desk Lamp",
            HotkeyAction::Snooze => "Snooze 5 minutes",
            HotkeyAction::IntensityUp => "Intensity up",
            HotkeyAction::IntensityDown => "Intensity down",
            HotkeyAction::ControlCenter => "Open control center",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct HotkeyConfig {
    pub enabled: bool,
    pub toggle_effect: String,
    pub cycle_texture: String,
    pub toggle_lamp: String,
    pub snooze: String,
    pub intensity_up: String,
    pub intensity_down: String,
    pub control_center: String,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        // Super+Alt keeps clear of the usual $mod+key bindings in i3.
        HotkeyConfig {
            enabled: true,
            toggle_effect: "Super+Alt+p".into(),
            cycle_texture: "Super+Alt+t".into(),
            toggle_lamp: "Super+Alt+o".into(),
            snooze: "Super+Alt+backslash".into(),
            intensity_up: "Super+Alt+bracketright".into(),
            intensity_down: "Super+Alt+bracketleft".into(),
            control_center: String::new(),
        }
    }
}

impl HotkeyConfig {
    pub fn get(&self, a: HotkeyAction) -> &str {
        match a {
            HotkeyAction::ToggleEffect => &self.toggle_effect,
            HotkeyAction::CycleTexture => &self.cycle_texture,
            HotkeyAction::ToggleLamp => &self.toggle_lamp,
            HotkeyAction::Snooze => &self.snooze,
            HotkeyAction::IntensityUp => &self.intensity_up,
            HotkeyAction::IntensityDown => &self.intensity_down,
            HotkeyAction::ControlCenter => &self.control_center,
        }
    }

    pub fn set(&mut self, a: HotkeyAction, v: String) {
        match a {
            HotkeyAction::ToggleEffect => self.toggle_effect = v,
            HotkeyAction::CycleTexture => self.cycle_texture = v,
            HotkeyAction::ToggleLamp => self.toggle_lamp = v,
            HotkeyAction::Snooze => self.snooze = v,
            HotkeyAction::IntensityUp => self.intensity_up = v,
            HotkeyAction::IntensityDown => self.intensity_down = v,
            HotkeyAction::ControlCenter => self.control_center = v,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    /// Master switch for the texture (the lamp has its own).
    pub enabled: bool,
    pub theme_mode: ThemeMode,
    #[serde(deserialize_with = "texture_or_default")]
    pub light_texture: TextureId,
    #[serde(deserialize_with = "texture_or_default")]
    pub dark_texture: TextureId,
    /// Intensity per texture and theme, keyed like `"light.matte"`.
    pub intensity: BTreeMap<String, u8>,
    /// Textures shown in the control center.
    #[serde(deserialize_with = "known_textures")]
    pub pinned: Vec<TextureId>,
    /// Device pixels per texture pixel; raise it on HiDPI screens.
    pub grain_scale: f32,
    pub lamp: LampConfig,
    pub schedule: ScheduleConfig,
    pub battery: BatteryConfig,
    pub exceptions: ExceptionsConfig,
    /// RandR output names to leave uncovered.
    pub disabled_outputs: Vec<String>,
    pub hotkeys: HotkeyConfig,
    /// UI scale; 0 means "work it out from Xft.dpi".
    pub ui_scale: f32,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            enabled: true,
            theme_mode: ThemeMode::System,
            light_texture: TextureId::Crosshatch,
            dark_texture: TextureId::Matte,
            intensity: BTreeMap::new(),
            pinned: vec![TextureId::Matte, TextureId::Crosshatch, TextureId::ColdPress],
            grain_scale: 1.0,
            lamp: LampConfig::default(),
            schedule: ScheduleConfig::default(),
            battery: BatteryConfig::default(),
            exceptions: ExceptionsConfig::default(),
            disabled_outputs: Vec::new(),
            hotkeys: HotkeyConfig::default(),
            ui_scale: 0.0,
        }
    }
}

fn texture_or_default<'de, D: Deserializer<'de>>(d: D) -> Result<TextureId, D::Error> {
    let s = String::deserialize(d)?;
    Ok(TextureId::ALL.into_iter().find(|t| t.slug() == s).unwrap_or(TextureId::Matte))
}

fn known_textures<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<TextureId>, D::Error> {
    let names = Vec::<String>::deserialize(d)?;
    Ok(names.iter().filter_map(|s| TextureId::ALL.into_iter().find(|t| t.slug() == s)).collect())
}

fn lamp_preset_or_default<'de, D: Deserializer<'de>>(d: D) -> Result<LampPreset, D::Error> {
    let s = String::deserialize(d)?;
    Ok(LampPreset::parse(&s).unwrap_or(LampPreset::Ambient))
}

fn intensity_key(dark: bool, tex: TextureId) -> String {
    format!("{}.{}", if dark { "dark" } else { "light" }, tex.slug())
}

impl Config {
    pub fn texture(&self, dark: bool) -> TextureId {
        if dark { self.dark_texture } else { self.light_texture }
    }

    pub fn set_texture(&mut self, dark: bool, tex: TextureId) {
        if dark {
            self.dark_texture = tex;
        } else {
            self.light_texture = tex;
        }
    }

    /// Intensity in percent, kept inside the texture's range.
    pub fn intensity(&self, dark: bool, tex: TextureId) -> u8 {
        let (lo, hi) = tex.range();
        self.intensity
            .get(&intensity_key(dark, tex))
            .copied()
            .unwrap_or_else(|| tex.default_intensity(dark))
            .clamp(lo, hi)
    }

    pub fn set_intensity(&mut self, dark: bool, tex: TextureId, v: i32) {
        let (lo, hi) = tex.range();
        let v = v.clamp(lo as i32, hi as i32) as u8;
        self.intensity.insert(intensity_key(dark, tex), v);
    }

    /// Pins or unpins a texture. Pinning a fifth one drops the oldest.
    pub fn toggle_pin(&mut self, tex: TextureId) {
        if let Some(i) = self.pinned.iter().position(|&t| t == tex) {
            self.pinned.remove(i);
        } else {
            if self.pinned.len() >= MAX_PINNED {
                self.pinned.remove(0);
            }
            self.pinned.push(tex);
        }
    }

    pub fn output_enabled(&self, name: &str) -> bool {
        !self.disabled_outputs.iter().any(|n| n == name)
    }

    /// Pulls hand-edited values back into range.
    fn sanitize(&mut self) {
        let finite_or = |v: f32, lo: f32, hi: f32, fallback: f32| {
            if v.is_finite() { v.clamp(lo, hi) } else { fallback }
        };
        self.grain_scale = finite_or(self.grain_scale, 0.5, 4.0, 1.0);
        self.ui_scale = finite_or(self.ui_scale, 0.0, 4.0, 0.0);
        let mut seen = Vec::new();
        self.pinned.retain(|t| {
            let new = !seen.contains(t);
            seen.push(*t);
            new
        });
        self.pinned.truncate(MAX_PINNED);
        self.schedule.start %= 1440;
        self.schedule.end %= 1440;
        self.battery.threshold = self.battery.threshold.clamp(5, 95);
        let p = &mut self.lamp.params;
        for v in [&mut p.warmth, &mut p.spread, &mut p.dimming] {
            *v = (*v).min(100);
        }
    }

    pub fn path() -> PathBuf {
        config_home().join("paperlike").join("paperlike.toml")
    }

    /// Where versions before 0.2 kept the file.
    fn old_path() -> PathBuf {
        config_home().join("paperlike").join("config.toml")
    }

    /// Parses a config file's text and pulls values back into range.
    pub fn parse(text: &str) -> Result<Config, toml::de::Error> {
        let mut c: Config = toml::from_str(text)?;
        c.sanitize();
        Ok(c)
    }

    /// Loads the config, or the defaults if there isn't one. A file that
    /// can't be parsed at all is renamed to `paperlike.toml.broken`.
    pub fn load() -> Config {
        let path = Self::path();
        let old = Self::old_path();
        if !path.exists() && old.exists() && std::fs::rename(&old, &path).is_ok() {
            eprintln!("paperlike: moved {} to {}", old.display(), path.display());
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Config::default();
        };
        match Self::parse(&text) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("paperlike: {} is invalid ({e}); using defaults", path.display());
                let _ = std::fs::rename(&path, path.with_extension("toml.broken"));
                Config::default()
            }
        }
    }

    /// Writes the config atomically and returns the text written.
    pub fn save(&self) -> std::io::Result<String> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, &text)?;
        std::fs::rename(tmp, path)?;
        Ok(text)
    }
}

pub fn config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
    })
}

/// The XDG autostart entry behind "Open at login".
pub mod autostart {
    use std::path::PathBuf;

    pub fn path() -> PathBuf {
        super::config_home().join("autostart").join("paperlike.desktop")
    }

    pub fn enabled() -> bool {
        path().exists()
    }

    pub fn set(on: bool) -> std::io::Result<()> {
        let p = path();
        if !on {
            return match std::fs::remove_file(&p) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                r => r,
            };
        }
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // Desktop entries want arguments with spaces quoted.
        let exe = std::env::current_exe()?.display().to_string();
        let exec = if exe.contains(' ') { format!("\"{exe}\"") } else { exe };
        std::fs::write(
            &p,
            format!(
                "[Desktop Entry]\n\
                 Type=Application\n\
                 Name=Paperlike\n\
                 Comment=Paper-like texture for your screen\n\
                 Exec={exec}\n\
                 Terminal=false\n\
                 X-GNOME-Autostart-enabled=true\n"
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut c = Config::default();
        c.set_intensity(true, TextureId::Frost, 33);
        c.exceptions.toggle("Gimp");
        c.schedule.latitude = Some(60.1);
        let text = toml::to_string_pretty(&c).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back, c);
        assert_eq!(back.intensity(true, TextureId::Frost), 33);
    }

    #[test]
    fn partial_file_uses_defaults() {
        let c: Config = toml::from_str("enabled = false\n[lamp]\nwarmth = 10\n").unwrap();
        assert!(!c.enabled);
        assert_eq!(c.lamp.params.warmth, 10);
        assert_eq!(c.pinned, Config::default().pinned);
    }

    #[test]
    fn unknown_names_fall_back() {
        let c: Config = toml::from_str(
            "light_texture = \"velvet\"\npinned = [\"felt\", \"velvet\"]\n[lamp]\npreset = \"disco\"\n",
        )
        .unwrap();
        assert_eq!(c.light_texture, TextureId::Matte);
        assert_eq!(c.pinned, vec![TextureId::Felt]);
        assert_eq!(c.lamp.preset, LampPreset::Ambient);
    }

    #[test]
    fn intensity_is_clamped_to_range() {
        let mut c = Config::default();
        c.set_intensity(false, TextureId::Felt, 90);
        assert_eq!(c.intensity(false, TextureId::Felt), 25);
    }

    #[test]
    fn pin_limit() {
        let mut c = Config::default();
        c.toggle_pin(TextureId::Graphite);
        c.toggle_pin(TextureId::Felt);
        assert_eq!(c.pinned.len(), MAX_PINNED);
        assert_eq!(c.pinned.first(), Some(&TextureId::Crosshatch));
        assert_eq!(c.pinned.last(), Some(&TextureId::Felt));
    }
}
