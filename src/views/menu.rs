//! Right-click menu of the tray icon.

use super::{Ctx, Status};
use crate::cmd::Cmd;
use crate::config::Config;
use crate::ui::icons::{self, Icon};
use crate::ui::style::*;
use crate::ui::{Col, FontKind, R, Ui, idn};

pub const WIDTH: f32 = 214.0;
const ITEM: f32 = 30.0;

enum Item {
    Action(Icon, String, Cmd),
    Separator,
}

fn items(cfg: &Config, st: &Status) -> Vec<Item> {
    use Item::*;
    let snoozed = st.snooze_left.is_some();
    vec![
        Action(
            Icon::Power,
            if cfg.enabled { "Turn effect off" } else { "Turn effect on" }.into(),
            Cmd::ToggleEffect,
        ),
        Action(
            Icon::Lamp,
            if cfg.lamp.enabled { "Turn Desk Lamp off" } else { "Turn Desk Lamp on" }.into(),
            Cmd::ToggleLamp,
        ),
        if snoozed {
            Action(Icon::Bell, "Resume now".into(), Cmd::Unsnooze)
        } else {
            Action(Icon::Bell, "Snooze for 5 minutes".into(), Cmd::Snooze(5))
        },
        Action(
            Icon::Texture(cfg.texture(st.dark_now)),
            "Next texture".into(),
            Cmd::CycleTexture(1),
        ),
        Separator,
        Action(Icon::Palette, "Control Center".into(), Cmd::TogglePanel),
        Action(Icon::Info, "Settings…".into(), Cmd::OpenSettings(None)),
        Separator,
        Action(Icon::Close, "Quit Paperlike".into(), Cmd::Quit),
    ]
}

pub fn height(cfg: &Config, st: &Status) -> f32 {
    12.0 + items(cfg, st)
        .iter()
        .map(|i| if matches!(i, Item::Separator) { 9.0 } else { ITEM })
        .sum::<f32>()
}

pub fn view(ui: &mut Ui, ctx: &mut Ctx) {
    let h = height(ctx.cfg, ctx.st);
    let panel = R::new(0.5, 0.5, WIDTH - 1.0, h - 1.0);
    ui.p.rrect_vgrad(panel, 12.0, PANEL_TOP, PANEL_BOTTOM);
    ui.p.rrect_stroke(panel, 12.0, 1.0, PANEL_EDGE);
    let mut y = 6.0;
    for (i, item) in items(ctx.cfg, ctx.st).into_iter().enumerate() {
        match item {
            Item::Separator => {
                ui.p.line(14.0, y + 4.5, WIDTH - 14.0, y + 4.5, 1.0, Col(255, 255, 255, 22));
                y += 9.0;
            }
            Item::Action(icon, label, cmd) => {
                let r = R::new(6.0, y, WIDTH - 12.0, ITEM);
                let resp = ui.interact(idn("menu", i), r);
                if resp.hovered {
                    ui.p.rrect(r, 8.0, Col(255, 255, 255, 26));
                }
                icons::draw(&mut ui.p, icon, r.x + 16.0, r.cy(), 13.0, CREAM);
                ui.p.text_vc(r.x + 34.0, r.cy(), &label, FontKind::Medium, 12.5, TEXT);
                if resp.clicked {
                    ctx.send(cmd);
                    ctx.send(Cmd::CloseMenu);
                }
                y += ITEM;
            }
        }
    }
}
