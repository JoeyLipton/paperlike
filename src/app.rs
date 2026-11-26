//! The daemon: owns the X connection, configuration and runtime state, and
//! runs a single-threaded event loop over the X socket, the command socket and
//! a handful of timers.

use std::collections::{HashMap, VecDeque};
use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::randr::{self, ConnectionExt as _};
use x11rb::protocol::xproto::*;

use crate::cmd::{self, Cmd, Request};
use crate::config::{self, BatteryMode, Config, ExceptionMode, HotkeyAction, ThemeMode};
use crate::ipc;
use crate::power::{self, PowerStatus};
use crate::schedule;
use crate::texture::TextureId;
use crate::theme::SystemTheme;
use crate::ui::{self, Fonts, Key, Painter};
use crate::views::control_center::{self, ControlCenter};
use crate::views::settings::{self, Page, Settings};
use crate::views::{Ctx, Pause, Status, menu};
use crate::x11::keys::{self, Grabs, KeyMap};
use crate::x11::overlay::{self, Content, Overlay, TextureLayer};
use crate::x11::surface::Surface;
use crate::x11::tray::Tray;
use crate::x11::{Monitor, Res, Xc};

static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(_: libc::c_int) {
    STOP.store(true, Ordering::SeqCst);
}

/// A root-coordinate rectangle a popup is attached to.
#[derive(Debug, Clone, Copy)]
struct Anchor {
    x: i16,
    y: i16,
    w: u16,
    h: u16,
}

struct Popup<T> {
    s: Surface,
    view: T,
    anchor: Anchor,
}

pub struct App {
    xc: Xc,
    cfg: Config,
    fonts: Fonts,
    scale: f32,
    monitors: Vec<Monitor>,
    overlay: Overlay,
    tray: Tray,
    keymap: KeyMap,
    grabs: Grabs,
    ipc: ipc::Server,
    cc: Option<Popup<ControlCenter>>,
    menu: Option<Popup<()>>,
    settings: Option<(Surface, Settings)>,
    cmds: Vec<Cmd>,

    compositor: bool,
    snooze_until: Option<Instant>,
    /// (instance, class) of the focused window, ignoring our own windows.
    active: Option<(String, String)>,
    power: PowerStatus,
    theme: SystemTheme,
    screen_dark: HashMap<String, bool>,
    clients: Vec<String>,

    overlay_dirty: bool,
    tray_dirty: bool,
    restack_pending: bool,
    raises: VecDeque<Instant>,
    save_at: Option<Instant>,
    next_housekeeping: Instant,
    next_theme_poll: Instant,
    next_sample: Instant,
    next_second: Instant,
    running: bool,
}

const TRAY_SIZE: u16 = 22;

impl App {
    pub fn new(ipc: ipc::Server) -> Res<App> {
        let xc = Xc::connect()?;
        let cfg = Config::load();
        let scale = if cfg.ui_scale > 0.0 {
            cfg.ui_scale
        } else {
            xc.xft_dpi().map_or(1.0, |d| (d / 96.0).clamp(1.0, 3.0))
        };
        xc.conn.change_window_attributes(
            xc.root,
            &ChangeWindowAttributesAux::new().event_mask(
                EventMask::PROPERTY_CHANGE
                    | EventMask::SUBSTRUCTURE_NOTIFY
                    | EventMask::STRUCTURE_NOTIFY,
            ),
        )?;
        xc.conn.randr_select_input(
            xc.root,
            randr::NotifyMask::SCREEN_CHANGE
                | randr::NotifyMask::CRTC_CHANGE
                | randr::NotifyMask::OUTPUT_CHANGE,
        )?;
        xc.watch_selection(xc.cm_selection)?;
        xc.watch_selection(xc.tray_selection)?;
        let keymap = KeyMap::load(&xc)?;
        let now = Instant::now();
        let mut app = App {
            compositor: xc.compositor_running(),
            xc,
            cfg,
            fonts: Fonts::new(),
            scale,
            monitors: Vec::new(),
            overlay: Overlay::new(),
            tray: Tray::new(),
            keymap,
            grabs: Grabs::default(),
            ipc,
            cc: None,
            menu: None,
            settings: None,
            cmds: Vec::new(),
            snooze_until: None,
            active: None,
            power: power::read(),
            theme: SystemTheme::new(),
            screen_dark: HashMap::new(),
            clients: Vec::new(),
            overlay_dirty: true,
            tray_dirty: true,
            restack_pending: false,
            raises: VecDeque::new(),
            save_at: None,
            next_housekeeping: now,
            next_theme_poll: now,
            next_sample: now,
            next_second: now,
            running: true,
        };
        app.refresh_monitors()?;
        app.theme.poll();
        app.update_active();
        app.grabs.regrab(&app.xc, &app.keymap, &app.cfg.hotkeys);
        app.tray.refresh(&app.xc, (TRAY_SIZE as f32 * app.scale) as u16)?;
        if !app.compositor {
            eprintln!(
                "paperlike: no compositor is running; the overlay stays hidden until one (e.g. picom) starts"
            );
        }
        if !app.tray.docked() {
            eprintln!(
                "paperlike: no system tray found; use `paperlike panel` or a hotkey to open the control center"
            );
        }
        Ok(app)
    }

    pub fn run(&mut self) -> Res<()> {
        // SAFETY: installing a handler that only stores to an atomic.
        unsafe {
            libc::signal(libc::SIGINT, on_signal as *const () as libc::sighandler_t);
            libc::signal(libc::SIGTERM, on_signal as *const () as libc::sighandler_t);
            libc::signal(libc::SIGPIPE, libc::SIG_IGN);
        }
        let xfd = self.xc.conn.stream().as_raw_fd();
        let ifd = self.ipc.fd();
        while self.running && !STOP.load(Ordering::SeqCst) {
            self.drain()?;
            loop {
                self.process()?;
                self.xc.conn.flush()?;
                if !self.drain()? {
                    break;
                }
            }
            if !self.running {
                break;
            }
            let mut fds = [
                libc::pollfd { fd: xfd, events: libc::POLLIN, revents: 0 },
                libc::pollfd { fd: ifd, events: libc::POLLIN, revents: 0 },
            ];
            let timeout = self.timeout_ms();
            // SAFETY: `fds` is a valid array of two pollfd structs.
            unsafe { libc::poll(fds.as_mut_ptr(), 2, timeout) };
            if fds[0].revents & (libc::POLLHUP | libc::POLLERR) != 0 {
                return Err("lost the connection to the X server".into());
            }
            if fds[1].revents & libc::POLLIN != 0 {
                self.handle_ipc();
            }
        }
        self.shutdown();
        Ok(())
    }

    fn shutdown(&mut self) {
        if self.save_at.is_some() {
            self.save();
        }
        for s in [
            self.cc.take().map(|p| p.s),
            self.menu.take().map(|p| p.s),
            self.settings.take().map(|s| s.0),
        ]
        .into_iter()
        .flatten()
        {
            s.destroy(&self.xc);
        }
        self.overlay.destroy(&self.xc);
        self.tray.undock(&self.xc);
        let _ = self.xc.conn.flush();
    }

    fn drain(&mut self) -> Res<bool> {
        let mut any = false;
        while let Some(ev) = self.xc.conn.poll_for_event()? {
            self.handle_event(ev)?;
            any = true;
        }
        Ok(any)
    }

    fn process(&mut self) -> Res<()> {
        self.tick();
        for _ in 0..4 {
            for c in std::mem::take(&mut self.cmds) {
                self.exec(c);
            }
            self.render_surfaces()?;
            if self.cmds.is_empty() {
                break;
            }
        }
        if self.overlay_dirty {
            self.update_overlay()?;
        }
        if self.tray_dirty {
            self.draw_tray()?;
        }
        if self.restack_pending {
            self.restack();
        }
        Ok(())
    }

    // ---- timers -----------------------------------------------------------

    fn tick(&mut self) {
        let now = Instant::now();
        if let Some(t) = self.snooze_until
            && now >= t
        {
            self.snooze_until = None;
            self.state_changed();
        }
        if now >= self.next_housekeeping {
            self.power = power::read();
            // Schedules, sun position and battery are re-evaluated; the overlay
            // only repaints if the result actually differs.
            self.overlay_dirty = true;
            self.tray_dirty = true;
            self.next_housekeeping = now + Duration::from_secs(20);
        }
        if self.cfg.theme_mode == ThemeMode::System && now >= self.next_theme_poll {
            if self.theme.poll() {
                self.state_changed();
            }
            self.next_theme_poll = now + Duration::from_secs(4);
        }
        if self.cfg.theme_mode == ThemeMode::Screen && now >= self.next_sample {
            self.sample_screens();
            self.next_sample = now + Duration::from_millis(2500);
        }
        if self.snooze_until.is_some() && now >= self.next_second {
            self.mark_ui_dirty();
            self.next_second = now + Duration::from_secs(1);
        }
        if let Some(t) = self.save_at
            && now >= t
        {
            self.save();
        }
    }

    fn timeout_ms(&self) -> i32 {
        let now = Instant::now();
        let mut next = self.next_housekeeping;
        if self.cfg.theme_mode == ThemeMode::System {
            next = next.min(self.next_theme_poll);
        }
        if self.cfg.theme_mode == ThemeMode::Screen {
            next = next.min(self.next_sample);
        }
        if let Some(t) = self.snooze_until {
            next = next.min(t);
            if self.cc.is_some() || self.menu.is_some() || self.settings.is_some() {
                next = next.min(self.next_second);
            }
        }
        if let Some(t) = self.save_at {
            next = next.min(t);
        }
        next.saturating_duration_since(now).as_millis().min(60_000) as i32
    }

    fn save(&mut self) {
        self.save_at = None;
        if let Err(e) = self.cfg.save() {
            eprintln!("paperlike: could not save config: {e}");
        }
    }

    // ---- state ------------------------------------------------------------

    /// Something visible changed (not the config file).
    fn state_changed(&mut self) {
        self.overlay_dirty = true;
        self.tray_dirty = true;
        self.mark_ui_dirty();
    }

    /// The configuration changed: apply it and schedule a save.
    fn cfg_changed(&mut self) {
        self.state_changed();
        self.save_at = Some(Instant::now() + Duration::from_millis(700));
    }

    fn mark_ui_dirty(&mut self) {
        if let Some(p) = &mut self.cc {
            p.s.dirty = true;
        }
        if let Some(p) = &mut self.menu {
            p.s.dirty = true;
        }
        if let Some((s, _)) = &mut self.settings {
            s.dirty = true;
        }
    }

    fn primary(&self) -> Option<&Monitor> {
        self.monitors.iter().find(|m| m.primary).or(self.monitors.first())
    }

    fn dark_for(&self, m: Option<&Monitor>) -> bool {
        match self.cfg.theme_mode {
            ThemeMode::Light => false,
            ThemeMode::Dark => true,
            ThemeMode::System => self.theme.dark,
            ThemeMode::Screen => {
                m.and_then(|m| self.screen_dark.get(&m.name)).copied().unwrap_or(false)
            }
            ThemeMode::Sun => !schedule::is_daytime(&self.cfg.schedule, schedule::local_now()),
        }
    }

    fn dark_now(&self) -> bool {
        self.dark_for(self.primary())
    }

    fn snoozed(&self) -> bool {
        self.snooze_until.is_some_and(|t| t > Instant::now())
    }

    /// Why the overlay is hidden right now, apart from being switched off.
    fn paused_reason(&self) -> Option<Pause> {
        if !self.compositor {
            return Some(Pause::NoCompositor);
        }
        if self.snoozed() {
            return Some(Pause::Snoozed);
        }
        if !schedule::schedule_active(&self.cfg.schedule, schedule::local_now()) {
            return Some(Pause::Schedule);
        }
        let p = self.power;
        match self.cfg.battery.mode {
            BatteryMode::OnBattery if p.on_battery => return Some(Pause::OnBattery),
            BatteryMode::BelowPercent
                if p.on_battery && p.percent.is_some_and(|v| v < self.cfg.battery.threshold) =>
            {
                return Some(Pause::BatteryLow);
            }
            _ => {}
        }
        let ex = &self.cfg.exceptions;
        let listed = self.active.as_ref().is_some_and(|(i, c)| ex.contains(i) || ex.contains(c));
        match (ex.mode, &self.active) {
            (ExceptionMode::Exclude, Some((_, class))) if listed => {
                Some(Pause::Excluded(class.clone()))
            }
            (ExceptionMode::Only, _) if !listed => Some(Pause::NotListed),
            _ => None,
        }
    }

    fn desired(&self) -> Vec<Option<Content>> {
        let paused = self.paused_reason().is_some();
        self.monitors
            .iter()
            .map(|m| {
                if paused || !self.cfg.output_enabled(&m.name) {
                    return None;
                }
                let dark = self.dark_for(Some(m));
                let texture = self.cfg.enabled.then(|| {
                    let tex = self.cfg.texture(dark);
                    TextureLayer {
                        tex,
                        dark,
                        intensity: self.cfg.intensity(dark, tex),
                        scale: self.cfg.grain_scale,
                    }
                });
                let lamp_on = self.cfg.lamp.enabled && (!self.cfg.lamp.dark_only || dark);
                let lamp = lamp_on.then_some(self.cfg.lamp.params);
                (texture.is_some() || lamp.is_some()).then_some(Content { texture, lamp })
            })
            .collect()
    }

    fn update_overlay(&mut self) -> Res<()> {
        self.overlay_dirty = false;
        let desired = self.desired();
        self.overlay.update(&self.xc, &desired)?;
        self.restack_pending = true;
        Ok(())
    }

    fn refresh_monitors(&mut self) -> Res<()> {
        let mons = self.xc.monitors();
        if mons != self.monitors {
            self.monitors = mons;
            self.overlay.sync_monitors(&self.xc, &self.monitors)?;
            self.state_changed();
        }
        Ok(())
    }

    fn update_active(&mut self) {
        let class = self.xc.active_window().and_then(|w| self.xc.wm_class(w));
        if let Some((_, c)) = &class
            && c.eq_ignore_ascii_case("paperlike")
        {
            return;
        }
        if class != self.active {
            self.active = class;
            self.state_changed();
        }
    }

    fn sample_screens(&mut self) {
        let mut changed = false;
        for m in &self.monitors {
            let Some(lum) = self.xc.sample_brightness(m) else {
                continue;
            };
            let was = self.screen_dark.get(&m.name).copied().unwrap_or(false);
            // Hysteresis so a mixed screen doesn't flip back and forth.
            let now = if was { lum < 0.55 } else { lum < 0.40 };
            if now != was || !self.screen_dark.contains_key(&m.name) {
                self.screen_dark.insert(m.name.clone(), now);
                changed = true;
            }
        }
        if changed {
            self.state_changed();
        }
    }

    /// Keeps the overlay above other windows and our popups above the overlay.
    fn restack(&mut self) {
        self.restack_pending = false;
        let ours: Vec<Window> = self.overlay.mapped_windows().collect();
        let popups: Vec<Window> =
            self.cc.iter().map(|p| p.s.win).chain(self.menu.iter().map(|p| p.s.win)).collect();
        if ours.is_empty() && popups.is_empty() {
            return;
        }
        let Some(tree) = self.xc.conn.query_tree(self.xc.root).ok().and_then(|c| c.reply().ok())
        else {
            return;
        };
        let kids = tree.children;
        let lowest = kids.iter().position(|w| ours.contains(w)).unwrap_or(kids.len());
        let highest_overlay = kids.iter().rposition(|w| ours.contains(w));
        let foreign_above = kids[lowest..].iter().any(|&w| {
            !ours.contains(&w)
                && !popups.contains(&w)
                && self
                    .xc
                    .conn
                    .get_window_attributes(w)
                    .ok()
                    .and_then(|c| c.reply().ok())
                    .is_some_and(|a| a.map_state == MapState::VIEWABLE)
        });
        let popups_below = highest_overlay.is_some_and(|h| {
            popups.iter().any(|p| kids.iter().position(|w| w == p).is_some_and(|i| i < h))
        });
        if !foreign_above && !popups_below {
            return;
        }
        let now = Instant::now();
        while self.raises.front().is_some_and(|t| now.duration_since(*t) > Duration::from_secs(1)) {
            self.raises.pop_front();
        }
        if self.raises.len() >= 25 {
            return; // Someone else insists on being on top; don't fight forever.
        }
        self.raises.push_back(now);
        overlay::raise(&self.xc, ours.into_iter().chain(popups));
    }

    // ---- X events ---------------------------------------------------------

    fn is_ours(&self, w: Window) -> bool {
        self.overlay.owns(w)
            || self.tray.owns(w)
            || self.cc.as_ref().is_some_and(|p| p.s.win == w)
            || self.menu.as_ref().is_some_and(|p| p.s.win == w)
            || self.settings.as_ref().is_some_and(|s| s.0.win == w)
    }

    fn surface_mut(&mut self, w: Window) -> Option<&mut Surface> {
        if let Some(p) = self.cc.as_mut().filter(|p| p.s.win == w) {
            return Some(&mut p.s);
        }
        if let Some(p) = self.menu.as_mut().filter(|p| p.s.win == w) {
            return Some(&mut p.s);
        }
        self.settings.as_mut().filter(|s| s.0.win == w).map(|s| &mut s.0)
    }

    fn handle_event(&mut self, ev: Event) -> Res<()> {
        let root = self.xc.root;
        match ev {
            Event::Expose(e) => {
                if e.count > 0 {
                    return Ok(());
                }
                if self.overlay.owns(e.window) {
                    self.overlay.expose(&self.xc, e.window)?;
                } else if self.tray.owns(e.window) {
                    self.tray_dirty = true;
                } else if let Some(s) = self.surface_mut(e.window) {
                    s.dirty = true;
                }
            }
            Event::ConfigureNotify(e) => {
                if self.tray.owns(e.window) {
                    if (e.width, e.height) != (self.tray.w, self.tray.h) {
                        self.tray.w = e.width;
                        self.tray.h = e.height;
                        self.tray_dirty = true;
                    }
                } else if e.window == root {
                    self.refresh_monitors()?;
                } else if e.event == root && !self.is_ours(e.window) {
                    self.restack_pending = true;
                }
            }
            Event::MapNotify(e) => {
                if e.event == root && !self.is_ours(e.window) {
                    self.restack_pending = true;
                }
            }
            Event::ReparentNotify(e) => {
                if self.tray.owns(e.window) && e.parent == root {
                    self.tray.lost(&self.xc);
                }
            }
            Event::DestroyNotify(e) => {
                // Another client (e.g. `xkill`) may destroy our windows.
                if self.tray.owns(e.window) {
                    self.tray.lost(&self.xc);
                }
                if self.settings.as_ref().is_some_and(|s| s.0.win == e.window) {
                    self.cmds.push(Cmd::CloseSettings);
                }
                if self.cc.as_ref().is_some_and(|p| p.s.win == e.window) {
                    self.cmds.push(Cmd::ClosePanel);
                }
                if self.menu.as_ref().is_some_and(|p| p.s.win == e.window) {
                    self.cmds.push(Cmd::CloseMenu);
                }
            }
            Event::PropertyNotify(e) => {
                if e.window == root && e.atom == self.xc.atoms._NET_ACTIVE_WINDOW {
                    self.update_active();
                }
            }
            Event::ButtonPress(e) => {
                self.on_button(e.event, e.detail, e.event_x, e.event_y, e.root_x, e.root_y, true)
            }
            Event::ButtonRelease(e) => {
                self.on_button(e.event, e.detail, e.event_x, e.event_y, e.root_x, e.root_y, false)
            }
            Event::MotionNotify(e) => {
                if let Some(s) = self.surface_mut(e.event) {
                    let (x, y) = s.logical(e.event_x, e.event_y);
                    if s.input.motion(x, y) {
                        s.dirty = true;
                    }
                }
            }
            Event::LeaveNotify(e) => {
                if let Some(s) = self.surface_mut(e.event)
                    && s.input.leave()
                {
                    s.dirty = true;
                }
            }
            Event::KeyPress(e) => self.on_key(e),
            Event::ClientMessage(e) => {
                if self.settings.as_ref().is_some_and(|s| s.0.win == e.window)
                    && e.type_ == self.xc.atoms.WM_PROTOCOLS
                    && e.data.as_data32()[0] == self.xc.atoms.WM_DELETE_WINDOW
                {
                    self.cmds.push(Cmd::CloseSettings);
                }
            }
            Event::MappingNotify(_) => {
                self.keymap = KeyMap::load(&self.xc)?;
                self.grabs.regrab(&self.xc, &self.keymap, &self.cfg.hotkeys);
            }
            Event::XfixesSelectionNotify(e) => {
                if e.selection == self.xc.cm_selection {
                    self.compositor = self.xc.compositor_running();
                    // Update immediately: an ARGB overlay without a compositor
                    // would cover the screen.
                    self.state_changed();
                    self.update_overlay()?;
                } else if e.selection == self.xc.tray_selection {
                    self.tray.refresh(&self.xc, (TRAY_SIZE as f32 * self.scale) as u16)?;
                    self.tray_dirty = true;
                }
            }
            Event::RandrScreenChangeNotify(_) | Event::RandrNotify(_) => self.refresh_monitors()?,
            Event::Error(e) if std::env::var_os("PAPERLIKE_DEBUG").is_some() => {
                eprintln!("paperlike: X error {e:?}");
            }
            _ => {}
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn on_button(
        &mut self,
        win: Window,
        button: u8,
        ex: i16,
        ey: i16,
        rx: i16,
        ry: i16,
        press: bool,
    ) {
        if self.tray.owns(win) {
            if press {
                match button {
                    1 => self.cmds.push(Cmd::TogglePanel),
                    2 => self.cmds.push(Cmd::ToggleEffect),
                    3 => self.cmds.push(Cmd::OpenMenu { x: rx, y: ry }),
                    4 => self.cmds.push(Cmd::IntensityDelta(1)),
                    5 => self.cmds.push(Cmd::IntensityDelta(-1)),
                    _ => {}
                }
            }
            return;
        }
        if press {
            // A press anywhere but inside an open popup closes it.
            let outside = |s: &Surface| {
                win != s.win
                    || ex < 0
                    || ey < 0
                    || ex as f32 >= s.w * s.scale
                    || ey as f32 >= s.h * s.scale
            };
            if self.cc.as_ref().is_some_and(|p| outside(&p.s)) {
                self.cmds.push(Cmd::ClosePanel);
                if self.cc.as_ref().is_some_and(|p| p.s.win == win) {
                    return;
                }
            }
            if self.menu.as_ref().is_some_and(|p| outside(&p.s)) {
                self.cmds.push(Cmd::CloseMenu);
                if self.menu.as_ref().is_some_and(|p| p.s.win == win) {
                    return;
                }
            }
        }
        if let Some(s) = self.surface_mut(win) {
            let (x, y) = s.logical(ex, ey);
            let changed =
                if press { s.input.press(button, x, y) } else { s.input.release(button, x, y) };
            if changed {
                s.dirty = true;
            }
        }
    }

    fn on_key(&mut self, e: KeyPressEvent) {
        let state = u16::from(e.state);
        let (keysym, ch) = self.keymap.text_key(e.detail, state);
        let base = self.keymap.keysym(e.detail, false);
        let key = Key { keysym, base, ch, mods: state };
        let hotkey = self.grabs.lookup(e.detail, state);
        let recording = self.settings.as_ref().is_some_and(|s| s.1.recording.is_some());

        if recording && let Some((s, _)) = &mut self.settings {
            s.input.key(key);
            s.dirty = true;
            return;
        }
        let popup_open = self.cc.is_some() || self.menu.is_some();
        if popup_open && e.event != self.settings.as_ref().map_or(0, |s| s.0.win) {
            if base == keys::sym::ESCAPE || hotkey == Some(HotkeyAction::ControlCenter) {
                self.cmds.push(Cmd::ClosePanel);
                self.cmds.push(Cmd::CloseMenu);
            } else if let Some(a) = hotkey {
                self.hotkey(a);
            }
            return;
        }
        if e.event == self.xc.root {
            if let Some(a) = hotkey {
                self.hotkey(a);
            }
            return;
        }
        if let Some(s) = self.surface_mut(e.event) {
            s.input.key(key);
            s.dirty = true;
        }
    }

    fn hotkey(&mut self, a: HotkeyAction) {
        self.cmds.push(match a {
            HotkeyAction::ToggleEffect => Cmd::ToggleEffect,
            HotkeyAction::CycleTexture => Cmd::CycleTexture(1),
            HotkeyAction::ToggleLamp => Cmd::ToggleLamp,
            HotkeyAction::Snooze => Cmd::ToggleSnooze,
            HotkeyAction::IntensityUp => Cmd::IntensityDelta(1),
            HotkeyAction::IntensityDown => Cmd::IntensityDelta(-1),
            HotkeyAction::ControlCenter => Cmd::TogglePanel,
        });
    }

    // ---- commands ---------------------------------------------------------

    fn handle_ipc(&mut self) {
        for (stream, line) in self.ipc.accept() {
            let reply = match cmd::parse(&line) {
                Ok(Request::Run(c)) => {
                    let r = self.exec(c);
                    // Apply right away so scripts see the effect before we answer.
                    if let Err(e) = self.process() {
                        eprintln!("paperlike: {e}");
                    }
                    let _ = self.xc.conn.flush();
                    r
                }
                Ok(Request::Status { json }) => self.status_line(json),
                Ok(Request::Help) => cmd::HELP.to_string(),
                Err(e) => format!("error: {e}"),
            };
            ipc::reply(stream, &reply);
        }
    }

    /// Executes a command and returns a short human-readable result.
    fn exec(&mut self, c: Cmd) -> String {
        match self.try_exec(c) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("paperlike: {e}");
                format!("error: {e}")
            }
        }
    }

    fn try_exec(&mut self, c: Cmd) -> Res<String> {
        let now = Instant::now();
        Ok(match c {
            Cmd::ToggleEffect => {
                if self.snoozed() {
                    self.snooze_until = None;
                    self.cfg.enabled = true;
                } else {
                    self.cfg.enabled = !self.cfg.enabled;
                }
                self.cfg_changed();
                if self.cfg.enabled { "on" } else { "off" }.into()
            }
            Cmd::SetEffect(on) => {
                self.cfg.enabled = on;
                if on {
                    self.snooze_until = None;
                }
                self.cfg_changed();
                if on { "on" } else { "off" }.into()
            }
            Cmd::CycleTexture(d) => {
                let dark = self.dark_now();
                let i = (self.cfg.texture(dark).index() as i32 + d)
                    .rem_euclid(TextureId::ALL.len() as i32);
                let t = TextureId::ALL[i as usize];
                self.cfg.set_texture(dark, t);
                self.cfg.enabled = true;
                self.cfg_changed();
                t.name().into()
            }
            Cmd::SetTexture { tex, dark } => {
                let dark = dark.unwrap_or_else(|| self.dark_now());
                self.cfg.set_texture(dark, tex);
                if dark == self.dark_now() {
                    self.cfg.enabled = true;
                }
                self.cfg_changed();
                format!("{} ({})", tex.name(), if dark { "dark screens" } else { "light screens" })
            }
            Cmd::IntensityDelta(d) => {
                let dark = self.dark_now();
                let t = self.cfg.texture(dark);
                self.cfg.set_intensity(dark, t, self.cfg.intensity(dark, t) as i32 + d);
                self.cfg_changed();
                format!("{}%", self.cfg.intensity(dark, t))
            }
            Cmd::IntensitySet(v) => {
                let dark = self.dark_now();
                let t = self.cfg.texture(dark);
                self.cfg.set_intensity(dark, t, v as i32);
                self.cfg_changed();
                format!("{}%", self.cfg.intensity(dark, t))
            }
            Cmd::ToggleLamp => {
                self.cfg.lamp.enabled = !self.cfg.lamp.enabled;
                self.cfg_changed();
                format!("lamp {}", if self.cfg.lamp.enabled { "on" } else { "off" })
            }
            Cmd::SetLamp(on) => {
                self.cfg.lamp.enabled = on;
                self.cfg_changed();
                format!("lamp {}", if on { "on" } else { "off" })
            }
            Cmd::LampPreset(p) => {
                self.cfg.lamp.preset = p;
                self.cfg.lamp.params = p.params();
                self.cfg.lamp.enabled = true;
                self.cfg_changed();
                p.name().into()
            }
            Cmd::Snooze(min) => {
                self.snooze_until = Some(now + Duration::from_secs(min as u64 * 60));
                self.next_second = now;
                self.state_changed();
                format!("snoozed for {min} min")
            }
            Cmd::ToggleSnooze => {
                return self.try_exec(if self.snoozed() { Cmd::Unsnooze } else { Cmd::Snooze(5) });
            }
            Cmd::Unsnooze => {
                self.snooze_until = None;
                self.state_changed();
                "resumed".into()
            }
            Cmd::Theme(m) => {
                self.cfg.theme_mode = m;
                self.next_sample = now;
                self.next_theme_poll = now;
                self.cfg_changed();
                format!("theme follows {}", m.label().to_lowercase())
            }
            Cmd::CycleTheme => {
                let i = ThemeMode::ALL.iter().position(|&m| m == self.cfg.theme_mode).unwrap_or(0);
                return self.try_exec(Cmd::Theme(ThemeMode::ALL[(i + 1) % ThemeMode::ALL.len()]));
            }
            Cmd::TogglePanel => {
                if self.cc.is_some() {
                    self.close_panel();
                    "closed".into()
                } else {
                    self.open_panel()?;
                    "opened".into()
                }
            }
            Cmd::ClosePanel => {
                self.close_panel();
                String::new()
            }
            Cmd::OpenMenu { x, y } => {
                self.close_panel();
                self.close_menu();
                let h = menu::height(&self.cfg, &self.status());
                let anchor = Anchor { x, y, w: 1, h: 1 };
                let (px, py) = place(&self.monitors, self.scale, anchor, menu::WIDTH, h);
                let s = Surface::popup(&self.xc, px, py, menu::WIDTH, h, self.scale)?;
                s.grab(&self.xc);
                self.menu = Some(Popup { s, view: (), anchor });
                self.restack_pending = true;
                String::new()
            }
            Cmd::CloseMenu => {
                self.close_menu();
                String::new()
            }
            Cmd::OpenSettings(page) => {
                self.close_panel();
                self.close_menu();
                let page = page.unwrap_or(Page::General);
                match &mut self.settings {
                    Some((s, view)) => {
                        view.show(page, &mut self.cmds);
                        s.dirty = true;
                        self.xc.conn.configure_window(
                            s.win,
                            &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
                        )?;
                        self.xc.conn.set_input_focus(
                            InputFocus::PARENT,
                            s.win,
                            x11rb::CURRENT_TIME,
                        )?;
                    }
                    None => {
                        let m = self.primary().cloned().unwrap_or(Monitor {
                            name: String::new(),
                            x: 0,
                            y: 0,
                            w: 1280,
                            h: 800,
                            primary: true,
                        });
                        let (w, h) = (settings::WIDTH * self.scale, settings::HEIGHT * self.scale);
                        let x = m.x + ((m.w as f32 - w) / 2.0).max(0.0) as i16;
                        let y = m.y + ((m.h as f32 - h) / 2.0).max(0.0) as i16;
                        let s = Surface::dialog(
                            &self.xc,
                            x,
                            y,
                            settings::WIDTH,
                            settings::HEIGHT,
                            self.scale,
                            "Paperlike Settings",
                        )?;
                        let mut view = Settings::new(page);
                        view.show(page, &mut self.cmds);
                        self.clients = self.xc.client_classes();
                        self.settings = Some((s, view));
                    }
                }
                format!("settings: {}", page.label())
            }
            Cmd::CloseSettings => {
                if let Some((s, _)) = self.settings.take() {
                    s.destroy(&self.xc);
                }
                String::new()
            }
            Cmd::RefreshDisplays => {
                self.monitors.clear();
                self.refresh_monitors()?;
                format!("{} display(s)", self.monitors.len())
            }
            Cmd::RefreshClients => {
                self.clients = self.xc.client_classes();
                self.mark_ui_dirty();
                String::new()
            }
            Cmd::ToggleExclusion(class) => {
                let listed = self.cfg.exceptions.toggle(&class);
                self.cfg_changed();
                format!("{class} {}", if listed { "listed" } else { "unlisted" })
            }
            Cmd::SetAutostart(on) => {
                config::autostart::set(on)?;
                self.mark_ui_dirty();
                format!("autostart {}", if on { "on" } else { "off" })
            }
            Cmd::RegrabHotkeys => {
                self.grabs.regrab(&self.xc, &self.keymap, &self.cfg.hotkeys);
                self.mark_ui_dirty();
                String::new()
            }
            Cmd::ResetDefaults => {
                self.cfg = Config::default();
                self.grabs.regrab(&self.xc, &self.keymap, &self.cfg.hotkeys);
                self.cfg_changed();
                "settings reset".into()
            }
            Cmd::ReloadConfig => {
                self.cfg = Config::load();
                self.grabs.regrab(&self.xc, &self.keymap, &self.cfg.hotkeys);
                self.state_changed();
                "reloaded".into()
            }
            Cmd::Quit => {
                self.running = false;
                "bye".into()
            }
        })
    }

    fn open_panel(&mut self) -> Res<()> {
        self.close_menu();
        let anchor = match self.tray.root_rect(&self.xc) {
            Some((x, y, w, h)) => Anchor { x, y, w, h },
            None => {
                let (x, y) = self.xc.pointer();
                Anchor { x, y, w: 1, h: 1 }
            }
        };
        let view = ControlCenter::default();
        let h = view.height(self.compositor);
        let (x, y) = place(&self.monitors, self.scale, anchor, control_center::WIDTH, h);
        let s = Surface::popup(&self.xc, x, y, control_center::WIDTH, h, self.scale)?;
        s.grab(&self.xc);
        self.cc = Some(Popup { s, view, anchor });
        self.restack_pending = true;
        Ok(())
    }

    fn close_panel(&mut self) {
        if let Some(p) = self.cc.take() {
            p.s.destroy(&self.xc);
        }
    }

    fn close_menu(&mut self) {
        if let Some(p) = self.menu.take() {
            p.s.destroy(&self.xc);
        }
    }

    // ---- rendering --------------------------------------------------------

    fn status(&self) -> Status {
        let now = schedule::local_now();
        Status {
            compositor: self.compositor,
            tray: self.tray.docked(),
            snooze_left: self.snooze_until.map(|t| t.saturating_duration_since(Instant::now())),
            paused: self.paused_reason(),
            dark_now: self.dark_now(),
            system_dark: self.theme.dark,
            focused_app: self.active.as_ref().map(|a| a.1.clone()).filter(|c| !c.is_empty()),
            power: self.power,
            monitors: self.monitors.clone(),
            clients: self.clients.clone(),
            hotkeys_failed: self.grabs.failed.clone(),
            autostart: config::autostart::enabled(),
            sun: match (self.cfg.schedule.latitude, self.cfg.schedule.longitude) {
                (Some(a), Some(o)) => schedule::sun_times(a, o, now.yday, now.gmtoff_min),
                _ => None,
            },
            config_path: Config::path().display().to_string(),
        }
    }

    fn render_surfaces(&mut self) -> Res<()> {
        let dirty = self.cc.as_ref().is_some_and(|p| p.s.dirty)
            || self.menu.as_ref().is_some_and(|p| p.s.dirty)
            || self.settings.as_ref().is_some_and(|s| s.0.dirty);
        if !dirty {
            return Ok(());
        }
        let st = self.status();
        let mut cmds = Vec::new();
        let mut changed = false;

        if let Some(p) = &mut self.cc
            && p.s.dirty
        {
            let mut ctx = Ctx { cfg: &mut self.cfg, st: &st, cmds: &mut cmds, changed: false };
            let view = &mut p.view;
            ui::frame(&mut p.s.pixmap, &mut p.s.input, &mut self.fonts, p.s.scale, |ui| {
                view.view(ui, &mut ctx)
            });
            changed |= ctx.changed;
            let h = p.view.height(st.compositor);
            if (h - p.s.h).abs() > 0.5 {
                let (x, y) = place(&self.monitors, self.scale, p.anchor, control_center::WIDTH, h);
                p.s.reshape(&self.xc, x, y, control_center::WIDTH, h)?;
            } else {
                p.s.present(&self.xc)?;
                p.s.dirty = false;
            }
        }
        if let Some(p) = &mut self.menu
            && p.s.dirty
        {
            let mut ctx = Ctx { cfg: &mut self.cfg, st: &st, cmds: &mut cmds, changed: false };
            ui::frame(&mut p.s.pixmap, &mut p.s.input, &mut self.fonts, p.s.scale, |ui| {
                menu::view(ui, &mut ctx)
            });
            changed |= ctx.changed;
            p.s.present(&self.xc)?;
            p.s.dirty = false;
        }
        if let Some((s, view)) = &mut self.settings
            && s.dirty
        {
            let mut ctx = Ctx { cfg: &mut self.cfg, st: &st, cmds: &mut cmds, changed: false };
            ui::frame(&mut s.pixmap, &mut s.input, &mut self.fonts, s.scale, |ui| {
                view.view(ui, &mut ctx)
            });
            changed |= ctx.changed;
            s.present(&self.xc)?;
            s.dirty = false;
        }
        if changed {
            self.cfg_changed();
        }
        self.cmds.extend(cmds);
        Ok(())
    }

    fn draw_tray(&mut self) -> Res<()> {
        self.tray_dirty = false;
        let (w, h) = (self.tray.w as u32, self.tray.h as u32);
        if !self.tray.docked() || w == 0 || h == 0 {
            return Ok(());
        }
        let Some(mut pm) = tiny_skia::Pixmap::new(w, h) else {
            return Ok(());
        };
        let on = self.cfg.enabled && self.paused_reason().is_none();
        {
            let mut p = Painter::new(&mut pm, &mut self.fonts, 1.0);
            let size = w.min(h) as f32;
            let col = if on { ui::style::CREAM } else { ui::style::CREAM.fade(0.45) };
            let fs = size * 0.95;
            let tw = p.text_w("p", ui::FontKind::Brand, fs);
            p.text_vc(
                w as f32 * 0.5 - tw * 0.5,
                h as f32 * 0.5 - size * 0.08,
                "p",
                ui::FontKind::Brand,
                fs,
                col,
            );
            // A small warm dot shows the lamp is on.
            if self.cfg.lamp.enabled {
                let [r, g, b] = crate::lamp::color(self.cfg.lamp.params.warmth);
                let col = ui::Col(r, g, b, 255);
                p.circle(
                    w as f32 * 0.5 + size * 0.3,
                    h as f32 * 0.5 + size * 0.28,
                    size * 0.11,
                    col,
                );
            }
        }
        let mut bgra = pm.take();
        for px in bgra.as_chunks_mut::<4>().0 {
            px.swap(0, 2);
        }
        self.tray.present(&self.xc, &bgra)
    }

    fn status_line(&self, json: bool) -> String {
        let dark = self.dark_now();
        let tex = self.cfg.texture(dark);
        let intensity = self.cfg.intensity(dark, tex);
        let paused = self.paused_reason();
        let snooze =
            self.snooze_until.map(|t| t.saturating_duration_since(Instant::now()).as_secs());
        if json {
            let status = StatusJson {
                enabled: self.cfg.enabled,
                active: self.cfg.enabled && paused.is_none(),
                paused: paused.map(|p| p.to_string()),
                texture: tex.slug(),
                theme: if dark { "dark" } else { "light" },
                intensity,
                lamp: self.cfg.lamp.enabled,
                lamp_preset: self.cfg.lamp.preset.slug(),
                snooze_seconds: snooze.unwrap_or(0),
            };
            return serde_json::to_string(&status).unwrap_or_default();
        }
        let mut s = if let Some(left) = snooze {
            format!("snoozed {}:{:02}", left / 60, left % 60)
        } else if !self.cfg.enabled {
            "off".to_string()
        } else if let Some(p) = paused {
            format!("paused ({p})")
        } else {
            let theme = if dark { "dark" } else { "light" };
            format!("on · {} · {intensity}% · {theme}", tex.name())
        };
        if self.cfg.lamp.enabled {
            s.push_str(&format!(" · lamp: {}", self.cfg.lamp.preset.name()));
        }
        s
    }
}

/// `paperlike status --json`
#[derive(serde::Serialize)]
struct StatusJson {
    enabled: bool,
    active: bool,
    paused: Option<String>,
    texture: &'static str,
    theme: &'static str,
    intensity: u8,
    lamp: bool,
    lamp_preset: &'static str,
    snooze_seconds: u64,
}

/// Positions a popup of logical size `w`×`h` next to `anchor`, on the anchor's
/// monitor, opening upwards when the anchor is in the lower half.
fn place(monitors: &[Monitor], scale: f32, a: Anchor, w: f32, h: f32) -> (i16, i16) {
    let (dw, dh) = ((w * scale) as i32, (h * scale) as i32);
    let (ax, ay) = (a.x as i32 + a.w as i32 / 2, a.y as i32 + a.h as i32 / 2);
    let m = monitors
        .iter()
        .find(|m| {
            ax >= m.x as i32
                && ax < m.x as i32 + m.w as i32
                && ay >= m.y as i32
                && ay < m.y as i32 + m.h as i32
        })
        .or_else(|| monitors.iter().find(|m| m.primary))
        .or(monitors.first());
    let (mx, my, mw, mh) =
        m.map_or((0, 0, 1920, 1080), |m| (m.x as i32, m.y as i32, m.w as i32, m.h as i32));
    let margin = (8.0 * scale) as i32;
    let x = (ax - dw / 2).clamp(mx + margin, (mx + mw - dw - margin).max(mx + margin));
    let gap = (6.0 * scale) as i32;
    let y = if ay > my + mh / 2 { a.y as i32 - dh - gap } else { a.y as i32 + a.h as i32 + gap };
    let y = y.clamp(my + margin, (my + mh - dh - margin).max(my + margin));
    (x as i16, y as i16)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(x: i16, y: i16, w: u16, h: u16) -> Monitor {
        Monitor { name: String::new(), x, y, w, h, primary: false }
    }

    #[test]
    fn popup_opens_above_a_bottom_bar() {
        let mons = [monitor(0, 0, 1920, 1080)];
        let tray = Anchor { x: 1890, y: 1060, w: 20, h: 20 };
        let (x, y) = place(&mons, 1.0, tray, 300.0, 400.0);
        // Pushed left to stay on screen, and opening upwards.
        assert_eq!(x, 1920 - 300 - 8);
        assert_eq!(y, 1060 - 400 - 6);
    }

    #[test]
    fn popup_opens_below_a_top_bar_on_second_monitor() {
        let mons = [monitor(0, 0, 1920, 1080), monitor(1920, 0, 2560, 1440)];
        let tray = Anchor { x: 3000, y: 2, w: 20, h: 20 };
        let (x, y) = place(&mons, 2.0, tray, 300.0, 400.0);
        assert_eq!(x, 3010 - 300);
        assert_eq!(y, 22 + 12);
    }
}
