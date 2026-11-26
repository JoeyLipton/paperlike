//! The control center: the popup that opens from the tray icon.
//!
//! ```text
//!              paperlike
//!   ┌─────────────────────┐ ┌──────┐
//!   │ (p) Paperlike Effect│ │Snooze│
//!   ├─────────────────────┤ ├──────┤
//!   │ Intensity       22% │ │ App  │
//!   ├─────────────────────┤ ├──────┤
//!   │ Desk Lamp       (o) │ │Theme │
//!   └─────────────────────┘ └──────┘
//!   [tex]  [tex]  [tex]  [tex]
//!            All textures ›
//!          ( Go to Settings )
//! ```

use super::{Ctx, app_mark, fmt_duration, wordmark};
use crate::cmd::Cmd;
use crate::config::{ExceptionMode, ThemeMode};
use crate::lamp::LampPreset;
use crate::ui::icons::{self, Icon};
use crate::ui::style::*;
use crate::ui::widgets::{self, tile};
use crate::ui::{Col, FontKind, R, Ui, id, idn};
use crate::views::settings::Page;

pub const WIDTH: f32 = 304.0;
const PAD: f32 = 16.0;
const GAP: f32 = 8.0;
/// Width of the square tiles on the right.
const SIDE: f32 = 64.0;

#[derive(Default)]
pub struct ControlCenter {
    snooze_picker: bool,
}

impl ControlCenter {
    fn panel_height(&self, compositor: bool) -> f32 {
        let mut h = 56.0;
        if !compositor {
            h += 52.0;
        }
        h += 58.0 + GAP + 58.0 + GAP + 50.0 + GAP;
        if self.snooze_picker {
            h += 40.0 + GAP;
        }
        h + 66.0 + 34.0
    }

    /// Height of the whole popup, including the button below the panel.
    pub fn height(&self, compositor: bool) -> f32 {
        self.panel_height(compositor) + 48.0
    }

    pub fn view(&mut self, ui: &mut Ui, ctx: &mut Ctx) {
        let panel_h = self.panel_height(ctx.st.compositor);
        let panel = R::new(0.5, 0.5, WIDTH - 1.0, panel_h - 1.0);
        ui.p.rrect_vgrad(panel, 24.0, PANEL_TOP, PANEL_BOTTOM);
        ui.p.rrect_stroke(panel, 24.0, 1.0, PANEL_EDGE);
        wordmark(ui, WIDTH * 0.5, 12.0, 27.0);

        let inner = WIDTH - 2.0 * PAD;
        let wide = inner - SIDE - GAP;
        let side_x = PAD + wide + GAP;
        let mut y = 56.0;

        if !ctx.st.compositor {
            let r = R::new(PAD, y, inner, 44.0);
            ui.p.rrect(r, 14.0, WARN.fade(0.16));
            icons::draw(&mut ui.p, Icon::Warning, r.x + 20.0, r.cy(), 15.0, WARN);
            let (x, top) = (r.x + 38.0, r.y + 6.0);
            ui.p.text(x, top, "No compositor running", FontKind::Semi, 12.5, TEXT);
            let hint = "Start picom to see the effect";
            ui.p.text(x, top + 17.0, hint, FontKind::Medium, 11.0, MUTED);
            y += 52.0;
        }

        self.effect_row(ui, ctx, y, wide, side_x);
        y += 58.0 + GAP;
        intensity_row(ui, ctx, y, wide, side_x);
        y += 58.0 + GAP;
        lamp_row(ui, ctx, y, wide, side_x);
        y += 50.0 + GAP;

        if self.snooze_picker {
            let w = (inner - 3.0 * GAP) / 4.0;
            for (i, minutes) in [1u32, 5, 10, 15].into_iter().enumerate() {
                let r = R::new(PAD + i as f32 * (w + GAP), y, w, 40.0);
                if widgets::chip(ui, idn("cc.snooze", i), r, &format!("{minutes} min"), false) {
                    ctx.send(Cmd::Snooze(minutes));
                    self.snooze_picker = false;
                }
            }
            y += 40.0 + GAP;
        }

        // Pinned textures.
        let current = ctx.cfg.texture(ctx.st.dark_now);
        let w = (inner - 3.0 * GAP) / 4.0;
        if ctx.cfg.pinned.is_empty() {
            let r = R::new(PAD, y, inner, 66.0);
            let hint = "Pin textures in Settings › Texture";
            ui.p.text_center(r, hint, FontKind::Medium, 11.5, FAINT);
        }
        for (i, tex) in ctx.cfg.pinned.clone().into_iter().enumerate() {
            let r = R::new(PAD + i as f32 * (w + GAP), y, w, 66.0);
            let active = tex == current && ctx.cfg.enabled;
            let resp = widgets::texture_card(ui, idn("cc.tex", i), r, tex, active, 17.0, 10.0);
            if resp.clicked {
                ctx.send(Cmd::SetTexture { tex, dark: None });
            }
        }
        y += 66.0 + 16.0;

        if widgets::link(ui, id("cc.all"), WIDTH * 0.5, y + 6.0, "All textures", true) {
            ctx.send(Cmd::OpenSettings(Some(Page::Texture)));
        }

        let pill = R::new(WIDTH * 0.5 - 64.0, panel_h + 10.0, 128.0, 32.0);
        let resp = ui.interact(id("cc.settings"), pill);
        let bg = if resp.hovered { PANEL_TOP } else { PANEL_BOTTOM };
        ui.p.rrect(pill, 16.0, bg);
        ui.p.rrect_stroke(pill, 16.0, 1.0, PANEL_EDGE);
        ui.p.text_center(pill, "Go to Settings", FontKind::Semi, 12.5, TEXT);
        if resp.clicked {
            ctx.send(Cmd::OpenSettings(None));
        }
    }

    /// The on/off tile and the snooze tile.
    fn effect_row(&mut self, ui: &mut Ui, ctx: &mut Ctx, y: f32, wide: f32, side_x: f32) {
        let snoozed = ctx.st.snooze_left.is_some();
        let r = R::new(PAD, y, wide, 58.0);
        if widgets::tile_button(ui, id("cc.effect"), r, 16.0, false) {
            ctx.send(if snoozed { Cmd::Unsnooze } else { Cmd::ToggleEffect });
        }
        app_mark(ui, r.x + 29.0, r.cy(), 18.0, ctx.cfg.enabled && !snoozed);
        let x = r.x + 56.0;
        ui.p.text(x, r.y + 11.0, "Paperlike Effect", FontKind::Semi, 13.5, TEXT);
        let status = if let Some(left) = ctx.st.snooze_left {
            format!("Snoozed · {}", fmt_duration(left))
        } else if !ctx.cfg.enabled {
            "Off".to_string()
        } else if let Some(reason) = &ctx.st.paused {
            format!("Paused · {reason}")
        } else {
            "On".to_string()
        };
        let status = ui.p.ellipsize(&status, FontKind::Medium, 12.0, wide - 66.0);
        ui.p.text(x, r.y + 30.0, &status, FontKind::Medium, 12.0, MUTED);

        let r = R::new(side_x, y, SIDE, 58.0);
        let active = snoozed || self.snooze_picker;
        if widgets::tile_button(ui, id("cc.snooze"), r, 16.0, active) {
            if snoozed {
                ctx.send(Cmd::Unsnooze);
            } else {
                self.snooze_picker = !self.snooze_picker;
            }
        }
        icons::draw(&mut ui.p, Icon::Bell, r.cx(), r.y + 21.0, 17.0, CREAM);
        let label = if snoozed { "Resume" } else { "Snooze" };
        let row = R::new(r.x, r.y + 33.0, r.w, 16.0);
        ui.p.text_center(row, label, FontKind::Semi, 10.5, TEXT);
    }
}

/// The intensity slider and the per-app exception tile.
fn intensity_row(ui: &mut Ui, ctx: &mut Ctx, y: f32, wide: f32, side_x: f32) {
    let dark = ctx.st.dark_now;
    let tex = ctx.cfg.texture(dark);
    let current = ctx.cfg.intensity(dark, tex);
    let r = R::new(PAD, y, wide, 58.0);
    tile(ui, r, 16.0, false, false);
    ui.p.text(r.x + 13.0, r.y + 9.0, "Intensity", FontKind::Semi, 13.5, TEXT);
    let value = format!("{current}%");
    ui.p.text_right(r.right() - 13.0, r.y + 18.0, &value, FontKind::Medium, 12.0, MUTED);

    // Small sun and big sun step the intensity by one.
    let sy = r.y + 40.0;
    let minus = R::new(r.x + 6.0, sy - 11.0, 22.0, 22.0);
    let plus = R::new(r.right() - 29.0, sy - 12.0, 24.0, 24.0);
    for (key, rect, size, delta) in [("cc.int-", minus, 11.0, -1), ("cc.int+", plus, 15.0, 1)] {
        let resp = ui.interact(id(key), rect);
        let col = if resp.hovered { CREAM } else { MUTED };
        icons::draw(&mut ui.p, Icon::Sun, rect.cx(), rect.cy(), size, col);
        if resp.clicked {
            ctx.send(Cmd::IntensityDelta(delta));
        }
    }
    let (lo, hi) = tex.range();
    let span = (hi - lo) as f32;
    let track = R::new(minus.right() + 6.0, sy - 8.0, plus.x - minus.right() - 12.0, 16.0);
    let v = (current - lo) as f32 / span;
    if let Some(nv) = widgets::slider(ui, id("cc.int"), track, v, ctx.cfg.enabled) {
        ctx.cfg.set_intensity(dark, tex, (lo as f32 + (nv * span).round()) as i32);
        ctx.changed = true;
    }

    // "Disable for <app>" (or "Run in <app>" in only-listed mode).
    let r = R::new(side_x, y, SIDE, 58.0);
    let app = ctx.st.focused_app.clone();
    let listed = app.as_deref().is_some_and(|a| ctx.cfg.exceptions.contains(a));
    if widgets::tile_button(ui, id("cc.exclude"), r, 16.0, listed)
        && let Some(a) = &app
    {
        ctx.send(Cmd::ToggleExclusion(a.clone()));
    }
    let icon_col = if app.is_some() { CREAM } else { FAINT };
    icons::draw(&mut ui.p, Icon::Shield, r.cx(), r.y + 16.0, 15.0, icon_col);
    let only = ctx.cfg.exceptions.mode == ExceptionMode::Only;
    let verb = match (only, listed) {
        (true, true) => "Runs in",
        (true, false) => "Run in",
        (false, true) => "Disabled for",
        (false, false) => "Disable for",
    };
    let name = app.as_deref().unwrap_or("this app");
    for (i, text) in [verb, name].into_iter().enumerate() {
        let text = ui.p.ellipsize(text, FontKind::Semi, 9.5, SIDE - 8.0);
        let row = R::new(r.x, r.y + 28.0 + 12.0 * i as f32, r.w, 12.0);
        ui.p.text_center(row, &text, FontKind::Semi, 9.5, TEXT);
    }
}

/// The desk lamp tile and the theme tile.
fn lamp_row(ui: &mut Ui, ctx: &mut Ctx, y: f32, wide: f32, side_x: f32) {
    let r = R::new(PAD, y, wide, 50.0);
    let on = ctx.cfg.lamp.enabled;
    if widgets::tile_button(ui, id("cc.lamp"), r, 16.0, false) {
        ctx.send(Cmd::ToggleLamp);
    }
    if on {
        let [cr, cg, cb] = crate::lamp::color(ctx.cfg.lamp.params.warmth);
        let glow = Col(cr, cg, cb, 120);
        ui.p.radial(r.x + 23.0, r.cy(), 15.0, glow, glow.fade(0.0));
    }
    let icon_col = if on { CREAM } else { MUTED };
    icons::draw(&mut ui.p, Icon::Lamp, r.x + 23.0, r.cy(), 17.0, icon_col);
    ui.p.text(r.x + 44.0, r.y + 8.0, "Desk Lamp", FontKind::Semi, 13.5, TEXT);
    let preset = LampPreset::matching(&ctx.cfg.lamp.params).map_or("Custom", |p| p.name());
    ui.p.text(r.x + 44.0, r.y + 26.0, preset, FontKind::Medium, 11.5, MUTED);
    if widgets::toggle(ui, id("cc.lamp.toggle"), r.right() - 48.0, r.cy() - 10.0, on) {
        ctx.send(Cmd::ToggleLamp);
    }

    let r = R::new(side_x, y, SIDE, 50.0);
    if widgets::tile_button(ui, id("cc.theme"), r, 16.0, false) {
        ctx.send(Cmd::CycleTheme);
    }
    let icon = match ctx.cfg.theme_mode {
        ThemeMode::Light => Icon::Sun,
        ThemeMode::Dark => Icon::Moon,
        ThemeMode::System => Icon::Auto,
        ThemeMode::Screen => Icon::Screen,
        ThemeMode::Sun => Icon::Clock,
    };
    icons::draw(&mut ui.p, icon, r.cx(), r.y + 17.0, 16.0, CREAM);
    let row = R::new(r.x, r.y + 30.0, r.w, 14.0);
    ui.p.text_center(row, ctx.cfg.theme_mode.label(), FontKind::Semi, 10.5, TEXT);
}
