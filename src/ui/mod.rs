//! A small immediate-mode UI. Views redraw themselves from scratch each
//! frame and ask [`Ui::interact`] whether the rectangle they just drew is
//! hovered, pressed or clicked. Frames only run when input or state changes.

pub mod icons;
pub mod paint;
pub mod style;
pub mod text;
pub mod widgets;

pub use paint::{Col, Painter, R};
pub use text::{FontKind, Fonts};

pub type Id = u64;

/// Stable id from a string (FNV-1a).
pub fn id(s: &str) -> Id {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Id of the `n`-th child of `parent` (list items, segments…).
pub fn sub(parent: Id, n: usize) -> Id {
    parent ^ (n as u64 + 1).wrapping_mul(0x9e37_79b9_7f4a_7c15)
}

/// Id of the `n`-th item of a named list.
pub fn idn(s: &str, n: usize) -> Id {
    sub(id(s), n)
}

#[derive(Debug, Clone, Copy)]
pub struct Key {
    /// Keysym with Shift/Lock applied (for text input).
    pub keysym: u32,
    /// Unshifted keysym (for recording hotkeys).
    pub base: u32,
    pub ch: Option<char>,
    pub mods: u16,
}

/// Per-surface input state, fed by X events.
#[derive(Default)]
pub struct InputState {
    pub mouse: Option<(f32, f32)>,
    pub hovered: Option<Id>,
    pub pressed: Option<Id>,
    pub clicked: Option<Id>,
    pub right_clicked: Option<Id>,
    pub scroll: f32,
    pub keys: Vec<Key>,
    pub focused: Option<Id>,
    hits: Vec<(Id, R)>,
}

impl InputState {
    fn hit(&self, x: f32, y: f32) -> Option<Id> {
        self.hits.iter().rev().find(|(_, r)| r.contains(x, y)).map(|h| h.0)
    }

    /// Returns true if the surface needs a redraw.
    pub fn motion(&mut self, x: f32, y: f32) -> bool {
        self.mouse = Some((x, y));
        if self.pressed.is_some() {
            return true;
        }
        let h = self.hit(x, y);
        if h != self.hovered {
            self.hovered = h;
            return true;
        }
        false
    }

    pub fn leave(&mut self) -> bool {
        if self.pressed.is_some() {
            return false;
        }
        self.mouse = None;
        self.hovered.take().is_some()
    }

    pub fn press(&mut self, button: u8, x: f32, y: f32) -> bool {
        self.mouse = Some((x, y));
        match button {
            1 => {
                self.pressed = self.hit(x, y);
                if self.pressed != self.focused {
                    self.focused = None;
                }
                true
            }
            3 => {
                self.right_clicked = self.hit(x, y);
                self.right_clicked.is_some()
            }
            4 => {
                self.scroll -= 1.0;
                true
            }
            5 => {
                self.scroll += 1.0;
                true
            }
            _ => false,
        }
    }

    pub fn release(&mut self, button: u8, x: f32, y: f32) -> bool {
        if button != 1 {
            return false;
        }
        self.mouse = Some((x, y));
        let h = self.hit(x, y);
        if let Some(p) = self.pressed.take()
            && h == Some(p)
        {
            self.clicked = Some(p);
        }
        self.hovered = h;
        true
    }

    pub fn key(&mut self, k: Key) -> bool {
        self.keys.push(k);
        true
    }

    fn has_events(&self) -> bool {
        self.clicked.is_some()
            || self.right_clicked.is_some()
            || self.scroll != 0.0
            || !self.keys.is_empty()
    }

    fn end_frame(&mut self, hits: Vec<(Id, R)>, focus: Option<Option<Id>>) {
        self.hits = hits;
        self.clicked = None;
        self.right_clicked = None;
        self.scroll = 0.0;
        self.keys.clear();
        if let Some(f) = focus {
            self.focused = f;
        }
        if self.pressed.is_none() {
            self.hovered = self.mouse.and_then(|(x, y)| self.hit(x, y));
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Resp {
    pub hovered: bool,
    pub pressed: bool,
    pub clicked: bool,
    pub right_clicked: bool,
}

pub struct Ui<'a> {
    pub p: Painter<'a>,
    inp: &'a InputState,
    hits: Vec<(Id, R)>,
    focus: Option<Option<Id>>,
}

impl<'a> Ui<'a> {
    /// Registers `r` (content coordinates) as interactive and reports its state.
    pub fn interact(&mut self, id: Id, r: R) -> Resp {
        if let Some(v) = self.p.visible(r) {
            self.hits.push((id, v));
        }
        Resp {
            hovered: self.inp.hovered == Some(id),
            pressed: self.inp.pressed == Some(id),
            clicked: self.inp.clicked == Some(id),
            right_clicked: self.inp.right_clicked == Some(id),
        }
    }

    /// Mouse position in content coordinates.
    pub fn mouse(&self) -> Option<(f32, f32)> {
        self.inp.mouse.map(|(x, y)| (x, y - self.p.dy))
    }

    pub fn raw_mouse(&self) -> Option<(f32, f32)> {
        self.inp.mouse
    }

    pub fn scroll(&self) -> f32 {
        self.inp.scroll
    }

    pub fn keys(&self) -> &[Key] {
        &self.inp.keys
    }

    pub fn focused(&self, id: Id) -> bool {
        match self.focus {
            Some(f) => f == Some(id),
            None => self.inp.focused == Some(id),
        }
    }

    pub fn set_focus(&mut self, id: Option<Id>) {
        self.focus = Some(id);
    }
}

/// Runs one frame. If input events are pending, the view runs twice: once to
/// handle them and once more so the picture reflects the new state.
pub fn frame(
    pm: &mut tiny_skia::Pixmap,
    input: &mut InputState,
    fonts: &mut Fonts,
    scale: f32,
    mut view: impl FnMut(&mut Ui),
) {
    let passes = if input.has_events() { 2 } else { 1 };
    for _ in 0..passes {
        pm.fill(tiny_skia::Color::TRANSPARENT);
        let mut ui =
            Ui { p: Painter::new(pm, fonts, scale), inp: input, hits: Vec::new(), focus: None };
        view(&mut ui);
        let Ui { hits, focus, .. } = ui;
        input.end_frame(hits, focus);
    }
}
