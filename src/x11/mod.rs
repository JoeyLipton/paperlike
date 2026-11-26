//! Thin helpers over an x11rb connection shared by the overlay, tray and UI.

pub mod keys;
pub mod overlay;
pub mod surface;
pub mod tray;

use std::error::Error;

use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::render::{self, ConnectionExt as _, PictType, Pictformat};
use x11rb::protocol::xfixes::ConnectionExt as _;
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

pub type Res<T> = Result<T, Box<dyn Error>>;

x11rb::atom_manager! {
    pub Atoms: AtomsCookie {
        WM_PROTOCOLS,
        WM_DELETE_WINDOW,
        UTF8_STRING,
        _NET_WM_NAME,
        _NET_WM_PID,
        _NET_WM_WINDOW_TYPE,
        _NET_WM_WINDOW_TYPE_DIALOG,
        _NET_WM_WINDOW_TYPE_NOTIFICATION,
        _NET_WM_WINDOW_TYPE_POPUP_MENU,
        _NET_WM_STATE,
        _NET_WM_STATE_ABOVE,
        _NET_WM_STATE_STICKY,
        _NET_WM_STATE_SKIP_TASKBAR,
        _NET_WM_STATE_SKIP_PAGER,
        _NET_ACTIVE_WINDOW,
        _NET_CLIENT_LIST,
        _NET_WM_BYPASS_COMPOSITOR,
        _COMPTON_SHADOW,
        _MOTIF_WM_HINTS,
        _NET_SYSTEM_TRAY_OPCODE,
        _NET_SYSTEM_TRAY_VISUAL,
        _XEMBED_INFO,
        RESOURCE_MANAGER,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Monitor {
    pub name: String,
    pub x: i16,
    pub y: i16,
    pub w: u16,
    pub h: u16,
    pub primary: bool,
}

pub struct Xc {
    pub conn: RustConnection,
    pub root: Window,
    pub root_visual: Visualid,
    pub root_depth: u8,
    pub atoms: Atoms,
    /// 32-bit TrueColor visual for translucent windows.
    pub argb_visual: Visualid,
    pub argb_colormap: Colormap,
    pub argb_format: Pictformat,
    pub formats: render::QueryPictFormatsReply,
    /// `_NET_WM_CM_Sn`: owned while a compositing manager runs.
    pub cm_selection: Atom,
    /// `_NET_SYSTEM_TRAY_Sn`: owned by the system tray (i3bar, polybar, …).
    pub tray_selection: Atom,
    pub image_lsb_first: bool,
    max_request_bytes: usize,
}

impl Xc {
    pub fn connect() -> Res<Xc> {
        let (conn, screen_num) = x11rb::connect(None)?;
        let screen = conn.setup().roots[screen_num].clone();
        let image_lsb_first = conn.setup().image_byte_order == ImageOrder::LSB_FIRST;
        let atoms = Atoms::new(&conn)?.reply()?;
        let cm_selection = intern(&conn, &format!("_NET_WM_CM_S{screen_num}"))?;
        let tray_selection = intern(&conn, &format!("_NET_SYSTEM_TRAY_S{screen_num}"))?;

        conn.xfixes_query_version(5, 0)?.reply()?;
        conn.render_query_version(0, 11)?.reply()?;
        conn.randr_query_version(1, 5)?.reply()?;

        let argb_visual = screen
            .allowed_depths
            .iter()
            .filter(|d| d.depth == 32)
            .flat_map(|d| d.visuals.iter())
            .find(|v| v.class == VisualClass::TRUE_COLOR)
            .map(|v| v.visual_id)
            .ok_or("the X server offers no 32-bit TrueColor visual")?;
        let argb_colormap = conn.generate_id()?;
        conn.create_colormap(ColormapAlloc::NONE, argb_colormap, screen.root, argb_visual)?;

        let formats = conn.render_query_pict_formats()?.reply()?;
        let argb_format = formats
            .formats
            .iter()
            .find(|f| {
                f.type_ == PictType::DIRECT
                    && f.depth == 32
                    && f.direct.alpha_mask == 0xff
                    && f.direct.alpha_shift == 24
                    && f.direct.red_shift == 16
            })
            .map(|f| f.id)
            .ok_or("no ARGB32 picture format")?;

        let max_request_bytes = conn.maximum_request_bytes();
        Ok(Xc {
            root: screen.root,
            root_visual: screen.root_visual,
            root_depth: screen.root_depth,
            conn,
            atoms,
            argb_visual,
            argb_colormap,
            argb_format,
            formats,
            cm_selection,
            tray_selection,
            image_lsb_first,
            max_request_bytes,
        })
    }

    /// Picture format matching a visual.
    pub fn visual_format(&self, visual: Visualid) -> Option<Pictformat> {
        self.formats
            .screens
            .iter()
            .flat_map(|s| s.depths.iter())
            .flat_map(|d| d.visuals.iter())
            .find(|v| v.visual == visual)
            .map(|v| v.format)
    }

    pub fn compositor_running(&self) -> bool {
        self.conn
            .get_selection_owner(self.cm_selection)
            .ok()
            .and_then(|c| c.reply().ok())
            .is_some_and(|r| r.owner != x11rb::NONE)
    }

    pub fn set_title(&self, win: Window, title: &str) -> Res<()> {
        self.conn.change_property8(
            PropMode::REPLACE,
            win,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            title.as_bytes(),
        )?;
        self.conn.change_property8(
            PropMode::REPLACE,
            win,
            self.atoms._NET_WM_NAME,
            self.atoms.UTF8_STRING,
            title.as_bytes(),
        )?;
        Ok(())
    }

    pub fn set_class(&self, win: Window, instance: &str, class: &str) -> Res<()> {
        let v = format!("{instance}\0{class}\0");
        self.conn.change_property8(
            PropMode::REPLACE,
            win,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            v.as_bytes(),
        )?;
        self.conn.change_property32(
            PropMode::REPLACE,
            win,
            self.atoms._NET_WM_PID,
            AtomEnum::CARDINAL,
            &[std::process::id()],
        )?;
        Ok(())
    }

    pub fn set_atoms(&self, win: Window, prop: Atom, values: &[Atom]) -> Res<()> {
        self.conn.change_property32(PropMode::REPLACE, win, prop, AtomEnum::ATOM, values)?;
        Ok(())
    }

    /// Uploads premultiplied BGRA pixels, split into requests the server accepts.
    #[allow(clippy::too_many_arguments)]
    pub fn put_bgra(
        &self,
        drawable: Drawable,
        gc: Gcontext,
        x: i16,
        y: i16,
        w: u16,
        h: u16,
        data: &[u8],
    ) -> Res<()> {
        let stride = w as usize * 4;
        if stride == 0 || h == 0 {
            return Ok(());
        }
        let rows_per = ((self.max_request_bytes.saturating_sub(64)) / stride).clamp(1, h as usize);
        let mut row = 0usize;
        while row < h as usize {
            let n = rows_per.min(h as usize - row);
            let chunk = &data[row * stride..(row + n) * stride];
            if self.image_lsb_first {
                self.conn.put_image(
                    ImageFormat::Z_PIXMAP,
                    drawable,
                    gc,
                    w,
                    n as u16,
                    x,
                    y + row as i16,
                    0,
                    32,
                    chunk,
                )?;
            } else {
                let swapped: Vec<u8> = chunk
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .flat_map(|p| [p[3], p[2], p[1], p[0]])
                    .collect();
                self.conn.put_image(
                    ImageFormat::Z_PIXMAP,
                    drawable,
                    gc,
                    w,
                    n as u16,
                    x,
                    y + row as i16,
                    0,
                    32,
                    &swapped,
                )?;
            }
            row += n;
        }
        Ok(())
    }

    pub fn get_window_prop(&self, win: Window, prop: Atom) -> Option<Window> {
        let r =
            self.conn.get_property(false, win, prop, AtomEnum::WINDOW, 0, 1).ok()?.reply().ok()?;
        r.value32().and_then(|mut v| v.next()).filter(|&w| w != 0)
    }

    pub fn active_window(&self) -> Option<Window> {
        self.get_window_prop(self.root, self.atoms._NET_ACTIVE_WINDOW)
    }

    /// `(instance, class)` from WM_CLASS.
    pub fn wm_class(&self, win: Window) -> Option<(String, String)> {
        let r = self
            .conn
            .get_property(false, win, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 256)
            .ok()?
            .reply()
            .ok()?;
        let mut parts = r.value.split(|&b| b == 0).map(|s| String::from_utf8_lossy(s).into_owned());
        let instance = parts.next().unwrap_or_default();
        let class = parts.next().unwrap_or_default();
        (!instance.is_empty() || !class.is_empty()).then_some((instance, class))
    }

    /// WM_CLASS of every managed client window, for exception suggestions.
    pub fn client_classes(&self) -> Vec<String> {
        let Some(r) = self
            .conn
            .get_property(false, self.root, self.atoms._NET_CLIENT_LIST, AtomEnum::WINDOW, 0, 4096)
            .ok()
            .and_then(|c| c.reply().ok())
        else {
            return Vec::new();
        };
        let mut out: Vec<String> = r
            .value32()
            .into_iter()
            .flatten()
            .filter_map(|w| self.wm_class(w))
            .map(|(_, class)| class)
            .filter(|c| !c.is_empty() && !c.eq_ignore_ascii_case("paperlike"))
            .collect();
        out.sort_by_key(|c| c.to_ascii_lowercase());
        out.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        out
    }

    /// `Xft.dpi` from the RESOURCE_MANAGER string, if set.
    pub fn xft_dpi(&self) -> Option<f32> {
        let r = self
            .conn
            .get_property(
                false,
                self.root,
                self.atoms.RESOURCE_MANAGER,
                AtomEnum::STRING,
                0,
                1 << 16,
            )
            .ok()?
            .reply()
            .ok()?;
        let text = String::from_utf8_lossy(&r.value);
        text.lines()
            .filter_map(|l| l.split_once(':'))
            .find(|(k, _)| k.trim() == "Xft.dpi")
            .and_then(|(_, v)| v.trim().parse::<f32>().ok())
    }

    pub fn monitors(&self) -> Vec<Monitor> {
        let Some(reply) =
            self.conn.randr_get_monitors(self.root, true).ok().and_then(|c| c.reply().ok())
        else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (i, m) in reply.monitors.iter().enumerate() {
            let name = self
                .conn
                .get_atom_name(m.name)
                .ok()
                .and_then(|c| c.reply().ok())
                .map(|r| String::from_utf8_lossy(&r.name).into_owned())
                .unwrap_or_else(|| format!("monitor-{i}"));
            out.push(Monitor { name, x: m.x, y: m.y, w: m.width, h: m.height, primary: m.primary });
        }
        if out.is_empty() {
            // No RandR monitors (e.g. Xvfb): treat the root window as one display.
            if let Ok(Ok(g)) = self.conn.get_geometry(self.root).map(|c| c.reply()) {
                out.push(Monitor {
                    name: "screen".into(),
                    x: 0,
                    y: 0,
                    w: g.width,
                    h: g.height,
                    primary: true,
                });
            }
        }
        out
    }

    pub fn pointer(&self) -> (i16, i16) {
        self.conn
            .query_pointer(self.root)
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|r| (r.root_x, r.root_y))
            .unwrap_or((0, 0))
    }

    /// Average luminance (0–1) of a monitor, sampled from a few scanlines.
    pub fn sample_brightness(&self, m: &Monitor) -> Option<f32> {
        if self.root_depth != 24 && self.root_depth != 32 {
            return None;
        }
        const ROWS: i16 = 12;
        let mut sum = 0.0f64;
        let mut n = 0u64;
        for r in 0..ROWS {
            let y = m.y + ((r as i32 * 2 + 1) * m.h as i32 / (ROWS as i32 * 2)) as i16;
            let img = self
                .conn
                .get_image(ImageFormat::Z_PIXMAP, self.root, m.x, y, m.w, 1, !0)
                .ok()?
                .reply()
                .ok()?;
            for px in img.data.as_chunks::<4>().0.iter().step_by(3) {
                let (b, g, r) = if self.image_lsb_first {
                    (px[0], px[1], px[2])
                } else {
                    (px[3], px[2], px[1])
                };
                sum += 0.2126 * r as f64 + 0.7152 * g as f64 + 0.0722 * b as f64;
                n += 1;
            }
        }
        (n > 0).then(|| (sum / n as f64 / 255.0) as f32)
    }

    pub fn watch_selection(&self, selection: Atom) -> Res<()> {
        use x11rb::protocol::xfixes::SelectionEventMask as M;
        self.conn.xfixes_select_selection_input(
            self.root,
            selection,
            M::SET_SELECTION_OWNER | M::SELECTION_WINDOW_DESTROY | M::SELECTION_CLIENT_CLOSE,
        )?;
        Ok(())
    }
}

fn intern(conn: &RustConnection, name: &str) -> Res<Atom> {
    Ok(conn.intern_atom(false, name.as_bytes())?.reply()?.atom)
}
