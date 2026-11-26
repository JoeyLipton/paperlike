//! Battery state from `/sys/class/power_supply`.

use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PowerStatus {
    pub has_battery: bool,
    pub on_battery: bool,
    pub percent: Option<u8>,
}

pub fn read() -> PowerStatus {
    read_from(Path::new("/sys/class/power_supply"))
}

fn read_from(dir: &Path) -> PowerStatus {
    let mut st = PowerStatus::default();
    let mut mains_online = false;
    let mut discharging = false;
    let Ok(entries) = fs::read_dir(dir) else {
        return st;
    };
    for e in entries.flatten() {
        let p = e.path();
        let get = |name: &str| fs::read_to_string(p.join(name)).map(|s| s.trim().to_string()).ok();
        match get("type").as_deref() {
            Some("Mains") | Some("USB") => {
                if get("online").as_deref() == Some("1") {
                    mains_online = true;
                }
            }
            Some("Battery") => {
                // Peripheral batteries (mice, headsets) report scope=Device.
                if get("scope").as_deref() == Some("Device") {
                    continue;
                }
                st.has_battery = true;
                if get("status").as_deref() == Some("Discharging") {
                    discharging = true;
                }
                if let Some(c) = get("capacity").and_then(|c| c.parse::<u8>().ok()) {
                    st.percent = Some(st.percent.map_or(c, |p: u8| p.min(c)));
                }
            }
            _ => {}
        }
    }
    st.on_battery = st.has_battery && (discharging || !mains_online);
    st
}

#[cfg(test)]
mod tests {
    use super::*;

    fn supply(dir: &Path, name: &str, files: &[(&str, &str)]) {
        let d = dir.join(name);
        fs::create_dir_all(&d).unwrap();
        for (f, v) in files {
            fs::write(d.join(f), format!("{v}\n")).unwrap();
        }
    }

    #[test]
    fn reads_laptop_battery() {
        let dir = std::env::temp_dir().join(format!("paperlike-power-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        supply(&dir, "AC", &[("type", "Mains"), ("online", "0")]);
        supply(&dir, "BAT0", &[("type", "Battery"), ("status", "Discharging"), ("capacity", "42")]);
        // A wireless mouse shouldn't count as the system battery.
        supply(
            &dir,
            "hidpp_battery_0",
            &[("type", "Battery"), ("scope", "Device"), ("capacity", "5")],
        );
        let st = read_from(&dir);
        fs::remove_dir_all(&dir).unwrap();
        assert_eq!(st, PowerStatus { has_battery: true, on_battery: true, percent: Some(42) });
    }

    #[test]
    fn desktop_without_battery() {
        let dir = std::env::temp_dir().join(format!("paperlike-power-ac-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        supply(&dir, "AC", &[("type", "Mains"), ("online", "1")]);
        let st = read_from(&dir);
        fs::remove_dir_all(&dir).unwrap();
        assert_eq!(st, PowerStatus::default());
    }
}
