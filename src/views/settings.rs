//! The settings window: a sidebar of pages, each page a column of cards.

use super::{Ctx, fmt_duration, wordmark};
use crate::cmd::Cmd;
use crate::config::{BatteryMode, ExceptionMode, HotkeyAction, ScheduleMode, ThemeMode};
use crate::lamp::LampPreset;
use crate::schedule;
use crate::texture::TextureId;
use crate::ui::icons::{self, Icon};
use crate::ui::style::*;
use crate::ui::widgets::{self, ButtonStyle, FieldEvent, TextBuf};
use crate::ui::{Col, FontKind, R, Ui, id, idn};
use crate::x11::{self, keys};

pub const WIDTH: f32 = 780.0;
pub const HEIGHT: f32 = 530.0;
const SIDEBAR: f32 = 204.0;
const X0: f32 = SIDEBAR + 28.0;
const CW: f32 = WIDTH - X0 - 28.0;
const TOP: f32 = 72.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    General,
    Texture,
    Lamp,
    Schedule,
    Exceptions,
    Display,
    Hotkeys,
    About,
}

impl Page {
    pub const ALL: [Page; 8] = [
        Page::General,
        Page::Texture,
        Page::Lamp,
        Page::Schedule,
        Page::Exceptions,
        Page::Display,
        Page::Hotkeys,
        Page::About,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Page::General => "General",
            Page::Texture => "Texture",
            Page::Lamp => "Desk Lamp",
            Page::Schedule => "Schedule",
            Page::Exceptions => "Exceptions",
            Page::Display => "Display",
            Page::Hotkeys => "Hotkeys",
            Page::About => "About",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Page::General => Icon::Shield,
            Page::Texture => Icon::Palette,
            Page::Lamp => Icon::Lamp,
            Page::Schedule => Icon::Clock,
            Page::Exceptions => Icon::Ban,
            Page::Display => Icon::Monitor,
            Page::Hotkeys => Icon::Keyboard,
            Page::About => Icon::Info,
        }
    }

    pub fn parse(s: &str) -> Option<Page> {
        let s = s.to_ascii_lowercase();
        Some(match s.as_str() {
            "general" => Page::General,
            "texture" | "textures" => Page::Texture,
            "lamp" | "desk-lamp" | "desklamp" => Page::Lamp,
            "schedule" => Page::Schedule,
            "exceptions" | "apps" => Page::Exceptions,
            "display" | "displays" | "monitors" => Page::Display,
            "hotkeys" | "keys" | "shortcuts" => Page::Hotkeys,
            "about" => Page::About,
            _ => return None,
        })
    }
}

pub struct Settings {
    pub page: Page,
    scroll: f32,
    content_h: f32,
    new_app: TextBuf,
    lat: TextBuf,
    lon: TextBuf,
    loc_error: Option<String>,
    pub recording: Option<HotkeyAction>,
    confirm_reset: bool,
}

impl Settings {
    pub fn new(page: Page) -> Settings {
        Settings {
            page,
            scroll: 0.0,
            content_h: 0.0,
            new_app: TextBuf::default(),
            lat: TextBuf::default(),
            lon: TextBuf::default(),
            loc_error: None,
            recording: None,
            confirm_reset: false,
        }
    }

    pub fn show(&mut self, page: Page, ctx_cmds: &mut Vec<Cmd>) {
        if self.page != page {
            self.page = page;
            self.scroll = 0.0;
        }
        if page == Page::Exceptions {
            ctx_cmds.push(Cmd::RefreshClients);
        }
    }

    pub fn view(&mut self, ui: &mut Ui, ctx: &mut Ctx) {
        let full = R::new(0.5, 0.5, WIDTH - 1.0, HEIGHT - 1.0);
        ui.p.rrect_vgrad(full, 18.0, PANEL_TOP, PANEL_BOTTOM);
        ui.p.rrect(R::new(SIDEBAR, 0.5, WIDTH - SIDEBAR - 0.5, HEIGHT - 1.0), 0.0, CONTENT_SHADE);
        // Re-round the right corners over the shade.
        ui.p.rrect_stroke(full, 18.0, 1.0, PANEL_EDGE);
        ui.p.line(SIDEBAR, 14.0, SIDEBAR, HEIGHT - 14.0, 1.0, Col(255, 255, 255, 18));

        self.sidebar(ui, ctx);
        self.header(ui);

        // Scrollable page body.
        let area = R::new(SIDEBAR + 1.0, TOP - 8.0, WIDTH - SIDEBAR - 2.0, HEIGHT - TOP + 7.0);
        if ui.raw_mouse().is_some_and(|(x, y)| area.contains(x, y)) && ui.scroll() != 0.0 {
            self.scroll += ui.scroll() * 48.0;
        }
        let max = (self.content_h - (HEIGHT - TOP - 18.0)).max(0.0);
        self.scroll = self.scroll.clamp(0.0, max);
        ui.p.set_clip(Some(area));
        ui.p.dy = TOP - self.scroll;
        let h = match self.page {
            Page::General => self.general(ui, ctx),
            Page::Texture => self.texture(ui, ctx),
            Page::Lamp => self.lamp(ui, ctx),
            Page::Schedule => self.schedule(ui, ctx),
            Page::Exceptions => self.exceptions(ui, ctx),
            Page::Display => self.display(ui, ctx),
            Page::Hotkeys => self.hotkeys(ui, ctx),
            Page::About => self.about(ui, ctx),
        };
        self.content_h = h;
        ui.p.dy = 0.0;
        ui.p.set_clip(None);
        if max > 0.0 {
            // Scroll indicator.
            let track_h = HEIGHT - TOP - 24.0;
            let thumb = (track_h * (HEIGHT - TOP) / (self.content_h + 18.0)).max(28.0);
            let ty = TOP + (track_h - thumb) * (self.scroll / max);
            ui.p.rrect(R::new(WIDTH - 9.0, ty, 4.0, thumb), 2.0, Col(255, 255, 255, 40));
        }
    }

    fn sidebar(&mut self, ui: &mut Ui, ctx: &mut Ctx) {
        let ww = ui.p.text_w("paperlike", FontKind::Brand, 24.0);
        ui.p.text(22.0, 16.0, "paperlike", FontKind::Brand, 24.0, CREAM);
        ui.p.text(
            28.0 + ww,
            26.0,
            concat!("v", env!("CARGO_PKG_VERSION")),
            FontKind::Medium,
            10.5,
            FAINT,
        );
        let mut y = 64.0;
        for (i, page) in Page::ALL.into_iter().enumerate() {
            let r = R::new(12.0, y, SIDEBAR - 24.0, 36.0);
            let resp = ui.interact(idn("side", i), r);
            if page == self.page {
                ui.p.rrect(r, 10.0, Col(0, 0, 0, 78));
            } else if resp.hovered {
                ui.p.rrect(r, 10.0, Col(255, 255, 255, 14));
            }
            widgets::badge_icon(ui, page.icon(), r.x + 20.0, r.cy(), 10.5);
            ui.p.text_vc(r.x + 40.0, r.cy(), page.label(), FontKind::Medium, 13.0, TEXT);
            if resp.clicked {
                self.show(page, ctx.cmds);
                self.recording = None;
            }
            y += 38.0;
        }
        let r = R::new(16.0, HEIGHT - 50.0, SIDEBAR - 32.0, 32.0);
        let label = if self.confirm_reset { "Click again to reset" } else { "Reset to defaults" };
        if widgets::button(ui, id("reset"), r, label, ButtonStyle::Outline) {
            if self.confirm_reset {
                ctx.send(Cmd::ResetDefaults);
                self.confirm_reset = false;
            } else {
                self.confirm_reset = true;
            }
        }
    }

    fn header(&mut self, ui: &mut Ui) {
        let r = R::new(X0, 22.0, 30.0, 30.0);
        ui.p.rrect(r, 8.0, CREAM);
        icons::draw(&mut ui.p, self.page.icon(), r.cx(), r.cy(), 17.0, DEEP_GREEN);
        ui.p.text_vc(X0 + 42.0, r.cy(), self.page.label(), FontKind::Semi, 19.0, TEXT);
    }

    // ---- page helpers -----------------------------------------------------

    /// Draws a card with a title and wrapped description, leaving `extra`
    /// pixels below for controls and `right` pixels on the right for a
    /// control. Returns the card and the y where extra content starts.
    fn row(ui: &mut Ui, y: f32, title: &str, desc: &str, extra: f32, right: f32) -> (R, f32) {
        let lines = if desc.is_empty() {
            Vec::new()
        } else {
            ui.p.wrap(desc, FontKind::Medium, 11.5, CW - 32.0 - right)
        };
        let desc_h = lines.len() as f32 * 17.0;
        let h = 14.0 + 19.0 + desc_h + 12.0 + extra;
        let card = R::new(X0, y, CW, h);
        widgets::card(ui, card);
        ui.p.text(X0 + 16.0, y + 13.0, title, FontKind::Semi, 13.5, TEXT);
        for (i, l) in lines.iter().enumerate() {
            ui.p.text(X0 + 16.0, y + 34.0 + i as f32 * 17.0, l, FontKind::Medium, 11.5, MUTED);
        }
        (card, y + 34.0 + desc_h + 6.0)
    }

    fn row_toggle(ui: &mut Ui, y: f32, key: &str, title: &str, desc: &str, on: bool) -> (R, bool) {
        let (card, _) = Self::row(ui, y, title, desc, 0.0, 60.0);
        let clicked = widgets::toggle(ui, id(key), card.right() - 52.0, card.y + 15.0, on);
        (card, clicked)
    }

    // ---- pages ------------------------------------------------------------

    fn general(&mut self, ui: &mut Ui, ctx: &mut Ctx) -> f32 {
        let mut y = 0.0;
        let (card, clicked) = Self::row_toggle(
            ui,
            y,
            "p.effect",
            "Paperlike Effect",
            "A paper texture over your screens. It takes the edge off glare and contrast without tinting colours.",
            ctx.cfg.enabled,
        );
        if clicked {
            ctx.send(Cmd::ToggleEffect);
        }
        y = card.bottom() + 10.0;

        let desc = match ctx.st.snooze_left {
            Some(left) => format!("Snoozed. The texture comes back in {}.", fmt_duration(left)),
            None => "Hide the texture for a few minutes.".into(),
        };
        let (card, cy) = Self::row(ui, y, "Snooze", &desc, 36.0, 0.0);
        for (i, m) in [1u32, 5, 10, 15].into_iter().enumerate() {
            let r = R::new(X0 + 16.0 + i as f32 * 80.0, cy + 2.0, 72.0, 28.0);
            if widgets::chip(ui, idn("p.snz", i), r, &format!("{m} min"), false) {
                ctx.send(Cmd::Snooze(m));
            }
        }
        if ctx.st.snooze_left.is_some() {
            let r = R::new(card.right() - 126.0, cy + 2.0, 110.0, 28.0);
            if widgets::button(ui, id("p.resume"), r, "Resume now", ButtonStyle::Primary) {
                ctx.send(Cmd::Unsnooze);
            }
        }
        y = card.bottom() + 22.0;

        widgets::caption(ui, X0 + 4.0, y, "Battery");
        y += 20.0;
        let p = ctx.st.power;
        let desc = if !p.has_battery {
            "No battery detected.".to_string()
        } else {
            format!(
                "Battery {}{}.",
                p.percent.map_or("?".into(), |v| format!("{v}%")),
                if p.on_battery { " · discharging" } else { " · on AC power" }
            )
        };
        let below = ctx.cfg.battery.mode == BatteryMode::BelowPercent;
        let (card, cy) =
            Self::row(ui, y, "Pause on battery", &desc, if below { 76.0 } else { 38.0 }, 0.0);
        let modes = [BatteryMode::Never, BatteryMode::OnBattery, BatteryMode::BelowPercent];
        let sel = modes.iter().position(|&m| m == ctx.cfg.battery.mode).unwrap_or(0);
        if let Some(i) = widgets::segmented(
            ui,
            id("p.bat"),
            R::new(X0 + 16.0, cy + 2.0, 330.0, 30.0),
            &["Never", "On battery", "Below a level"],
            sel,
        ) {
            ctx.cfg.battery.mode = modes[i];
            ctx.changed = true;
        }
        if below {
            let t = ctx.cfg.battery.threshold;
            ui.p.text_vc(
                X0 + 16.0,
                cy + 56.0,
                &format!("Below {t}%"),
                FontKind::Medium,
                12.0,
                TEXT,
            );
            let r = R::new(X0 + 110.0, cy + 48.0, card.w - 140.0, 16.0);
            if let Some(v) =
                widgets::slider(ui, id("p.batslider"), r, (t as f32 - 5.0) / 90.0, true)
            {
                ctx.cfg.battery.threshold = (5.0 + v * 90.0).round() as u8;
                ctx.changed = true;
            }
        }
        y = card.bottom() + 22.0;

        widgets::caption(ui, X0 + 4.0, y, "Startup");
        y += 20.0;
        let (card, clicked) = Self::row_toggle(
            ui,
            y,
            "p.login",
            "Open at login",
            "Adds ~/.config/autostart/paperlike.desktop. i3 only runs those via dex, so you may prefer \"exec --no-startup-id paperlike\" in your i3 config.",
            ctx.st.autostart,
        );
        if clicked {
            ctx.send(Cmd::SetAutostart(!ctx.st.autostart));
        }
        card.bottom()
    }

    fn texture(&mut self, ui: &mut Ui, ctx: &mut Ctx) -> f32 {
        let mut y = 0.0;
        let mode = ctx.cfg.theme_mode;
        let desc = match mode {
            ThemeMode::Light => "Always use your light-screen texture.".to_string(),
            ThemeMode::Dark => "Always use your dark-screen texture.".to_string(),
            ThemeMode::System => format!(
                "Follows the GTK/GNOME colour scheme (currently {}).",
                if ctx.st.system_dark { "dark" } else { "light" }
            ),
            ThemeMode::Screen => {
                "Each monitor switches between light and dark based on what it's showing."
                    .to_string()
            }
            ThemeMode::Sun => {
                "Light texture by day, dark after sunset. Set your location under Schedule."
                    .to_string()
            }
        };
        let (card, cy) = Self::row(ui, y, "Texture follows", &desc, 38.0, 0.0);
        let labels: Vec<&str> = ThemeMode::ALL.iter().map(|m| m.label()).collect();
        let sel = ThemeMode::ALL.iter().position(|&m| m == mode).unwrap_or(0);
        if let Some(i) = widgets::segmented(
            ui,
            id("t.mode"),
            R::new(X0 + 16.0, cy + 2.0, CW - 32.0, 30.0),
            &labels,
            sel,
        ) {
            ctx.send(Cmd::Theme(ThemeMode::ALL[i]));
        }
        y = card.bottom() + 18.0;

        let col_w = (CW - 16.0) / 2.0;
        let cell_w = (col_w - 16.0) / 3.0;
        let mut hovered = None;
        let rows = TextureId::ALL.len().div_ceil(3) as f32;
        for (c, dark) in [false, true].into_iter().enumerate() {
            let x = X0 + c as f32 * (col_w + 16.0);
            icons::draw(
                &mut ui.p,
                if dark { Icon::Moon } else { Icon::Sun },
                x + 8.0,
                y + 9.0,
                14.0,
                CREAM,
            );
            ui.p.text_vc(
                x + 22.0,
                y + 9.0,
                if dark { "Dark screens" } else { "Light screens" },
                FontKind::Semi,
                13.5,
                TEXT,
            );
            if ctx.st.dark_now == dark {
                let b = R::new(x + col_w - 52.0, y + 1.0, 52.0, 17.0);
                ui.p.rrect(b, 8.5, GREEN.fade(0.25));
                ui.p.text_center(b, "In use", FontKind::Bold, 9.5, GREEN);
            }
            let gy = y + 28.0;
            let current = ctx.cfg.texture(dark);
            for (i, t) in TextureId::ALL.into_iter().enumerate() {
                let r = R::new(
                    x + (i % 3) as f32 * (cell_w + 8.0),
                    gy + (i / 3) as f32 * 66.0,
                    cell_w,
                    58.0,
                );
                let key = if dark { "t.dark" } else { "t.light" };
                let resp = widgets::texture_card(ui, idn(key, i), r, t, t == current, 14.0, 9.5);
                if ctx.cfg.pinned.contains(&t) {
                    icons::draw(&mut ui.p, Icon::Pin, r.right() - 10.0, r.y + 10.0, 9.0, GREEN);
                }
                if resp.hovered {
                    hovered = Some(t);
                }
                if resp.clicked {
                    ctx.send(Cmd::SetTexture { tex: t, dark: Some(dark) });
                }
                if resp.right_clicked {
                    ctx.cfg.toggle_pin(t);
                    ctx.changed = true;
                }
            }
            let sy = gy + rows * 66.0 + 4.0;
            let v = ctx.cfg.intensity(dark, current);
            ui.p.text_vc(x + 2.0, sy + 8.0, "Intensity", FontKind::Medium, 12.0, TEXT);
            ui.p.text_right(x + col_w, sy + 8.0, &format!("{v}%"), FontKind::Medium, 12.0, MUTED);
            let (lo, hi) = current.range();
            let span = (hi - lo) as f32;
            let r = R::new(x + 6.0, sy + 20.0, col_w - 12.0, 16.0);
            if let Some(nv) = widgets::slider(ui, idn("t.int", c), r, (v - lo) as f32 / span, true)
            {
                ctx.cfg.set_intensity(dark, current, (lo as f32 + (nv * span).round()) as i32);
                ctx.changed = true;
            }
        }
        y += 28.0 + rows * 66.0 + 48.0;
        let hint = match hovered {
            Some(t) => format!("{}: {}", t.name(), t.description()),
            None => "Click to choose. Right-click to pin to the control center (up to 4).".into(),
        };
        ui.p.text(
            X0 + 2.0,
            y,
            &hint,
            FontKind::Medium,
            11.0,
            if hovered.is_some() { MUTED } else { FAINT },
        );
        y += 28.0;

        let (card, cy) = Self::row(
            ui,
            y,
            "Grain scale",
            "Scales the grain up. Try 1.5× or 2× on HiDPI screens. This is for the main monitor; others are matched to it (see Display).",
            38.0,
            0.0,
        );
        let scales = [1.0f32, 1.5, 2.0, 3.0];
        let sel = scales
            .iter()
            .enumerate()
            .min_by(|a, b| {
                (a.1 - ctx.cfg.grain_scale).abs().total_cmp(&(b.1 - ctx.cfg.grain_scale).abs())
            })
            .map_or(0, |(i, _)| i);
        if let Some(i) = widgets::segmented(
            ui,
            id("t.grain"),
            R::new(X0 + 16.0, cy + 2.0, 300.0, 30.0),
            &["1×", "1.5×", "2×", "3×"],
            sel,
        ) {
            ctx.cfg.grain_scale = scales[i];
            ctx.changed = true;
        }
        card.bottom()
    }

    fn lamp(&mut self, ui: &mut Ui, ctx: &mut Ctx) -> f32 {
        let mut y = 0.0;
        let (card, clicked) = Self::row_toggle(
            ui,
            y,
            "l.on",
            "Desk Lamp",
            "A warm glow from one corner of the screen, on top of the texture.",
            ctx.cfg.lamp.enabled,
        );
        if clicked {
            ctx.send(Cmd::ToggleLamp);
        }
        y = card.bottom() + 10.0;
        let (card, clicked) = Self::row_toggle(
            ui,
            y,
            "l.dark",
            "Dark mode only",
            "Only light the lamp while the dark-screen texture is showing.",
            ctx.cfg.lamp.dark_only,
        );
        if clicked {
            ctx.cfg.lamp.dark_only = !ctx.cfg.lamp.dark_only;
            ctx.changed = true;
        }
        y = card.bottom() + 22.0;

        widgets::caption(ui, X0 + 4.0, y, "Presets");
        y += 20.0;
        let w = (CW - 16.0) / 3.0;
        let current = LampPreset::matching(&ctx.cfg.lamp.params);
        for (i, p) in LampPreset::ALL.into_iter().enumerate() {
            let r = R::new(X0 + (i % 3) as f32 * (w + 8.0), y + (i / 3) as f32 * 62.0, w, 54.0);
            if widgets::tile_button(ui, idn("l.preset", i), r, 14.0, current == Some(p)) {
                ctx.send(Cmd::LampPreset(p));
            }
            let [cr, cg, cb] = crate::lamp::color(p.params().warmth);
            let col = Col(cr, cg, cb, 255);
            ui.p.radial(r.x + 26.0, r.cy(), 15.0, col, col.fade(0.0));
            ui.p.circle(r.x + 26.0, r.cy(), 5.0, Col(255, 248, 235, 255));
            ui.p.text_vc(r.x + 48.0, r.cy(), p.name(), FontKind::Semi, 12.5, TEXT);
        }
        y += 2.0 * 62.0 + 16.0;

        widgets::caption(ui, X0 + 4.0, y, "Adjustments");
        y += 20.0;
        let card = R::new(X0, y, CW, 3.0 * 42.0 + 58.0);
        widgets::card(ui, card);
        let positions = crate::lamp::LampPosition::ALL;
        let labels: Vec<&str> = positions.iter().map(|p| p.label()).collect();
        let sel = positions.iter().position(|&p| p == ctx.cfg.lamp.params.position).unwrap_or(0);
        ui.p.text_vc(X0 + 16.0, y + 27.0, "Position", FontKind::Medium, 12.5, TEXT);
        if let Some(i) = widgets::segmented(
            ui,
            id("l.pos"),
            R::new(X0 + 140.0, y + 12.0, CW - 156.0, 30.0),
            &labels,
            sel,
        ) {
            ctx.cfg.lamp.params.position = positions[i];
            ctx.changed = true;
        }
        let params = &mut ctx.cfg.lamp.params;
        let mut changed = false;
        for (i, (label, v)) in [
            ("Warmth", &mut params.warmth),
            ("Spread", &mut params.spread),
            ("Dimming", &mut params.dimming),
        ]
        .into_iter()
        .enumerate()
        {
            let cy = y + 69.0 + i as f32 * 42.0;
            ui.p.text_vc(X0 + 16.0, cy, label, FontKind::Medium, 12.5, TEXT);
            ui.p.text_right(
                card.right() - 16.0,
                cy,
                &format!("{v}%"),
                FontKind::Medium,
                12.0,
                MUTED,
            );
            let r = R::new(X0 + 140.0, cy - 8.0, CW - 140.0 - 60.0, 16.0);
            if let Some(nv) = widgets::slider(ui, idn("l.adj", i), r, *v as f32 / 100.0, true) {
                *v = (nv * 100.0).round() as u8;
                changed = true;
            }
        }
        if changed {
            ctx.changed = true;
        }
        card.bottom()
    }

    fn schedule(&mut self, ui: &mut Ui, ctx: &mut Ctx) -> f32 {
        let mut y = 0.0;
        let mode = ctx.cfg.schedule.mode;
        let desc = match mode {
            ScheduleMode::Always => "The texture is on all day.",
            ScheduleMode::Day => "On from sunrise to sunset.",
            ScheduleMode::Night => "On from sunset to sunrise.",
            ScheduleMode::Custom => "On during the hours below.",
        };
        let (card, cy) = Self::row(ui, y, "Schedule", desc, 38.0, 0.0);
        let modes =
            [ScheduleMode::Always, ScheduleMode::Day, ScheduleMode::Night, ScheduleMode::Custom];
        let sel = modes.iter().position(|&m| m == mode).unwrap_or(0);
        if let Some(i) = widgets::segmented(
            ui,
            id("s.mode"),
            R::new(X0 + 16.0, cy + 2.0, 400.0, 30.0),
            &["Always", "Daytime", "Night", "Custom"],
            sel,
        ) {
            ctx.cfg.schedule.mode = modes[i];
            ctx.changed = true;
        }
        y = card.bottom() + 10.0;

        if mode == ScheduleMode::Custom {
            let (card, cy) = Self::row(
                ui,
                y,
                "Hours",
                "In 15-minute steps. The window can run past midnight.",
                40.0,
                0.0,
            );
            let s = &mut ctx.cfg.schedule;
            ui.p.text_vc(X0 + 16.0, cy + 16.0, "From", FontKind::Medium, 12.5, TEXT);
            let d = widgets::stepper(
                ui,
                id("s.from"),
                X0 + 60.0,
                cy + 2.0,
                &schedule::fmt_hm(s.start as f64),
            );
            if d != 0 {
                s.start = ((s.start as i32 + d * 15).rem_euclid(1440)) as u16;
                ctx.changed = true;
            }
            ui.p.text_vc(X0 + 220.0, cy + 16.0, "To", FontKind::Medium, 12.5, TEXT);
            let s = &mut ctx.cfg.schedule;
            let d = widgets::stepper(
                ui,
                id("s.to"),
                X0 + 250.0,
                cy + 2.0,
                &schedule::fmt_hm(s.end as f64),
            );
            if d != 0 {
                s.end = ((s.end as i32 + d * 15).rem_euclid(1440)) as u16;
                ctx.changed = true;
            }
            y = card.bottom() + 10.0;
        }

        // The location is also used by the "Sun" texture mode.
        if !ui.focused(id("s.lat")) && !ui.focused(id("s.lon")) {
            let fmt = |v: Option<f64>| v.map(|v| format!("{v:.2}")).unwrap_or_default();
            self.lat.set(&fmt(ctx.cfg.schedule.latitude));
            self.lon.set(&fmt(ctx.cfg.schedule.longitude));
        }
        let today = match ctx.st.sun {
            Some((rise, set)) => format!(
                "Today: sunrise {} · sunset {}.",
                schedule::fmt_hm(rise),
                schedule::fmt_hm(set)
            ),
            None if ctx.cfg.schedule.latitude.is_some() => {
                "The sun doesn't rise or set here today.".into()
            }
            None => "Without a location, day is 07:00–19:00.".into(),
        };
        let desc =
            format!("Latitude and longitude in decimal degrees, e.g. 52.52 and 13.40. {today}");
        let (card, cy) = Self::row(
            ui,
            y,
            "Location",
            &desc,
            40.0 + if self.loc_error.is_some() { 18.0 } else { 0.0 },
            0.0,
        );
        let e1 = widgets::text_field(
            ui,
            id("s.lat"),
            R::new(X0 + 16.0, cy + 2.0, 130.0, 30.0),
            &mut self.lat,
            "Latitude",
        );
        let e2 = widgets::text_field(
            ui,
            id("s.lon"),
            R::new(X0 + 156.0, cy + 2.0, 130.0, 30.0),
            &mut self.lon,
            "Longitude",
        );
        let save = widgets::button(
            ui,
            id("s.save"),
            R::new(X0 + 296.0, cy + 2.0, 80.0, 30.0),
            "Save",
            ButtonStyle::Primary,
        );
        let clear = ctx.cfg.schedule.latitude.is_some()
            && widgets::button(
                ui,
                id("s.clear"),
                R::new(X0 + 384.0, cy + 2.0, 80.0, 30.0),
                "Clear",
                ButtonStyle::Outline,
            );
        if save || matches!(e1, FieldEvent::Submitted) || matches!(e2, FieldEvent::Submitted) {
            let lat = self.lat.text.trim().replace(',', ".").parse::<f64>();
            let lon = self.lon.text.trim().replace(',', ".").parse::<f64>();
            match (lat, lon) {
                (Ok(a), Ok(o)) if (-90.0..=90.0).contains(&a) && (-180.0..=180.0).contains(&o) => {
                    ctx.cfg.schedule.latitude = Some(a);
                    ctx.cfg.schedule.longitude = Some(o);
                    ctx.changed = true;
                    self.loc_error = None;
                    ui.set_focus(None);
                }
                _ => {
                    self.loc_error = Some("Latitude must be −90…90 and longitude −180…180.".into())
                }
            }
        }
        if clear {
            ctx.cfg.schedule.latitude = None;
            ctx.cfg.schedule.longitude = None;
            ctx.changed = true;
        }
        if let Some(e) = &self.loc_error {
            ui.p.text(X0 + 16.0, cy + 40.0, e, FontKind::Medium, 11.5, DANGER);
        }
        y = card.bottom() + 14.0;

        let active = ctx.st.paused != Some(super::Pause::Schedule);
        let msg = if active {
            "The schedule currently allows the texture."
        } else {
            "The schedule is currently pausing the texture."
        };
        icons::draw(
            &mut ui.p,
            if active { Icon::Check } else { Icon::Clock },
            X0 + 10.0,
            y + 8.0,
            12.0,
            if active { GREEN } else { WARN },
        );
        ui.p.text_vc(X0 + 24.0, y + 8.0, msg, FontKind::Medium, 12.0, MUTED);
        y + 20.0
    }

    fn exceptions(&mut self, ui: &mut Ui, ctx: &mut Ctx) -> f32 {
        let mut y = 0.0;
        let mode = ctx.cfg.exceptions.mode;
        let desc = match mode {
            ExceptionMode::Exclude => "The texture hides while one of these apps is focused.",
            ExceptionMode::Only => "The texture only shows while one of these apps is focused.",
        };
        let (card, cy) = Self::row(ui, y, "Mode", desc, 38.0, 0.0);
        let sel = if mode == ExceptionMode::Exclude { 0 } else { 1 };
        if let Some(i) = widgets::segmented(
            ui,
            id("e.mode"),
            R::new(X0 + 16.0, cy + 2.0, 320.0, 30.0),
            &["Exclude listed", "Only listed"],
            sel,
        ) {
            ctx.cfg.exceptions.mode =
                if i == 0 { ExceptionMode::Exclude } else { ExceptionMode::Only };
            ctx.changed = true;
        }
        y = card.bottom() + 10.0;

        let (card, cy) = Self::row(
            ui,
            y,
            "Add an app",
            "Matched against WM_CLASS (class or instance, any case). \"xprop WM_CLASS\" will show it.",
            40.0,
            0.0,
        );
        let ev = widgets::text_field(
            ui,
            id("e.new"),
            R::new(X0 + 16.0, cy + 2.0, CW - 32.0 - 90.0, 30.0),
            &mut self.new_app,
            "e.g. Gimp",
        );
        let add = widgets::button(
            ui,
            id("e.add"),
            R::new(card.right() - 96.0, cy + 2.0, 80.0, 30.0),
            "Add",
            ButtonStyle::Primary,
        );
        if add || matches!(ev, FieldEvent::Submitted) {
            let name = self.new_app.text.trim().to_string();
            if !name.is_empty() && !ctx.cfg.exceptions.contains(&name) {
                ctx.cfg.exceptions.list_mut().push(name);
                ctx.changed = true;
            }
            self.new_app.set("");
        }
        y = card.bottom() + 10.0;

        let list = ctx.cfg.exceptions.list().clone();
        let lh = if list.is_empty() { 44.0 } else { 36.0 * list.len() as f32 + 12.0 };
        let card = R::new(X0, y, CW, 34.0 + lh);
        widgets::card(ui, card);
        ui.p.text(
            X0 + 16.0,
            y + 13.0,
            &format!("Listed apps ({})", list.len()),
            FontKind::Semi,
            13.5,
            TEXT,
        );
        if list.is_empty() {
            ui.p.text(X0 + 16.0, y + 40.0, "No apps listed yet.", FontKind::Medium, 12.0, FAINT);
        }
        let mut remove = None;
        for (i, name) in list.iter().enumerate() {
            let ry = y + 40.0 + i as f32 * 36.0;
            if i > 0 {
                ui.p.line(
                    X0 + 16.0,
                    ry - 2.0,
                    card.right() - 16.0,
                    ry - 2.0,
                    1.0,
                    Col(255, 255, 255, 14),
                );
            }
            ui.p.text_vc(X0 + 16.0, ry + 15.0, name, FontKind::Medium, 13.0, TEXT);
            let xr = R::new(card.right() - 44.0, ry + 2.0, 28.0, 26.0);
            let resp = ui.interact(idn("e.rm", i), xr);
            if resp.hovered {
                ui.p.rrect(xr, 8.0, Col(255, 255, 255, 18));
            }
            icons::draw(
                &mut ui.p,
                Icon::Close,
                xr.cx(),
                xr.cy(),
                11.0,
                if resp.hovered { TEXT } else { MUTED },
            );
            if resp.clicked {
                remove = Some(i);
            }
        }
        if let Some(i) = remove {
            ctx.cfg.exceptions.list_mut().remove(i);
            ctx.changed = true;
        }
        y = card.bottom() + 22.0;

        widgets::caption(ui, X0 + 4.0, y, "Running apps");
        if widgets::link(ui, id("e.refresh"), card.right() - 34.0, y + 6.0, "Refresh", false) {
            ctx.send(Cmd::RefreshClients);
        }
        y += 22.0;
        let mut x = X0;
        if ctx.st.clients.is_empty() {
            ui.p.text(X0 + 4.0, y + 4.0, "No other windows found.", FontKind::Medium, 12.0, FAINT);
            y += 28.0;
        }
        let clients = ctx.st.clients.clone();
        for (i, c) in clients.iter().enumerate() {
            let w = ui.p.text_w(c, FontKind::Semi, 11.5) + 26.0;
            if x + w > X0 + CW {
                x = X0;
                y += 36.0;
            }
            let listed = ctx.cfg.exceptions.contains(c);
            if widgets::chip(ui, idn("e.client", i), R::new(x, y, w, 28.0), c, listed) {
                ctx.cfg.exceptions.toggle(c);
                ctx.changed = true;
            }
            x += w + 8.0;
        }
        if !clients.is_empty() {
            y += 36.0;
        }
        if let Some(app) = &ctx.st.focused_app {
            ui.p.text(
                X0 + 4.0,
                y + 4.0,
                &format!("Last focused app: {app}"),
                FontKind::Medium,
                11.5,
                FAINT,
            );
            y += 22.0;
        }
        y
    }

    fn display(&mut self, ui: &mut Ui, ctx: &mut Ctx) -> f32 {
        let mut y = 0.0;
        ui.p.text(
            X0 + 2.0,
            y,
            "Choose which monitors the overlay covers.",
            FontKind::Medium,
            12.5,
            MUTED,
        );
        y += 28.0;
        let mons = ctx.st.monitors.clone();
        let mut toggle: Option<String> = None;
        if !mons.is_empty() {
            let x0 = mons.iter().map(|m| m.x as f32).fold(f32::MAX, f32::min);
            let y0 = mons.iter().map(|m| m.y as f32).fold(f32::MAX, f32::min);
            let x1 = mons.iter().map(|m| m.x as f32 + m.w as f32).fold(f32::MIN, f32::max);
            let y1 = mons.iter().map(|m| m.y as f32 + m.h as f32).fold(f32::MIN, f32::max);
            let (bw, bh) = (x1 - x0, y1 - y0);
            let area_h = 120.0;
            let k = ((CW - 40.0) / bw).min((area_h - 16.0) / bh);
            let ox = X0 + (CW - bw * k) * 0.5;
            let oy = y + (area_h - bh * k) * 0.5;
            ui.p.rrect(R::new(X0, y, CW, area_h), 12.0, Col(0, 0, 0, 40));
            for (i, m) in mons.iter().enumerate() {
                let r = R::new(
                    ox + (m.x as f32 - x0) * k + 2.0,
                    oy + (m.y as f32 - y0) * k + 2.0,
                    m.w as f32 * k - 4.0,
                    m.h as f32 * k - 4.0,
                );
                let on = ctx.cfg.output_enabled(&m.name);
                let resp = ui.interact(idn("d.box", i), r);
                ui.p.rrect(
                    r,
                    6.0,
                    if on {
                        TILE_ACTIVE
                    } else if resp.hovered {
                        TILE_HOVER
                    } else {
                        TILE
                    },
                );
                ui.p.rrect_stroke(r, 6.0, 1.2, if on { CREAM.fade(0.7) } else { TILE_EDGE });
                ui.p.text_center(
                    r,
                    &format!("{}", i + 1),
                    FontKind::Semi,
                    13.0,
                    if on { TEXT } else { FAINT },
                );
                if resp.clicked {
                    toggle = Some(m.name.clone());
                }
            }
            y += area_h + 12.0;
        }
        let grain = x11::grain_scales(ctx.cfg.grain_scale, ctx.cfg.match_grain, &mons);
        for (i, m) in mons.iter().enumerate() {
            let r = R::new(X0, y, CW, 44.0);
            widgets::card(ui, r);
            let b = R::new(r.x + 12.0, r.cy() - 11.0, 22.0, 22.0);
            ui.p.rrect(b, 6.0, Col(0, 0, 0, 70));
            ui.p.text_center(b, &format!("{}", i + 1), FontKind::Semi, 11.5, TEXT);
            let nw = ui.p.text_vc(r.x + 44.0, r.cy(), &m.name, FontKind::Semi, 13.0, TEXT);
            if m.primary {
                let c = R::new(r.x + 52.0 + nw, r.cy() - 8.5, 38.0, 17.0);
                ui.p.rrect(c, 5.0, Col(255, 255, 255, 30));
                ui.p.text_center(c, "Main", FontKind::Bold, 9.5, TEXT);
            }
            ui.p.text_right(
                r.right() - 70.0,
                r.cy(),
                &format!("{}×{} · grain {}×", m.w, m.h, fmt_scale(grain[i])),
                FontKind::Medium,
                11.5,
                MUTED,
            );
            let on = ctx.cfg.output_enabled(&m.name);
            if widgets::toggle(ui, idn("d.on", i), r.right() - 52.0, r.cy() - 10.0, on) {
                toggle = Some(m.name.clone());
            }
            y += 52.0;
        }
        y += 6.0;
        let (card, clicked) = Self::row_toggle(
            ui,
            y,
            "d.grain",
            "Same grain size on every monitor",
            "Scales the grain to each monitor's pixel density, taking the main monitor as the reference. Monitors that don't report their size are left as they are.",
            ctx.cfg.match_grain,
        );
        if clicked {
            ctx.cfg.match_grain = !ctx.cfg.match_grain;
            ctx.changed = true;
        }
        y = card.bottom() + 10.0;
        if let Some(name) = toggle {
            let list = &mut ctx.cfg.disabled_outputs;
            if let Some(i) = list.iter().position(|n| *n == name) {
                list.remove(i);
            } else {
                list.push(name);
            }
            ctx.changed = true;
        }
        let r = R::new(X0 + CW - 170.0, y, 170.0, 24.0);
        let resp = ui.interact(id("d.refresh"), r);
        let col = if resp.hovered { TEXT } else { MUTED };
        icons::draw(&mut ui.p, Icon::Refresh, r.x + 10.0, r.cy(), 11.0, col);
        ui.p.text_vc(r.x + 22.0, r.cy(), "Refresh display output", FontKind::Semi, 11.5, col);
        if resp.clicked {
            ctx.send(Cmd::RefreshDisplays);
        }
        y + 30.0
    }

    fn hotkeys(&mut self, ui: &mut Ui, ctx: &mut Ctx) -> f32 {
        let mut y = 0.0;
        let (card, clicked) = Self::row_toggle(
            ui,
            y,
            "h.on",
            "Global hotkeys",
            "Combos that i3 or another program already uses can't be grabbed; they show as \"In use\".",
            ctx.cfg.hotkeys.enabled,
        );
        if clicked {
            ctx.cfg.hotkeys.enabled = !ctx.cfg.hotkeys.enabled;
            ctx.changed = true;
            ctx.send(Cmd::RegrabHotkeys);
        }
        y = card.bottom() + 10.0;

        // While recording, the next non-modifier key press becomes the hotkey.
        if let Some(action) = self.recording {
            for k in ui.keys().to_vec() {
                let mods = k.mods & keys::RELEVANT_MODS;
                if mods == 0 && k.base == keys::sym::ESCAPE {
                    self.recording = None;
                } else if mods == 0
                    && (k.base == keys::sym::BACKSPACE || k.base == keys::sym::DELETE)
                {
                    ctx.cfg.hotkeys.set(action, String::new());
                    self.recording = None;
                } else if !keys::is_modifier(k.base)
                    && let Some(s) = keys::format_hotkey(keys::Hotkey { mods, keysym: k.base })
                {
                    ctx.cfg.hotkeys.set(action, s);
                    self.recording = None;
                } else {
                    continue;
                }
                ctx.changed = true;
                ctx.send(Cmd::RegrabHotkeys);
                break;
            }
        }

        let card = R::new(X0, y, CW, HotkeyAction::ALL.len() as f32 * 46.0 + 8.0);
        widgets::card(ui, card);
        for (i, a) in HotkeyAction::ALL.into_iter().enumerate() {
            let cy = y + 27.0 + i as f32 * 46.0;
            if i > 0 {
                ui.p.line(
                    X0 + 16.0,
                    cy - 23.0,
                    card.right() - 16.0,
                    cy - 23.0,
                    1.0,
                    Col(255, 255, 255, 14),
                );
            }
            ui.p.text_vc(X0 + 16.0, cy, a.label(), FontKind::Semi, 13.0, TEXT);
            let b = R::new(card.right() - 226.0, cy - 15.0, 210.0, 30.0);
            let rec = self.recording == Some(a);
            let resp = ui.interact(idn("h.key", i), b);
            ui.p.rrect(
                b,
                9.0,
                Col(
                    0,
                    0,
                    0,
                    if rec {
                        100
                    } else if resp.hovered {
                        85
                    } else {
                        65
                    },
                ),
            );
            ui.p.rrect_stroke(b, 9.0, 1.0, if rec { CREAM.fade(0.6) } else { TILE_EDGE });
            let label = if rec {
                "Press keys…".to_string()
            } else {
                keys::pretty(ctx.cfg.hotkeys.get(a))
            };
            let unset = ctx.cfg.hotkeys.get(a).is_empty();
            ui.p.text_center(
                b,
                &label,
                FontKind::Semi,
                12.0,
                if rec {
                    CREAM
                } else if unset {
                    FAINT
                } else {
                    TEXT
                },
            );
            if ctx.cfg.hotkeys.enabled && !unset && ctx.st.hotkeys_failed.contains(&a) {
                ui.p.text_right(b.x - 10.0, cy, "In use", FontKind::Semi, 11.0, DANGER);
            }
            if resp.clicked {
                self.recording = if rec { None } else { Some(a) };
            }
        }
        y = card.bottom() + 10.0;
        ui.p.text(
            X0 + 4.0,
            y,
            "While recording: Esc cancels, Backspace clears.",
            FontKind::Medium,
            11.0,
            FAINT,
        );
        y += 30.0;

        widgets::caption(ui, X0 + 4.0, y, "Or bind the CLI in your i3 config");
        y += 20.0;
        let lines = [
            "bindsym $mod+Mod1+p exec --no-startup-id paperlike toggle",
            "bindsym $mod+Mod1+t exec --no-startup-id paperlike next",
            "bindsym $mod+Mod1+o exec --no-startup-id paperlike lamp",
            "bindsym $mod+Mod1+backslash exec --no-startup-id paperlike snooze 5",
            "bindsym $mod+Mod1+bracketright exec --no-startup-id paperlike intensity +1",
        ];
        let r = R::new(X0, y, CW, lines.len() as f32 * 20.0 + 20.0);
        ui.p.rrect(r, 10.0, Col(0, 0, 0, 70));
        for (i, l) in lines.iter().enumerate() {
            ui.p.text(X0 + 14.0, y + 10.0 + i as f32 * 20.0, l, FontKind::Medium, 11.5, MUTED);
        }
        r.bottom()
    }

    fn about(&mut self, ui: &mut Ui, ctx: &mut Ctx) -> f32 {
        let mut y = 4.0;
        wordmark(ui, X0 + CW * 0.5, y, 48.0);
        y += 66.0;
        ui.p.text_center(
            R::new(X0, y, CW, 16.0),
            concat!("Version ", env!("CARGO_PKG_VERSION")),
            FontKind::Medium,
            12.0,
            FAINT,
        );
        y += 26.0;
        let h = ui.p.paragraph(
            X0 + 40.0,
            y,
            CW - 80.0,
            "Paper and fabric textures for your X11 desktop. They cut glare and harsh contrast without tinting your colours.",
            FontKind::Medium,
            12.5,
            MUTED,
        );
        y += h + 18.0;

        let st = ctx.st;
        let rows: Vec<(&str, String, bool)> = vec![
            (
                "Compositor",
                if st.compositor {
                    "running".into()
                } else {
                    "not running: start picom or another compositor".into()
                },
                st.compositor,
            ),
            (
                "Tray icon",
                if st.tray { "docked".into() } else { "no system tray found".into() },
                st.tray,
            ),
            ("Displays", format!("{}", st.monitors.len()), true),
            ("Config file", st.config_path.clone(), true),
        ];
        let card = R::new(X0, y, CW, rows.len() as f32 * 34.0 + 10.0);
        widgets::card(ui, card);
        for (i, (k, v, ok)) in rows.iter().enumerate() {
            let cy = y + 22.0 + i as f32 * 34.0;
            ui.p.text_vc(X0 + 16.0, cy, k, FontKind::Semi, 12.5, TEXT);
            let v = ui.p.ellipsize(v, FontKind::Medium, 12.0, CW - 170.0);
            ui.p.text_vc(
                X0 + 130.0,
                cy,
                &v,
                FontKind::Medium,
                12.0,
                if *ok { MUTED } else { WARN },
            );
        }
        y = card.bottom() + 22.0;
        widgets::caption(ui, X0 + 4.0, y, "Command line");
        y += 20.0;
        // HELP is formatted for a terminal ("  command   description", with
        // deeper-indented continuation lines); split it into two columns.
        let help: Vec<(String, String)> = crate::cmd::HELP
            .lines()
            .skip(2)
            .filter(|l| !l.trim().is_empty())
            .map(|l| {
                let t = l.trim();
                if l.starts_with("      ") {
                    return (String::new(), t.to_string());
                }
                match t.split_once("  ") {
                    Some((cmd, desc)) => (cmd.trim().to_string(), desc.trim().to_string()),
                    None => (t.to_string(), String::new()),
                }
            })
            .collect();
        let r = R::new(X0, y, CW, help.len() as f32 * 19.0 + 16.0);
        ui.p.rrect(r, 10.0, Col(0, 0, 0, 70));
        for (i, (cmd, desc)) in help.iter().enumerate() {
            let ty = y + 8.0 + i as f32 * 19.0;
            ui.p.text(X0 + 14.0, ty, cmd, FontKind::Semi, 11.0, TEXT);
            ui.p.text(X0 + 190.0, ty, desc, FontKind::Medium, 11.0, MUTED);
        }
        r.bottom()
    }
}

/// `1.5` as "1.5", `2.0` as "2", `1.125` as "1.13".
fn fmt_scale(v: f32) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}
