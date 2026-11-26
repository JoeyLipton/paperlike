//! Font loading, glyph caching and text blitting.

use std::collections::HashMap;

use fontdue::{Font, FontSettings, Metrics};

static MEDIUM: &[u8] = include_bytes!("../../assets/fonts/Manrope-500.ttf");
static SEMI: &[u8] = include_bytes!("../../assets/fonts/Manrope-600.ttf");
static BOLD: &[u8] = include_bytes!("../../assets/fonts/Manrope-700.ttf");
static BRAND: &[u8] = include_bytes!("../../assets/fonts/Chewy-Regular.ttf");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontKind {
    Medium = 0,
    Semi = 1,
    Bold = 2,
    /// Hand-drawn display face for the wordmark.
    Brand = 3,
}

struct Glyph {
    m: Metrics,
    cov: Vec<u8>,
}

pub struct Fonts {
    fonts: Vec<Font>,
    cache: HashMap<(FontKind, char, u32), Glyph>,
}

/// Integer clip rectangle in device pixels (x0, y0, x1, y1).
#[derive(Debug, Clone, Copy)]
pub struct Clip(pub i32, pub i32, pub i32, pub i32);

impl Fonts {
    pub fn new() -> Fonts {
        let load =
            |b: &'static [u8]| Font::from_bytes(b, FontSettings::default()).expect("embedded font");
        Fonts {
            fonts: vec![load(MEDIUM), load(SEMI), load(BOLD), load(BRAND)],
            cache: HashMap::new(),
        }
    }

    fn glyph(&mut self, f: FontKind, c: char, px: f32) -> &Glyph {
        let key = (f, c, (px * 4.0).round() as u32);
        let font = &self.fonts[f as usize];
        self.cache.entry(key).or_insert_with(|| {
            let (m, cov) = font.rasterize(c, px);
            Glyph { m, cov }
        })
    }

    pub fn ascent(&self, f: FontKind, px: f32) -> f32 {
        self.fonts[f as usize].horizontal_line_metrics(px).map_or(px * 0.8, |m| m.ascent)
    }

    pub fn descent(&self, f: FontKind, px: f32) -> f32 {
        self.fonts[f as usize].horizontal_line_metrics(px).map_or(px * -0.2, |m| m.descent)
    }

    fn kern(&self, f: FontKind, a: char, b: char, px: f32) -> f32 {
        self.fonts[f as usize].horizontal_kern(a, b, px).unwrap_or(0.0)
    }

    /// Advance width of `text` in device pixels.
    pub fn width(&mut self, f: FontKind, text: &str, px: f32) -> f32 {
        let mut w = 0.0;
        let mut prev = None;
        for c in text.chars() {
            if let Some(p) = prev {
                w += self.kern(f, p, c, px);
            }
            w += self.glyph(f, c, px).m.advance_width;
            prev = Some(c);
        }
        w
    }

    /// Draws `text` with its baseline at `y`. `rgba` is non-premultiplied.
    /// Returns the advance width.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        pm: &mut tiny_skia::Pixmap,
        clip: Clip,
        x: f32,
        y: f32,
        text: &str,
        f: FontKind,
        px: f32,
        rgba: [u8; 4],
    ) -> f32 {
        let (pw, ph) = (pm.width() as i32, pm.height() as i32);
        let cx0 = clip.0.max(0);
        let cy0 = clip.1.max(0);
        let cx1 = clip.2.min(pw);
        let cy1 = clip.3.min(ph);
        let mut pen = x;
        let mut prev = None;
        for c in text.chars() {
            if let Some(p) = prev {
                pen += self.kern(f, p, c, px);
            }
            prev = Some(c);
            let g = self.glyph(f, c, px);
            let gx = (pen + g.m.xmin as f32).round() as i32;
            let gy = (y - g.m.ymin as f32 - g.m.height as f32).round() as i32;
            let adv = g.m.advance_width;
            let (gw, gh) = (g.m.width as i32, g.m.height as i32);
            if gw > 0 && gh > 0 {
                let data = pm.data_mut();
                for row in 0..gh {
                    let py = gy + row;
                    if py < cy0 || py >= cy1 {
                        continue;
                    }
                    for col in 0..gw {
                        let tx = gx + col;
                        if tx < cx0 || tx >= cx1 {
                            continue;
                        }
                        let cov = g.cov[(row * gw + col) as usize] as u32;
                        if cov == 0 {
                            continue;
                        }
                        let sa = cov * rgba[3] as u32 / 255;
                        let i = ((py * pw + tx) * 4) as usize;
                        let inv = 255 - sa;
                        for k in 0..3 {
                            let s = rgba[k] as u32 * sa / 255;
                            data[i + k] = (s + data[i + k] as u32 * inv / 255) as u8;
                        }
                        data[i + 3] = (sa + data[i + 3] as u32 * inv / 255) as u8;
                    }
                }
            }
            pen += adv;
        }
        pen - x
    }
}

impl Default for Fonts {
    fn default() -> Self {
        Self::new()
    }
}
