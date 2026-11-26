//! Local time, sunrise and sunset, and the on/off schedule.

use crate::config::{ScheduleConfig, ScheduleMode};

#[derive(Debug, Clone, Copy)]
pub struct LocalNow {
    /// Minutes since local midnight.
    pub minutes: u32,
    /// Day of the year, 0-based.
    pub yday: u32,
    /// Offset from UTC in minutes.
    pub gmtoff_min: i32,
}

pub fn local_now() -> LocalNow {
    // SAFETY: `time` and `localtime_r` only write to the provided out-pointers.
    unsafe {
        let t = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        LocalNow {
            minutes: (tm.tm_hour * 60 + tm.tm_min) as u32,
            yday: tm.tm_yday as u32,
            gmtoff_min: (tm.tm_gmtoff / 60) as i32,
        }
    }
}

/// Sunrise and sunset in local minutes after midnight (NOAA approximation).
/// Returns `None` during polar day/night.
pub fn sun_times(lat: f64, lon: f64, yday: u32, gmtoff_min: i32) -> Option<(f64, f64)> {
    use std::f64::consts::PI;
    let gamma = 2.0 * PI / 365.0 * yday as f64;
    let eqtime = 229.18
        * (0.000075 + 0.001868 * gamma.cos()
            - 0.032077 * gamma.sin()
            - 0.014615 * (2.0 * gamma).cos()
            - 0.040849 * (2.0 * gamma).sin());
    let decl = 0.006918 - 0.399912 * gamma.cos() + 0.070257 * gamma.sin()
        - 0.006758 * (2.0 * gamma).cos()
        + 0.000907 * (2.0 * gamma).sin()
        - 0.002697 * (3.0 * gamma).cos()
        + 0.00148 * (3.0 * gamma).sin();
    let lat_r = lat.to_radians();
    let cos_ha =
        (90.833f64.to_radians().cos() / (lat_r.cos() * decl.cos())) - lat_r.tan() * decl.tan();
    if !(-1.0..=1.0).contains(&cos_ha) {
        return None;
    }
    let ha = cos_ha.acos().to_degrees();
    let wrap = |m: f64| m.rem_euclid(1440.0);
    let sunrise = wrap(720.0 - 4.0 * (lon + ha) - eqtime + gmtoff_min as f64);
    let sunset = wrap(720.0 - 4.0 * (lon - ha) - eqtime + gmtoff_min as f64);
    Some((sunrise, sunset))
}

/// Is `now` inside `[start, end)`, wrapping around midnight?
pub fn in_window(now: u32, start: u32, end: u32) -> bool {
    if start == end {
        return true;
    }
    if start < end { now >= start && now < end } else { now >= start || now < end }
}

/// Whether the sun is up. Without a location, 07:00–19:00 counts as day.
pub fn is_daytime(cfg: &ScheduleConfig, now: LocalNow) -> bool {
    match (cfg.latitude, cfg.longitude) {
        (Some(lat), Some(lon)) => match sun_times(lat, lon, now.yday, now.gmtoff_min) {
            Some((rise, set)) => in_window(now.minutes, rise as u32, set as u32),
            // Polar: summer if the sun's declination matches the hemisphere.
            None => {
                let summer = (80..266).contains(&now.yday);
                summer == (lat >= 0.0)
            }
        },
        _ => in_window(now.minutes, 7 * 60, 19 * 60),
    }
}

/// Whether the schedule currently allows the effect.
pub fn schedule_active(cfg: &ScheduleConfig, now: LocalNow) -> bool {
    match cfg.mode {
        ScheduleMode::Always => true,
        ScheduleMode::Day => is_daytime(cfg, now),
        ScheduleMode::Night => !is_daytime(cfg, now),
        ScheduleMode::Custom => in_window(now.minutes, cfg.start as u32, cfg.end as u32),
    }
}

pub fn fmt_hm(minutes: f64) -> String {
    let m = minutes.rem_euclid(1440.0).round() as u32 % 1440;
    format!("{:02}:{:02}", m / 60, m % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_wrap() {
        assert!(in_window(23 * 60, 21 * 60, 7 * 60));
        assert!(in_window(60, 21 * 60, 7 * 60));
        assert!(!in_window(12 * 60, 21 * 60, 7 * 60));
        assert!(in_window(12 * 60, 9 * 60, 17 * 60));
    }

    #[test]
    fn helsinki_midsummer() {
        // Helsinki, ~June 21, UTC+3: sunrise ≈ 03:54, sunset ≈ 22:50.
        let (rise, set) = sun_times(60.17, 24.94, 171, 180).unwrap();
        assert!((rise - (3.0 * 60.0 + 54.0)).abs() < 15.0, "rise {}", fmt_hm(rise));
        assert!((set - (22.0 * 60.0 + 50.0)).abs() < 15.0, "set {}", fmt_hm(set));
    }
}
