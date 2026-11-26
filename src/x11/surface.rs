//! Translucent windows that show software-rendered UI (control center, tray
//! menu, settings).

use x11rb::connection::Connection;
use x11rb::properties::{WmHints, WmSizeHints};
use x11rb::protocol::xproto::*;
use x11rb::wrapper::ConnectionExt as _;

use super::{Res, Xc};
use crate::ui::InputState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Override-redirect popup that grabs the pointer while open.
    Popup,
    /// Managed dialog window (i3 floats it).
    Dialog,
}

pub struct Surface {
    pub win: Window,
    gc: Gcontext,
    pub kind: Kind,
    pub scale: f32,
    /// Logical size.
    pub w: f32,
    pub h: f32,
    pub x: i16,
    pub y: i16,
    pub pixmap: tiny_skia::Pixmap,
    pub input: InputState,
    pub dirty: bool,
    buf: Vec<u8>,
}

fn dev(v: f32, s: f32) -> u16 {
    (v * s).round().max(1.0) as u16
}

impl Surface {
    fn create(xc: &Xc, kind: Kind, x: i16, y: i16, w: f32, h: f32, scale: f32) -> Res<Surface> {
        let (dw, dh) = (dev(w, scale), dev(h, scale));
        let win = xc.conn.generate_id()?;
        let mut aux = CreateWindowAux::new()
            .background_pixel(0)
            .border_pixel(0)
            .colormap(xc.argb_colormap)
            .event_mask(
                EventMask::EXPOSURE
                    | EventMask::BUTTON_PRESS
                    | EventMask::BUTTON_RELEASE
                    | EventMask::POINTER_MOTION
                    | EventMask::LEAVE_WINDOW
                    | EventMask::KEY_PRESS
                    | EventMask::STRUCTURE_NOTIFY
                    | EventMask::FOCUS_CHANGE,
            );
        if kind == Kind::Popup {
            aux = aux.override_redirect(1);
        }
        xc.conn.create_window(
            32,
            win,
            xc.root,
            x,
            y,
            dw,
            dh,
            0,
            WindowClass::INPUT_OUTPUT,
            xc.argb_visual,
            &aux,
        )?;
        let gc = xc.conn.generate_id()?;
        xc.conn.create_gc(gc, win, &CreateGCAux::new().graphics_exposures(0))?;
        xc.set_class(win, "paperlike", "Paperlike")?;
        let pixmap = tiny_skia::Pixmap::new(dw as u32, dh as u32).ok_or("bad surface size")?;
        Ok(Surface {
            win,
            gc,
            kind,
            scale,
            w,
            h,
            x,
            y,
            pixmap,
            input: InputState::default(),
            dirty: true,
            buf: Vec::new(),
        })
    }

    pub fn popup(xc: &Xc, x: i16, y: i16, w: f32, h: f32, scale: f32) -> Res<Surface> {
        let s = Self::create(xc, Kind::Popup, x, y, w, h, scale)?;
        xc.set_title(s.win, "Paperlike")?;
        xc.set_atoms(
            s.win,
            xc.atoms._NET_WM_WINDOW_TYPE,
            &[xc.atoms._NET_WM_WINDOW_TYPE_POPUP_MENU],
        )?;
        xc.conn.map_window(s.win)?;
        xc.conn.configure_window(s.win, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE))?;
        Ok(s)
    }

    pub fn dialog(
        xc: &Xc,
        x: i16,
        y: i16,
        w: f32,
        h: f32,
        scale: f32,
        title: &str,
    ) -> Res<Surface> {
        let s = Self::create(xc, Kind::Dialog, x, y, w, h, scale)?;
        let a = &xc.atoms;
        xc.set_title(s.win, title)?;
        xc.set_atoms(s.win, a.WM_PROTOCOLS, &[a.WM_DELETE_WINDOW])?;
        // A dialog type and fixed size make i3 float the window.
        xc.set_atoms(s.win, a._NET_WM_WINDOW_TYPE, &[a._NET_WM_WINDOW_TYPE_DIALOG])?;
        let (dw, dh) = (dev(w, scale) as i32, dev(h, scale) as i32);
        let hints = WmSizeHints {
            position: Some((
                x11rb::properties::WmSizeHintsSpecification::ProgramSpecified,
                x as i32,
                y as i32,
            )),
            min_size: Some((dw, dh)),
            max_size: Some((dw, dh)),
            ..Default::default()
        };
        hints.set_normal_hints(&xc.conn, s.win)?;
        let wm = WmHints { input: Some(true), ..Default::default() };
        wm.set(&xc.conn, s.win)?;
        // Ask for no WM decorations; the window draws its own rounded frame.
        xc.conn.change_property32(
            PropMode::REPLACE,
            s.win,
            a._MOTIF_WM_HINTS,
            a._MOTIF_WM_HINTS,
            &[2, 0, 0, 0, 0],
        )?;
        xc.conn.map_window(s.win)?;
        Ok(s)
    }

    /// Resizes/moves a popup (logical size, root position).
    pub fn reshape(&mut self, xc: &Xc, x: i16, y: i16, w: f32, h: f32) -> Res<()> {
        let (dw, dh) = (dev(w, self.scale), dev(h, self.scale));
        xc.conn.configure_window(
            self.win,
            &ConfigureWindowAux::new().x(x as i32).y(y as i32).width(dw as u32).height(dh as u32),
        )?;
        if dw as u32 != self.pixmap.width() || dh as u32 != self.pixmap.height() {
            self.pixmap = tiny_skia::Pixmap::new(dw as u32, dh as u32).ok_or("bad surface size")?;
        }
        self.x = x;
        self.y = y;
        self.w = w;
        self.h = h;
        self.dirty = true;
        Ok(())
    }

    pub fn grab(&self, xc: &Xc) {
        // Retry briefly: the key/button that opened us may still hold a grab.
        for _ in 0..20 {
            let ok = xc
                .conn
                .grab_pointer(
                    true,
                    self.win,
                    EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE | EventMask::POINTER_MOTION,
                    GrabMode::ASYNC,
                    GrabMode::ASYNC,
                    x11rb::NONE,
                    x11rb::NONE,
                    x11rb::CURRENT_TIME,
                )
                .ok()
                .and_then(|c| c.reply().ok())
                .is_some_and(|r| r.status == GrabStatus::SUCCESS);
            if ok {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        for _ in 0..20 {
            let ok = xc
                .conn
                .grab_keyboard(
                    true,
                    self.win,
                    x11rb::CURRENT_TIME,
                    GrabMode::ASYNC,
                    GrabMode::ASYNC,
                )
                .ok()
                .and_then(|c| c.reply().ok())
                .is_some_and(|r| r.status == GrabStatus::SUCCESS);
            if ok {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
    }

    /// Copies the rendered pixmap to the window.
    pub fn present(&mut self, xc: &Xc) -> Res<()> {
        let src = self.pixmap.data();
        self.buf.resize(src.len(), 0);
        for (d, s) in self.buf.as_chunks_mut::<4>().0.iter_mut().zip(src.as_chunks::<4>().0) {
            d[0] = s[2];
            d[1] = s[1];
            d[2] = s[0];
            d[3] = s[3];
        }
        let (w, h) = (self.pixmap.width() as u16, self.pixmap.height() as u16);
        xc.put_bgra(self.win, self.gc, 0, 0, w, h, &self.buf)?;
        Ok(())
    }

    pub fn destroy(&self, xc: &Xc) {
        if self.kind == Kind::Popup {
            let _ = xc.conn.ungrab_pointer(x11rb::CURRENT_TIME);
            let _ = xc.conn.ungrab_keyboard(x11rb::CURRENT_TIME);
        }
        let _ = xc.conn.free_gc(self.gc);
        let _ = xc.conn.destroy_window(self.win);
        let _ = xc.conn.flush();
    }

    /// Device pixels → logical coordinates.
    pub fn logical(&self, x: i16, y: i16) -> (f32, f32) {
        (x as f32 / self.scale, y as f32 / self.scale)
    }
}
