//! Reusable controls.

use super::icons::{self, Icon};
use super::style::*;
use super::{Col, FontKind, Id, R, Resp, Ui, sub};
use crate::texture::TextureId;
use crate::x11::keys::sym;

/// Background of a tile.
pub fn tile(ui: &mut Ui, r: R, radius: f32, hovered: bool, active: bool) {
    let bg = match (active, hovered) {
        (true, _) => TILE_ACTIVE,
        (false, true) => TILE_HOVER,
        (false, false) => TILE,
    };
    let edge = if active { CREAM.fade(0.16) } else { TILE_EDGE };
    ui.p.rrect(r, radius, bg);
    ui.p.rrect_stroke(r, radius, 1.0, edge);
}

/// A clickable tile. Returns true when clicked.
pub fn tile_button(ui: &mut Ui, id: Id, r: R, radius: f32, active: bool) -> bool {
    let resp = ui.interact(id, r);
    tile(ui, r, radius, resp.hovered, active);
    resp.clicked
}

/// A texture tile: icon on top, name on one or two lines below.
pub fn texture_card(
    ui: &mut Ui,
    id: Id,
    r: R,
    tex: TextureId,
    active: bool,
    icon_size: f32,
    font: f32,
) -> Resp {
    let resp = ui.interact(id, r);
    tile(ui, r, 14.0, resp.hovered, active);
    icons::draw(&mut ui.p, Icon::Texture(tex), r.cx(), r.y + r.h * 0.33, icon_size, CREAM);
    let line = font * 1.2;
    let (a, b) = tex.lines();
    let top = r.y + r.h * 0.52;
    if b.is_empty() {
        let row = R::new(r.x, top + line * 0.5, r.w, line);
        ui.p.text_center(row, a, FontKind::Semi, font, TEXT);
    } else {
        for (i, text) in [a, b].into_iter().enumerate() {
            let row = R::new(r.x, top + i as f32 * line, r.w, line);
            ui.p.text_center(row, text, FontKind::Semi, font, TEXT);
        }
    }
    resp
}

/// Background card for a settings row.
pub fn card(ui: &mut Ui, r: R) {
    ui.p.rrect(r, 12.0, ROW);
    ui.p.rrect_stroke(r, 12.0, 1.0, TILE_EDGE);
}

/// A 36×20 switch. Returns true when clicked.
pub fn toggle(ui: &mut Ui, id: Id, x: f32, y: f32, on: bool) -> bool {
    let r = R::new(x, y, 36.0, 20.0);
    let resp = ui.interact(id, r);
    let track = if on { GREEN } else { Col(0, 0, 0, if resp.hovered { 110 } else { 90 }) };
    ui.p.rrect(r, 10.0, track);
    let knob_x = if on { r.right() - 10.0 } else { r.x + 10.0 };
    ui.p.circle(knob_x, r.cy(), 7.6, CREAM);
    resp.clicked
}

/// A horizontal slider over 0–1. Returns the new value while it's dragged
/// or clicked.
pub fn slider(ui: &mut Ui, id: Id, r: R, value: f32, enabled: bool) -> Option<f32> {
    let hit = R::new(r.x - 6.0, r.cy() - 10.0, r.w + 12.0, 20.0);
    let resp = ui.interact(id, hit);
    let mut v = value.clamp(0.0, 1.0);
    let mut changed = None;
    // A quick click can arrive as press + release in one batch, so react to
    // `clicked` as well as `pressed`.
    if (resp.pressed || resp.clicked)
        && let Some((mx, _)) = ui.mouse()
    {
        let nv = ((mx - r.x) / r.w).clamp(0.0, 1.0);
        if (nv - v).abs() > f32::EPSILON {
            changed = Some(nv);
            v = nv;
        }
    }
    let track = R::new(r.x, r.cy() - 3.0, r.w, 6.0);
    let fill = if enabled { SLIDER_FILL } else { SLIDER_FILL.fade(0.45) };
    ui.p.rrect(track, 3.0, TRACK);
    ui.p.rrect(R::new(track.x, track.y, (track.w * v).max(6.0), track.h), 3.0, fill);
    let knob_x = r.x + r.w * v;
    let knob_r = if resp.hovered || resp.pressed { 7.5 } else { 6.5 };
    let knob = if enabled { CREAM } else { CREAM.fade(0.6) };
    ui.p.circle(knob_x, r.cy() + 0.5, knob_r + 1.0, Col(0, 0, 0, 50));
    ui.p.circle(knob_x, r.cy(), knob_r, knob);
    changed
}

/// A segmented control. Returns the index of a newly selected segment.
pub fn segmented(ui: &mut Ui, id: Id, r: R, labels: &[&str], selected: usize) -> Option<usize> {
    ui.p.rrect(r, 9.0, Col(0, 0, 0, 70));
    let w = (r.w - 4.0) / labels.len().max(1) as f32;
    let mut picked = None;
    for (i, label) in labels.iter().enumerate() {
        let cell = R::new(r.x + 2.0 + i as f32 * w, r.y + 2.0, w, r.h - 4.0);
        let resp = ui.interact(sub(id, i), cell);
        if i == selected {
            ui.p.rrect(cell, 7.5, Col(255, 255, 255, 34));
        } else if resp.hovered {
            ui.p.rrect(cell, 7.5, Col(255, 255, 255, 12));
        }
        let col = if i == selected { TEXT } else { MUTED };
        ui.p.text_center(cell, label, FontKind::Semi, 12.0, col);
        if resp.clicked && i != selected {
            picked = Some(i);
        }
    }
    picked
}

pub enum ButtonStyle {
    /// Filled cream, for the main action.
    Primary,
    Outline,
}

pub fn button(ui: &mut Ui, id: Id, r: R, label: &str, style: ButtonStyle) -> bool {
    let resp = ui.interact(id, r);
    let radius = r.h * 0.5;
    match style {
        ButtonStyle::Primary => {
            let bg = if resp.hovered { CREAM } else { CREAM.fade(0.92) };
            ui.p.rrect(r, radius, bg);
            ui.p.text_center(r, label, FontKind::Semi, 12.5, DEEP_GREEN);
        }
        ButtonStyle::Outline => {
            if resp.hovered {
                ui.p.rrect(r, radius, Col(255, 255, 255, 12));
            }
            ui.p.rrect_stroke(r, radius, 1.0, CREAM.fade(0.28));
            ui.p.text_center(r, label, FontKind::Semi, 12.0, MUTED);
        }
    }
    resp.clicked
}

/// A small pill-shaped toggle button.
pub fn chip(ui: &mut Ui, id: Id, r: R, label: &str, selected: bool) -> bool {
    let resp = ui.interact(id, r);
    tile(ui, r, r.h * 0.5, resp.hovered, selected);
    let col = if selected { CREAM } else { TEXT };
    ui.p.text_center(r, label, FontKind::Semi, 11.5, col);
    resp.clicked
}

/// A text link, optionally with a trailing chevron.
pub fn link(ui: &mut Ui, id: Id, cx: f32, cy: f32, label: &str, chevron: bool) -> bool {
    let w = ui.p.text_w(label, FontKind::Semi, 11.5) + if chevron { 12.0 } else { 0.0 };
    let resp = ui.interact(id, R::new(cx - w * 0.5 - 6.0, cy - 10.0, w + 12.0, 20.0));
    let col = if resp.hovered { TEXT } else { MUTED };
    let x = cx - w * 0.5;
    let tw = ui.p.text_vc(x, cy, label, FontKind::Semi, 11.5, col);
    if chevron {
        icons::draw(&mut ui.p, Icon::Chevron, x + tw + 6.0, cy + 0.5, 9.0, col);
    }
    resp.clicked
}

/// An uppercase section heading.
pub fn caption(ui: &mut Ui, x: f32, y: f32, text: &str) {
    ui.p.text(x, y, &text.to_uppercase(), FontKind::Bold, 10.0, FAINT);
}

/// An icon in a cream circle, as used in the settings sidebar.
pub fn badge_icon(ui: &mut Ui, icon: Icon, cx: f32, cy: f32, r: f32) {
    ui.p.circle(cx, cy, r, CREAM);
    icons::draw(&mut ui.p, icon, cx, cy, r * 1.15, DEEP_GREEN);
}

#[derive(Default, Clone)]
pub struct TextBuf {
    pub text: String,
    /// Cursor position in characters.
    pub cursor: usize,
}

impl TextBuf {
    pub fn set(&mut self, s: &str) {
        self.text = s.to_string();
        self.cursor = self.text.chars().count();
    }

    fn byte(&self, ch: usize) -> usize {
        self.text.char_indices().nth(ch).map_or(self.text.len(), |(b, _)| b)
    }
}

/// What happened to a text field this frame.
#[derive(PartialEq, Eq)]
pub enum FieldEvent {
    None,
    Edited,
    Submitted,
}

const MAX_FIELD_CHARS: usize = 96;

/// A single-line text input.
pub fn text_field(ui: &mut Ui, id: Id, r: R, buf: &mut TextBuf, placeholder: &str) -> FieldEvent {
    let resp = ui.interact(id, r);
    if (resp.pressed || resp.clicked) && !ui.focused(id) {
        ui.set_focus(Some(id));
    }
    let focused = ui.focused(id);
    let mut event = FieldEvent::None;
    if focused {
        for k in ui.keys().to_vec() {
            let len = buf.text.chars().count();
            match k.keysym {
                sym::RETURN | sym::KP_ENTER => event = FieldEvent::Submitted,
                sym::ESCAPE => ui.set_focus(None),
                sym::BACKSPACE if buf.cursor > 0 => {
                    let range = buf.byte(buf.cursor - 1)..buf.byte(buf.cursor);
                    buf.text.replace_range(range, "");
                    buf.cursor -= 1;
                    event = FieldEvent::Edited;
                }
                sym::DELETE if buf.cursor < len => {
                    let range = buf.byte(buf.cursor)..buf.byte(buf.cursor + 1);
                    buf.text.replace_range(range, "");
                    event = FieldEvent::Edited;
                }
                sym::LEFT => buf.cursor = buf.cursor.saturating_sub(1),
                sym::RIGHT => buf.cursor = (buf.cursor + 1).min(len),
                sym::HOME => buf.cursor = 0,
                sym::END => buf.cursor = len,
                _ => {
                    if let Some(c) = k.ch.filter(|c| !c.is_control())
                        && len < MAX_FIELD_CHARS
                    {
                        let at = buf.byte(buf.cursor);
                        buf.text.insert(at, c);
                        buf.cursor += 1;
                        event = FieldEvent::Edited;
                    }
                }
            }
        }
    }
    let (bg, edge) =
        if focused { (Col(0, 0, 0, 95), CREAM.fade(0.45)) } else { (Col(0, 0, 0, 70), TILE_EDGE) };
    ui.p.rrect(r, 9.0, bg);
    ui.p.rrect_stroke(r, 9.0, 1.0, edge);
    let tx = r.x + 10.0;
    if buf.text.is_empty() && !focused {
        ui.p.text_vc(tx, r.cy(), placeholder, FontKind::Medium, 12.5, FAINT);
    } else {
        ui.p.text_vc(tx, r.cy(), &buf.text, FontKind::Medium, 12.5, TEXT);
    }
    if focused {
        let before: String = buf.text.chars().take(buf.cursor).collect();
        let cx = tx + ui.p.text_w(&before, FontKind::Medium, 12.5) + 0.5;
        ui.p.line(cx, r.cy() - 7.5, cx, r.cy() + 7.5, 1.3, CREAM);
    }
    event
}

/// A `[−] value [+]` stepper. Returns −1, 0 or +1.
pub fn stepper(ui: &mut Ui, id: Id, x: f32, y: f32, value: &str) -> i32 {
    let minus = R::new(x, y, 28.0, 28.0);
    let plus = R::new(x + 104.0, y, 28.0, 28.0);
    let mut delta = 0;
    if tile_button(ui, sub(id, 0), minus, 14.0, false) {
        delta = -1;
    }
    if tile_button(ui, sub(id, 1), plus, 14.0, false) {
        delta = 1;
    }
    icons::draw(&mut ui.p, Icon::Minus, minus.cx(), minus.cy(), 11.0, TEXT);
    icons::draw(&mut ui.p, Icon::Plus, plus.cx(), plus.cy(), 11.0, TEXT);
    let mid = R::new(x + 28.0, y, 76.0, 28.0);
    ui.p.text_center(mid, value, FontKind::Semi, 14.0, TEXT);
    delta
}
