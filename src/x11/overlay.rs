//! The texture itself: one click-through, override-redirect ARGB window per
//! monitor, painted server-side with XRender.
//!
//! Each texture image is uploaded once per (texture, theme, size). Painting
//! is a single composite (tiled where needed) through a solid alpha mask that
//! carries the intensity, plus a radial gradient for the desk lamp. Nothing
//! is redrawn unless a setting changes or the compositor asks for an expose.

use x11rb::connection::Connection;
use x11rb::protocol::render::{
    self, ConnectionExt as _, CreatePictureAux, PictOp, Picture, Repeat,
};
use x11rb::protocol::shape;
use x11rb::protocol::xfixes::ConnectionExt as _;
use x11rb::protocol::xproto::*;
use x11rb::wrapper::ConnectionExt as _;

use super::{Monitor, Res, Xc};
use crate::lamp::{self, LampParams};
use crate::texture::{self, TextureId};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextureLayer {
    pub tex: TextureId,
    pub dark: bool,
    pub intensity: u8,
    pub scale: f32,
}

/// Everything one monitor's overlay shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Content {
    pub texture: Option<TextureLayer>,
    pub lamp: Option<LampParams>,
}

struct Screen {
    mon: Monitor,
    win: Window,
    pic: Picture,
    mapped: bool,
    painted: Option<Content>,
}

/// What a cached texture image depends on (not the intensity).
#[derive(Debug, Clone, Copy, PartialEq)]
struct TileKey {
    tex: TextureId,
    dark: bool,
    scale: f32,
    /// Monitor size, for textures that cover the screen.
    screen: Option<(u16, u16)>,
}

impl TileKey {
    fn new(layer: &TextureLayer, mon: &Monitor) -> TileKey {
        TileKey {
            tex: layer.tex,
            dark: layer.dark && layer.tex.themed(),
            scale: layer.scale,
            screen: layer.tex.screen_sized().then_some((mon.w, mon.h)),
        }
    }
}

struct Tile {
    key: TileKey,
    pixmap: Pixmap,
    pic: Picture,
    used: u64,
}

const MAX_TILES: usize = 4;

#[derive(Default)]
pub struct Overlay {
    screens: Vec<Screen>,
    tiles: Vec<Tile>,
    gc: Option<Gcontext>,
    clock: u64,
}

impl Overlay {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn owns(&self, win: Window) -> bool {
        self.screens.iter().any(|s| s.win == win)
    }

    pub fn mapped_windows(&self) -> impl Iterator<Item = Window> + '_ {
        self.screens.iter().filter(|s| s.mapped).map(|s| s.win)
    }

    /// Creates/destroys windows so there is exactly one per monitor.
    pub fn sync_monitors(&mut self, xc: &Xc, monitors: &[Monitor]) -> Res<()> {
        let mut keep = Vec::new();
        for s in self.screens.drain(..) {
            if monitors.contains(&s.mon) {
                keep.push(s);
            } else {
                let _ = xc.conn.render_free_picture(s.pic);
                let _ = xc.conn.destroy_window(s.win);
            }
        }
        let mut screens = Vec::new();
        for m in monitors {
            if let Some(i) = keep.iter().position(|s| &s.mon == m) {
                screens.push(keep.remove(i));
            } else {
                let (win, pic) = create_window(xc, m)?;
                screens.push(Screen { mon: m.clone(), win, pic, mapped: false, painted: None });
            }
        }
        self.screens = screens;
        Ok(())
    }

    /// Shows/hides/repaints each monitor. `desired` is indexed like the
    /// monitor list passed to [`sync_monitors`](Self::sync_monitors).
    pub fn update(&mut self, xc: &Xc, desired: &[Option<Content>]) -> Res<()> {
        for i in 0..self.screens.len() {
            let want = desired.get(i).copied().flatten();
            match want {
                None => {
                    let s = &mut self.screens[i];
                    if s.mapped {
                        xc.conn.unmap_window(s.win)?;
                        s.mapped = false;
                    }
                    s.painted = None;
                }
                Some(c) => {
                    if !self.screens[i].mapped {
                        let s = &mut self.screens[i];
                        xc.conn.map_window(s.win)?;
                        xc.conn.configure_window(
                            s.win,
                            &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
                        )?;
                        s.mapped = true;
                        s.painted = None;
                    }
                    if self.screens[i].painted != Some(c) {
                        self.paint(xc, i, &c)?;
                    }
                }
            }
        }
        Ok(())
    }

    pub fn expose(&mut self, xc: &Xc, win: Window) -> Res<()> {
        if let Some(i) = self.screens.iter().position(|s| s.win == win)
            && let Some(c) = self.screens[i].painted
        {
            self.paint(xc, i, &c)?;
        }
        Ok(())
    }

    pub fn destroy(&mut self, xc: &Xc) {
        for s in self.screens.drain(..) {
            let _ = xc.conn.render_free_picture(s.pic);
            let _ = xc.conn.destroy_window(s.win);
        }
        for t in self.tiles.drain(..) {
            let _ = xc.conn.render_free_picture(t.pic);
            let _ = xc.conn.free_pixmap(t.pixmap);
        }
        if let Some(gc) = self.gc.take() {
            let _ = xc.conn.free_gc(gc);
        }
    }

    fn paint(&mut self, xc: &Xc, i: usize, c: &Content) -> Res<()> {
        let mon = self.screens[i].mon.clone();
        let tile = match &c.texture {
            Some(layer) => Some(self.tile(xc, TileKey::new(layer, &mon))?),
            None => None,
        };
        let s = &mut self.screens[i];
        let (w, h) = (mon.w, mon.h);
        let full = [Rectangle { x: 0, y: 0, width: w, height: h }];
        let clear = render::Color { red: 0, green: 0, blue: 0, alpha: 0 };
        xc.conn.render_fill_rectangles(PictOp::SRC, s.pic, clear, &full)?;
        if let (Some(tile), Some(layer)) = (tile, &c.texture) {
            // The layer's opacity is the intensity, applied as a solid mask.
            let mask = solid_alpha(xc, layer.intensity as f32 / 100.0)?;
            xc.conn.render_composite(PictOp::OVER, tile, mask, s.pic, 0, 0, 0, 0, 0, 0, w, h)?;
            xc.conn.render_free_picture(mask)?;
        }
        if let Some(lp) = &c.lamp {
            paint_lamp(xc, s.pic, w, h, lp)?;
        }
        s.painted = Some(*c);
        Ok(())
    }

    /// Returns the picture for a texture image, rendering and uploading it if needed.
    fn tile(&mut self, xc: &Xc, key: TileKey) -> Res<Picture> {
        self.clock += 1;
        if let Some(t) = self.tiles.iter_mut().find(|t| t.key == key) {
            t.used = self.clock;
            return Ok(t.pic);
        }
        let screen = key.screen.map_or((1920, 1080), |(w, h)| (w as usize, h as usize));
        let img = texture::render(key.tex, key.dark, key.scale, screen);
        let (w, h) = (img.w as u16, img.h as u16);

        let pixmap = xc.conn.generate_id()?;
        xc.conn.create_pixmap(32, pixmap, xc.root, w, h)?;
        let gc = match self.gc {
            Some(gc) => gc,
            None => {
                let gc = xc.conn.generate_id()?;
                xc.conn.create_gc(gc, pixmap, &CreateGCAux::new().graphics_exposures(0))?;
                self.gc = Some(gc);
                gc
            }
        };
        xc.put_bgra(pixmap, gc, 0, 0, w, h, &img.bgra)?;
        let repeat = if img.repeat { Repeat::NORMAL } else { Repeat::NONE };
        drop(img);
        release_scratch_memory();
        let pic = xc.conn.generate_id()?;
        xc.conn.render_create_picture(
            pic,
            pixmap,
            xc.argb_format,
            &CreatePictureAux::new().repeat(repeat),
        )?;

        if self.tiles.len() >= MAX_TILES
            && let Some(i) =
                self.tiles.iter().enumerate().min_by_key(|(_, t)| t.used).map(|(i, _)| i)
        {
            let old = self.tiles.remove(i);
            xc.conn.render_free_picture(old.pic)?;
            xc.conn.free_pixmap(old.pixmap)?;
        }
        self.tiles.push(Tile { key, pixmap, pic, used: self.clock });
        Ok(pic)
    }
}

/// Rendering a texture goes through tens of megabytes of scratch buffers.
/// glibc keeps freed heap memory for reuse, so without this the daemon could
/// sit on that memory until it exits.
fn release_scratch_memory() {
    // SAFETY: malloc_trim only returns memory that is already free.
    #[cfg(target_env = "gnu")]
    unsafe {
        libc::malloc_trim(0);
    }
}

fn create_window(xc: &Xc, m: &Monitor) -> Res<(Window, Picture)> {
    let win = xc.conn.generate_id()?;
    let aux = CreateWindowAux::new()
        .background_pixel(0)
        .border_pixel(0)
        .override_redirect(1)
        .colormap(xc.argb_colormap)
        .event_mask(EventMask::EXPOSURE);
    xc.conn.create_window(
        32,
        win,
        xc.root,
        m.x,
        m.y,
        m.w,
        m.h,
        0,
        WindowClass::INPUT_OUTPUT,
        xc.argb_visual,
        &aux,
    )?;

    // An empty input shape makes every click and scroll pass straight through.
    let region = xc.conn.generate_id()?;
    xc.conn.xfixes_create_region(region, &[])?;
    xc.conn.xfixes_set_window_shape_region(win, shape::SK::INPUT, 0, 0, region)?;
    xc.conn.xfixes_destroy_region(region)?;

    xc.set_class(win, "paperlike-overlay", "Paperlike")?;
    xc.set_title(win, "Paperlike overlay")?;
    let a = &xc.atoms;
    xc.set_atoms(
        win,
        a._NET_WM_STATE,
        &[
            a._NET_WM_STATE_ABOVE,
            a._NET_WM_STATE_STICKY,
            a._NET_WM_STATE_SKIP_TASKBAR,
            a._NET_WM_STATE_SKIP_PAGER,
        ],
    )?;
    // Ask compositors not to shadow the overlay and never to unredirect it.
    xc.conn.change_property32(
        PropMode::REPLACE,
        win,
        a._COMPTON_SHADOW,
        AtomEnum::CARDINAL,
        &[0],
    )?;
    xc.conn.change_property32(
        PropMode::REPLACE,
        win,
        a._NET_WM_BYPASS_COMPOSITOR,
        AtomEnum::CARDINAL,
        &[2],
    )?;

    let pic = xc.conn.generate_id()?;
    xc.conn.render_create_picture(pic, win, xc.argb_format, &CreatePictureAux::new())?;
    Ok((win, pic))
}

fn fixed(v: f32) -> render::Fixed {
    (v * 65536.0) as render::Fixed
}

fn c16(v: f32) -> u16 {
    (v.clamp(0.0, 1.0) * 65535.0 + 0.5) as u16
}

/// A 1×1 repeating picture of alpha `a`, used as a mask.
fn solid_alpha(xc: &Xc, a: f32) -> Res<Picture> {
    let pic = xc.conn.generate_id()?;
    xc.conn.render_create_solid_fill(
        pic,
        render::Color { red: 0, green: 0, blue: 0, alpha: c16(a) },
    )?;
    Ok(pic)
}

/// Paints the lamp's elliptical gradient at its "dimming" opacity.
fn paint_lamp(xc: &Xc, dst: Picture, w: u16, h: u16, p: &LampParams) -> Res<()> {
    let g = lamp::gradient(p, w as f32, h as f32);
    // XRender gradients are circular: draw a circle of radius rx in a space
    // where y is stretched by rx/ry, and map the screen into it.
    let k = g.rx / g.ry;
    let pic = xc.conn.generate_id()?;
    let centre = render::Pointfix { x: fixed(g.cx), y: fixed(g.cy * k) };
    let pos: Vec<render::Fixed> = g.stops.iter().map(|s| fixed(s.0)).collect();
    // Gradient stop colours are *not* premultiplied.
    let colors: Vec<render::Color> = g
        .stops
        .iter()
        .map(|s| render::Color {
            red: c16(s.1[0]),
            green: c16(s.1[1]),
            blue: c16(s.1[2]),
            alpha: c16(s.1[3]),
        })
        .collect();
    xc.conn.render_create_radial_gradient(
        pic,
        centre,
        centre,
        0,
        fixed(g.rx.max(1.0)),
        &pos,
        &colors,
    )?;
    xc.conn.render_change_picture(pic, &render::ChangePictureAux::new().repeat(Repeat::PAD))?;
    let one = fixed(1.0);
    xc.conn.render_set_picture_transform(
        pic,
        render::Transform {
            matrix11: one,
            matrix12: 0,
            matrix13: 0,
            matrix21: 0,
            matrix22: fixed(k),
            matrix23: 0,
            matrix31: 0,
            matrix32: 0,
            matrix33: one,
        },
    )?;
    let mask = solid_alpha(xc, g.opacity)?;
    xc.conn.render_composite(PictOp::OVER, pic, mask, dst, 0, 0, 0, 0, 0, 0, w, h)?;
    xc.conn.render_free_picture(mask)?;
    xc.conn.render_free_picture(pic)?;
    Ok(())
}

/// Raises the given windows (in order) to the top of the stack.
pub fn raise(xc: &Xc, wins: impl IntoIterator<Item = Window>) {
    for w in wins {
        let _ =
            xc.conn.configure_window(w, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE));
    }
    let _ = xc.conn.flush();
}
