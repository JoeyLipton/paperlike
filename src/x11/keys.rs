//! Keyboard mapping, global hotkeys and keysym names.

use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;

use super::{Res, Xc};
use crate::config::{HotkeyAction, HotkeyConfig};

pub const SHIFT: u16 = 1;
pub const LOCK: u16 = 2;
pub const CONTROL: u16 = 4;
pub const ALT: u16 = 8; // Mod1
pub const SUPER: u16 = 64; // Mod4
pub const RELEVANT_MODS: u16 = SHIFT | CONTROL | ALT | SUPER;

pub mod sym {
    pub const BACKSPACE: u32 = 0xff08;
    pub const RETURN: u32 = 0xff0d;
    pub const ESCAPE: u32 = 0xff1b;
    pub const HOME: u32 = 0xff50;
    pub const LEFT: u32 = 0xff51;
    pub const RIGHT: u32 = 0xff53;
    pub const END: u32 = 0xff57;
    pub const KP_ENTER: u32 = 0xff8d;
    pub const DELETE: u32 = 0xffff;
    pub const NUM_LOCK: u32 = 0xff7f;
}

/// Keycode → keysym table from the server.
pub struct KeyMap {
    min: u8,
    per: usize,
    syms: Vec<u32>,
    pub numlock: u16,
}

impl KeyMap {
    pub fn load(xc: &Xc) -> Res<KeyMap> {
        let setup = xc.conn.setup();
        let (min, max) = (setup.min_keycode, setup.max_keycode);
        let r = xc.conn.get_keyboard_mapping(min, max - min + 1)?.reply()?;
        let per = r.keysyms_per_keycode as usize;
        let mut km = KeyMap { min, per, syms: r.keysyms, numlock: 0x10 };
        if let Ok(Ok(m)) = xc.conn.get_modifier_mapping().map(|c| c.reply()) {
            let n = m.keycodes_per_modifier() as usize;
            for (i, &kc) in m.keycodes.iter().enumerate() {
                if kc != 0 && km.keysym(kc, false) == sym::NUM_LOCK {
                    km.numlock = 1 << (i / n.max(1));
                }
            }
        }
        Ok(km)
    }

    fn slot(&self, code: u8, idx: usize) -> u32 {
        if code < self.min || self.per == 0 {
            return 0;
        }
        let base = (code - self.min) as usize * self.per;
        self.syms.get(base + idx).copied().unwrap_or(0)
    }

    /// Keysym for a keycode, honouring Shift for the second column.
    pub fn keysym(&self, code: u8, shift: bool) -> u32 {
        let lower = self.slot(code, 0);
        if shift {
            let upper = self.slot(code, 1);
            if upper != 0 {
                return upper;
            }
            if (0x61..=0x7a).contains(&lower) {
                return lower - 0x20;
            }
        }
        lower
    }

    pub fn keycode(&self, keysym: u32) -> Option<u8> {
        (0..self.syms.len() / self.per.max(1)).find_map(|i| {
            let code = self.min as usize + i;
            (0..self.per.min(2))
                .any(|j| self.syms[i * self.per + j] == keysym)
                .then_some(code as u8)
        })
    }

    /// Resolves a key event into what a text field needs.
    pub fn text_key(&self, code: u8, state: u16) -> (u32, Option<char>) {
        let shift = state & SHIFT != 0;
        let lock = state & LOCK != 0;
        let mut s = self.keysym(code, shift);
        if lock && (0x61..=0x7a).contains(&s) {
            s -= 0x20;
        } else if lock && shift && (0x41..=0x5a).contains(&s) {
            s += 0x20;
        }
        let ch = if state & (CONTROL | SUPER) != 0 { None } else { keysym_char(s) };
        (s, ch)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hotkey {
    pub mods: u16,
    pub keysym: u32,
}

const NAMED: &[(&str, u32)] = &[
    ("space", 0x20),
    ("apostrophe", 0x27),
    ("comma", 0x2c),
    ("minus", 0x2d),
    ("period", 0x2e),
    ("slash", 0x2f),
    ("semicolon", 0x3b),
    ("equal", 0x3d),
    ("bracketleft", 0x5b),
    ("backslash", 0x5c),
    ("bracketright", 0x5d),
    ("grave", 0x60),
    ("BackSpace", 0xff08),
    ("Tab", 0xff09),
    ("Return", 0xff0d),
    ("Pause", 0xff13),
    ("Escape", 0xff1b),
    ("Home", 0xff50),
    ("Left", 0xff51),
    ("Up", 0xff52),
    ("Right", 0xff53),
    ("Down", 0xff54),
    ("Prior", 0xff55),
    ("Next", 0xff56),
    ("End", 0xff57),
    ("Print", 0xff61),
    ("Insert", 0xff63),
    ("Delete", 0xffff),
];

const PRETTY: &[(&str, &str)] = &[
    ("space", "Space"),
    ("apostrophe", "'"),
    ("comma", ","),
    ("minus", "-"),
    ("period", "."),
    ("slash", "/"),
    ("semicolon", ";"),
    ("equal", "="),
    ("bracketleft", "["),
    ("backslash", "\\"),
    ("bracketright", "]"),
    ("grave", "`"),
    ("Prior", "PgUp"),
    ("Next", "PgDn"),
];

pub fn keysym_name(s: u32) -> Option<String> {
    if let Some((n, _)) = NAMED.iter().find(|(_, v)| *v == s) {
        return Some((*n).to_string());
    }
    match s {
        0x30..=0x39 | 0x61..=0x7a => Some((s as u8 as char).to_string()),
        0x41..=0x5a => Some(((s as u8 + 0x20) as char).to_string()),
        0xffbe..=0xffd5 => Some(format!("F{}", s - 0xffbe + 1)),
        _ => None,
    }
}

pub fn keysym_from_name(n: &str) -> Option<u32> {
    if let Some((_, v)) = NAMED.iter().find(|(k, _)| k.eq_ignore_ascii_case(n)) {
        return Some(*v);
    }
    let b = n.as_bytes();
    if b.len() == 1 && (b[0].is_ascii_alphanumeric()) {
        return Some(b[0].to_ascii_lowercase() as u32);
    }
    if let Some(num) = n.strip_prefix(['F', 'f']).and_then(|d| d.parse::<u32>().ok())
        && (1..=24).contains(&num)
    {
        return Some(0xffbe + num - 1);
    }
    None
}

pub fn keysym_char(s: u32) -> Option<char> {
    match s {
        0x20..=0x7e | 0xa0..=0xff => char::from_u32(s),
        0x0100_0000..=0x0110_ffff => char::from_u32(s - 0x0100_0000),
        _ => None,
    }
}

pub fn is_modifier(s: u32) -> bool {
    (0xffe1..=0xffee).contains(&s) || s == 0xfe03 || s == sym::NUM_LOCK
}

pub fn parse_hotkey(s: &str) -> Option<Hotkey> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let mut mods = 0;
    let parts: Vec<&str> = s.split('+').map(str::trim).collect();
    let (key, modifiers) = parts.split_last()?;
    for m in modifiers {
        mods |= match m.to_ascii_lowercase().as_str() {
            "shift" => SHIFT,
            "ctrl" | "control" => CONTROL,
            "alt" | "mod1" => ALT,
            "super" | "mod4" | "win" | "cmd" => SUPER,
            _ => return None,
        };
    }
    Some(Hotkey { mods, keysym: keysym_from_name(key)? })
}

pub fn format_hotkey(h: Hotkey) -> Option<String> {
    let mut out = String::new();
    for (bit, name) in [(SUPER, "Super"), (CONTROL, "Ctrl"), (ALT, "Alt"), (SHIFT, "Shift")] {
        if h.mods & bit != 0 {
            out.push_str(name);
            out.push('+');
        }
    }
    out.push_str(&keysym_name(h.keysym)?);
    Some(out)
}

/// Human-friendly label, e.g. `Super + Alt + \`.
pub fn pretty(s: &str) -> String {
    if s.trim().is_empty() {
        return "Not set".into();
    }
    s.split('+')
        .map(|p| {
            let p = p.trim();
            if let Some((_, v)) = PRETTY.iter().find(|(k, _)| k.eq_ignore_ascii_case(p)) {
                (*v).to_string()
            } else if p.len() == 1 {
                p.to_ascii_uppercase()
            } else {
                p.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" + ")
}

/// Global key grabs on the root window.
#[derive(Default)]
pub struct Grabs {
    grabbed: Vec<(u8, u16, HotkeyAction)>,
    pub failed: Vec<HotkeyAction>,
}

impl Grabs {
    pub fn regrab(&mut self, xc: &Xc, km: &KeyMap, cfg: &HotkeyConfig) {
        for &(code, mods, _) in &self.grabbed {
            for extra in lock_variants(km) {
                let _ = xc.conn.ungrab_key(code, xc.root, ModMask::from(mods | extra));
            }
        }
        self.grabbed.clear();
        self.failed.clear();
        if !cfg.enabled {
            return;
        }
        for action in HotkeyAction::ALL {
            let Some(hk) = parse_hotkey(cfg.get(action)) else {
                continue;
            };
            let Some(code) = km.keycode(hk.keysym) else {
                self.failed.push(action);
                continue;
            };
            let mut ok = true;
            for extra in lock_variants(km) {
                let cookie = xc.conn.grab_key(
                    false,
                    xc.root,
                    ModMask::from(hk.mods | extra),
                    code,
                    GrabMode::ASYNC,
                    GrabMode::ASYNC,
                );
                if !matches!(cookie.map(|c| c.check()), Ok(Ok(()))) {
                    ok = false;
                }
            }
            if ok {
                self.grabbed.push((code, hk.mods, action));
            } else {
                for extra in lock_variants(km) {
                    let _ = xc.conn.ungrab_key(code, xc.root, ModMask::from(hk.mods | extra));
                }
                self.failed.push(action);
            }
        }
    }

    pub fn lookup(&self, code: u8, state: u16) -> Option<HotkeyAction> {
        let mods = state & RELEVANT_MODS;
        self.grabbed.iter().find(|&&(c, m, _)| c == code && m == mods).map(|&(_, _, a)| a)
    }
}

fn lock_variants(km: &KeyMap) -> [u16; 4] {
    [0, LOCK, km.numlock, LOCK | km.numlock]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotkey_round_trip() {
        let h = parse_hotkey("Super+Alt+backslash").unwrap();
        assert_eq!(h.mods, SUPER | ALT);
        assert_eq!(h.keysym, 0x5c);
        assert_eq!(format_hotkey(h).unwrap(), "Super+Alt+backslash");
        assert_eq!(pretty("Super+Alt+backslash"), "Super + Alt + \\");
        assert_eq!(parse_hotkey("Ctrl+F12").unwrap().keysym, 0xffc9);
        assert!(parse_hotkey("Hyper+x").is_none());
    }
}
