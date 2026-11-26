//! XEmbed system tray icon (works with i3bar, polybar, and other XEmbed trays).

use x11rb::connection::Connection;
use x11rb::protocol::render::{ConnectionExt as _, CreatePictureAux, PictOp};
use x11rb::protocol::xproto::*;
use x11rb::wrapper::ConnectionExt as _;

use super::{Res, Xc};

const SYSTEM_TRAY_REQUEST_DOCK: u32 = 0;
const XEMBED_MAPPED: u32 = 1;

#[derive(Default)]
pub struct Tray {
    pub win: Option<Window>,
    manager: Option<Window>,
    argb: bool,
    gc: Option<Gcontext>,
    pub w: u16,
    pub h: u16,
}

impl Tray {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn owns(&self, w: Window) -> bool {
        self.win == Some(w)
    }

    pub fn docked(&self) -> bool {
        self.win.is_some()
    }

    /// Docks into the current tray, if any. Safe to call repeatedly; it
    /// re-docks when the tray owner changed (e.g. i3bar restarted).
    pub fn refresh(&mut self, xc: &Xc, size: u16) -> Res<()> {
        let owner = xc.conn.get_selection_owner(xc.tray_selection)?.reply()?.owner;
        let owner = (owner != x11rb::NONE).then_some(owner);
        if owner == self.manager && self.win.is_some() {
            return Ok(());
        }
        self.undock(xc);
        self.manager = owner;
        let Some(manager) = owner else { return Ok(()) };

        // Use the visual the tray asks for (ARGB on transparent bars).
        let tray_visual = xc
            .conn
            .get_property(
                false,
                manager,
                xc.atoms._NET_SYSTEM_TRAY_VISUAL,
                AtomEnum::VISUALID,
                0,
                1,
            )?
            .reply()
            .ok()
            .and_then(|r| r.value32().and_then(|mut v| v.next()));
        self.argb = tray_visual == Some(xc.argb_visual);

        let win = xc.conn.generate_id()?;
        let events = EventMask::EXPOSURE
            | EventMask::STRUCTURE_NOTIFY
            | EventMask::BUTTON_PRESS
            | EventMask::BUTTON_RELEASE;
        if self.argb {
            let aux = CreateWindowAux::new()
                .background_pixel(0)
                .border_pixel(0)
                .colormap(xc.argb_colormap)
                .event_mask(events);
            xc.conn.create_window(
                32,
                win,
                xc.root,
                0,
                0,
                size,
                size,
                0,
                WindowClass::INPUT_OUTPUT,
                xc.argb_visual,
                &aux,
            )?;
        } else {
            let aux = CreateWindowAux::new()
                .background_pixmap(BackPixmap::PARENT_RELATIVE)
                .event_mask(events);
            xc.conn.create_window(
                x11rb::COPY_DEPTH_FROM_PARENT,
                win,
                xc.root,
                0,
                0,
                size,
                size,
                0,
                WindowClass::INPUT_OUTPUT,
                x11rb::COPY_FROM_PARENT,
                &aux,
            )?;
        }
        xc.set_class(win, "paperlike-tray", "Paperlike")?;
        xc.set_title(win, "Paperlike")?;
        xc.conn.change_property32(
            PropMode::REPLACE,
            win,
            xc.atoms._XEMBED_INFO,
            xc.atoms._XEMBED_INFO,
            &[0, XEMBED_MAPPED],
        )?;

        let msg = ClientMessageEvent::new(
            32,
            manager,
            xc.atoms._NET_SYSTEM_TRAY_OPCODE,
            [x11rb::CURRENT_TIME, SYSTEM_TRAY_REQUEST_DOCK, win, 0, 0],
        );
        xc.conn.send_event(false, manager, EventMask::NO_EVENT, msg)?;
        xc.conn.flush()?;
        self.win = Some(win);
        self.w = size;
        self.h = size;
        Ok(())
    }

    pub fn undock(&mut self, xc: &Xc) {
        if let Some(gc) = self.gc.take() {
            let _ = xc.conn.free_gc(gc);
        }
        if let Some(w) = self.win.take() {
            let _ = xc.conn.destroy_window(w);
        }
        self.manager = None;
    }

    /// The tray went away (our window got destroyed or reparented to root).
    pub fn lost(&mut self, xc: &Xc) {
        self.undock(xc);
    }

    /// Paints premultiplied BGRA pixels of size `w`×`h` onto the icon.
    pub fn present(&mut self, xc: &Xc, bgra: &[u8]) -> Res<()> {
        let Some(win) = self.win else { return Ok(()) };
        let (w, h) = (self.w, self.h);
        if w == 0 || h == 0 {
            return Ok(());
        }
        if self.argb {
            let gc = match self.gc {
                Some(gc) => gc,
                None => {
                    let gc = xc.conn.generate_id()?;
                    xc.conn.create_gc(gc, win, &CreateGCAux::new())?;
                    self.gc = Some(gc);
                    gc
                }
            };
            xc.put_bgra(win, gc, 0, 0, w, h, bgra)?;
            return Ok(());
        }
        // Opaque tray: let the server paint the bar background (ParentRelative),
        // then alpha-composite the icon over it with XRender.
        xc.conn.clear_area(false, win, 0, 0, 0, 0)?;
        let Some(fmt) = xc.visual_format(xc.root_visual) else {
            return Ok(());
        };
        let pm = xc.conn.generate_id()?;
        xc.conn.create_pixmap(32, pm, xc.root, w, h)?;
        let gc = xc.conn.generate_id()?;
        xc.conn.create_gc(gc, pm, &CreateGCAux::new())?;
        xc.put_bgra(pm, gc, 0, 0, w, h, bgra)?;
        let src = xc.conn.generate_id()?;
        xc.conn.render_create_picture(src, pm, xc.argb_format, &CreatePictureAux::new())?;
        let dst = xc.conn.generate_id()?;
        xc.conn.render_create_picture(dst, win, fmt, &CreatePictureAux::new())?;
        xc.conn.render_composite(PictOp::OVER, src, x11rb::NONE, dst, 0, 0, 0, 0, 0, 0, w, h)?;
        xc.conn.render_free_picture(src)?;
        xc.conn.render_free_picture(dst)?;
        xc.conn.free_gc(gc)?;
        xc.conn.free_pixmap(pm)?;
        Ok(())
    }

    /// Icon rectangle in root coordinates.
    pub fn root_rect(&self, xc: &Xc) -> Option<(i16, i16, u16, u16)> {
        let win = self.win?;
        let t = xc.conn.translate_coordinates(win, xc.root, 0, 0).ok()?.reply().ok()?;
        Some((t.dst_x, t.dst_y, self.w, self.h))
    }
}
