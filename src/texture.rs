//! The textures.
//!
//! Every texture is built from the SVG-style filter primitives in
//! [`crate::svgfx`]: Perlin turbulence, a little colour math, and for the
//! embossed ones a distant-light relief pass. [`render`] produces the texture
//! at full opacity, either as a tile that the X server repeats or as an image
//! covering one screen. The overlay applies the intensity on top as an alpha
//! mask, so moving the slider never re-renders anything.
//!
//! Sizes below are in "CSS pixels": multiply by the grain scale to get device
//! pixels. Noise frequencies are per *unit*, where a tile spans a fixed number
//! of units regardless of its pixel size.

use serde::{Deserialize, Serialize};

use crate::svgfx::{
    Canvas, Grid, Perlin, Turbulence, desaturate, diffuse, hex_linear, hex_srgb, linear_to_srgb, q8,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextureId {
    Matte,
    Crosshatch,
    Parchment,
    Linen,
    ColdPress,
    RicePaper,
    Frost,
    Felt,
    Graphite,
    CoarseMatte,
}

impl TextureId {
    pub const ALL: [TextureId; 10] = [
        TextureId::Matte,
        TextureId::Crosshatch,
        TextureId::Parchment,
        TextureId::Linen,
        TextureId::ColdPress,
        TextureId::RicePaper,
        TextureId::Frost,
        TextureId::Felt,
        TextureId::Graphite,
        TextureId::CoarseMatte,
    ];

    pub fn slug(self) -> &'static str {
        match self {
            TextureId::Matte => "matte",
            TextureId::Crosshatch => "crosshatch",
            TextureId::Parchment => "parchment",
            TextureId::Linen => "linen",
            TextureId::ColdPress => "cold-press",
            TextureId::RicePaper => "rice-paper",
            TextureId::Frost => "frost",
            TextureId::Felt => "felt",
            TextureId::Graphite => "graphite",
            TextureId::CoarseMatte => "coarse-matte",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            TextureId::Matte => "Matte",
            TextureId::Crosshatch => "Crosshatch",
            TextureId::Parchment => "Parchment",
            TextureId::Linen => "Linen",
            TextureId::ColdPress => "Cold Press",
            TextureId::RicePaper => "Rice Paper",
            TextureId::Frost => "Frost",
            TextureId::Felt => "Felt",
            TextureId::Graphite => "Graphite",
            TextureId::CoarseMatte => "Coarse Matte",
        }
    }

    /// The name split over two lines for small tiles (second line may be empty).
    pub fn lines(self) -> (&'static str, &'static str) {
        let n = self.name();
        match n.split_once(' ') {
            Some((a, b)) => (a, b),
            None => (n, ""),
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            TextureId::Matte => "Fine, even grain. A good default for reading and writing.",
            TextureId::Crosshatch => "Faint diagonal threads over a light grain.",
            TextureId::Parchment => "Warm amber paper with soft fibres and mottling.",
            TextureId::Linen => "Woven linen with long horizontal slubs.",
            TextureId::ColdPress => "Embossed watercolour-paper tooth with no colour cast.",
            TextureId::RicePaper => "Translucent paper with long, faint fibres.",
            TextureId::Frost => "Cool blue-grey haze with drifting clouds.",
            TextureId::Felt => "Dense specks on a rosy ground.",
            TextureId::Graphite => "Cool grey dust and horizontal folds, nice for code.",
            TextureId::CoarseMatte => "A bigger, softer version of Matte.",
        }
    }

    /// Whether the texture has separate light-screen and dark-screen versions.
    pub fn themed(self) -> bool {
        !matches!(self, TextureId::Matte | TextureId::CoarseMatte | TextureId::Crosshatch)
    }

    /// Intensity slider range in percent.
    pub fn range(self) -> (u8, u8) {
        match self {
            TextureId::Crosshatch => (25, 80),
            TextureId::Parchment | TextureId::Linen | TextureId::Frost => (5, 60),
            TextureId::ColdPress | TextureId::RicePaper | TextureId::Graphite => (5, 50),
            TextureId::Felt => (5, 25),
            TextureId::Matte | TextureId::CoarseMatte => (5, 40),
        }
    }

    /// Default intensity in percent.
    pub fn default_intensity(self, dark: bool) -> u8 {
        let (light, dark_value) = match self {
            TextureId::Matte | TextureId::CoarseMatte => (20, 20),
            TextureId::Crosshatch => (70, 70),
            TextureId::Parchment => (60, 60),
            TextureId::Linen => (40, 35),
            TextureId::ColdPress => (10, 10),
            TextureId::RicePaper => (20, 16),
            TextureId::Frost => (50, 25),
            TextureId::Felt => (20, 20),
            TextureId::Graphite => (32, 25),
        };
        if dark { dark_value } else { light }
    }

    /// Parses a slug or display name. Case and punctuation are ignored, and an
    /// unambiguous prefix of at least four letters is enough.
    pub fn parse(s: &str) -> Option<Self> {
        let key: String = s
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        let flat = |t: &TextureId| t.slug().replace('-', "");
        if let Some(t) = TextureId::ALL.iter().find(|t| flat(t) == key) {
            return Some(*t);
        }
        let mut prefixed =
            TextureId::ALL.iter().filter(|t| key.len() >= 4 && flat(t).starts_with(&key));
        match (prefixed.next(), prefixed.next()) {
            (Some(t), None) => Some(*t),
            _ => None,
        }
    }

    pub fn index(self) -> usize {
        TextureId::ALL.iter().position(|&t| t == self).unwrap_or(0)
    }

    /// Whether the image depends on the screen size rather than being a tile.
    pub fn screen_sized(self) -> bool {
        matches!(self, TextureId::Parchment | TextureId::RicePaper | TextureId::CoarseMatte)
    }
}

/// A rendered texture as premultiplied BGRA. Either a tile to repeat, or an
/// image covering a screen from its top-left corner.
pub struct Image {
    pub w: usize,
    pub h: usize,
    pub bgra: Vec<u8>,
    pub repeat: bool,
}

impl Image {
    fn tile(c: Canvas) -> Image {
        Image { w: c.w, h: c.h, bgra: c.to_bgra(), repeat: true }
    }

    fn full(c: Canvas) -> Image {
        Image { w: c.w, h: c.h, bgra: c.to_bgra(), repeat: false }
    }
}

/// CSS pixels to device pixels.
fn dev(css: f64, scale: f32) -> usize {
    (css * scale as f64).round().max(1.0) as usize
}

/// Renders a texture at full opacity. `scale` is device pixels per CSS pixel,
/// `screen` the monitor size in device pixels.
pub fn render(tex: TextureId, dark: bool, scale: f32, screen: (usize, usize)) -> Image {
    let p = Perlin::new(0);
    match tex {
        TextureId::Matte => matte(&p, dev(500.0, scale)),
        // The coarse variant stretches one 200-unit tile over the screen height.
        TextureId::CoarseMatte => matte(&p, screen.0.min(screen.1).max(1)),
        TextureId::Crosshatch => crosshatch(&p, scale),
        TextureId::Parchment => parchment(&p, dark, screen),
        TextureId::Linen => linen(&p, dark, scale),
        TextureId::ColdPress => cold_press(&p, dark, scale),
        TextureId::RicePaper => rice_paper(&p, dark, scale, screen),
        TextureId::Frost => frost(&p, dark, scale),
        TextureId::Felt => felt(&p, dark, scale),
        TextureId::Graphite => graphite(&p, dark, scale),
    }
}

/// Stitch noise over a square tile of `size` units so it repeats seamlessly.
fn stitch(size: f64) -> Option<(f64, f64, f64, f64)> {
    Some((0.0, 0.0, size, size))
}

fn srgb(lin: [f32; 3]) -> [f32; 3] {
    lin.map(linear_to_srgb)
}

/// Grey fractal noise (1.5 per unit, 3 octaves) whose alpha is the noise's own
/// alpha channel. 500px tile spanning 200 units.
fn matte(p: &Perlin, size: usize) -> Image {
    let g = Grid::tile(size, size, 200.0, 200.0);
    let t = Turbulence::fractal(1.5, 3, stitch(200.0));
    let [r, gr, b] = g.noise_rgb(p, &t);
    let a = g.noise(p, &t, 3);
    let mut c = Canvas::new(size, size);
    c.over_with(|i| {
        let l = linear_to_srgb(q8(desaturate(r[i], gr[i], b[i])));
        ([l, l, l], a[i])
    });
    Image::tile(c)
}

/// Two faint diagonal stripe sets (4px period, 8% brown) over a sparse black
/// grain. 1000px tile.
fn crosshatch(p: &Perlin, scale: f32) -> Image {
    let size = dev(1000.0, scale);
    let g = Grid::tile(size, size, 200.0, 200.0);
    let t = Turbulence::fractal(1.5, 3, stitch(200.0));
    let [r, gr, b] = g.noise_rgb(p, &t);
    let thread = hex_srgb(0x785a28);
    let k = std::f32::consts::FRAC_1_SQRT_2;
    // One stripe per 4px: clear until 1.5px, ramp up by 2px, hold until
    // 3.5px, ramp back down by 4px.
    let stripe = |d: f32| {
        let u = d.rem_euclid(4.0);
        let a = if u < 1.5 {
            0.0
        } else if u < 2.0 {
            (u - 1.5) / 0.5
        } else if u < 3.5 {
            1.0
        } else {
            (4.0 - u) / 0.5
        };
        a * 0.08
    };
    let css = |i: usize| (((i % size) as f32 + 0.5) / scale, ((i / size) as f32 + 0.5) / scale);
    let mut c = Canvas::new(size, size);
    c.over_with(|i| ([0.0; 3], (0.15 * (r[i] + gr[i] + b[i])).clamp(0.0, 1.0)));
    c.over_with(|i| {
        let (u, v) = css(i);
        (thread, stripe(k * (2000.0 - u - v)))
    });
    c.over_with(|i| {
        let (u, v) = css(i);
        (thread, stripe(k * (u - v + 1000.0)))
    });
    Image::tile(c)
}

/// Warm paper, stretched over the whole screen (2400×1350 units). A mottled
/// height map from fractal noise plus long "vein" noise is lit from 55° and
/// multiplied (light) or screened (dark) onto the paper colour.
fn parchment(p: &Perlin, dark: bool, screen: (usize, usize)) -> Image {
    let (w, h) = screen;
    let g = Grid { w, h, sx: w as f64 / 2400.0, sy: h as f64 / 1350.0, ox: 0.0, oy: 0.0 };
    // Stitched over a slightly larger area than the image; only the
    // frequency rounding matters here since the image isn't tiled.
    let region = Some((0.0, 0.0, 2880.0, 1620.0));
    let noise = g.noise(p, &Turbulence::fractal(0.035, 4, region), 3);
    let veins = g.noise(p, &Turbulence::new(0.006, 0.06, 2, true, region), 0);
    let height: Vec<f32> = noise
        .iter()
        .zip(&veins)
        .map(|(&n, &v)| {
            let mask = q8((1.5 * v - 0.75).clamp(0.0, 1.0));
            q8(0.6 * n * mask + 0.4 * n)
        })
        .collect();
    let (surface, light, paper) =
        if dark { (5.4, 0xA68B6D, 0x1C130A) } else { (4.5, 0xFFFFFF, 0xEAD5A6) };
    let nl =
        diffuse(&height, w, h, (surface * g.sx as f32, surface * g.sy as f32), 55.0, 60.0, false);
    Image::full(lit_paper(w, h, &nl, light, paper, dark))
}

/// Paper colour combined with a lit relief: multiplied for light screens,
/// screened for dark ones.
fn lit_paper(w: usize, h: usize, nl: &[f32], light: u32, paper: u32, dark: bool) -> Canvas {
    let (lc, f) = (hex_linear(light), hex_linear(paper));
    let mut c = Canvas::new(w, h);
    c.over_with(|i| {
        let mix = |k: usize| {
            let l = q8(lc[k] * nl[i]);
            if dark { f[k] + l - f[k] * l } else { f[k] * l }
        };
        (srgb([mix(0), mix(1), mix(2)]), 1.0)
    });
    c
}

/// Warp and weft noise (stretched 0.015×0.35 and 0.35×0.015) averaged into a
/// woven height map and lit from 45°. 500px tile.
fn linen(p: &Perlin, dark: bool, scale: f32) -> Image {
    let size = dev(500.0, scale);
    let g = Grid::tile(size, size, 500.0, 500.0);
    let warp = g.noise(p, &Turbulence::new(0.015, 0.35, 3, true, stitch(500.0)), 3);
    let weft = g.noise(p, &Turbulence::new(0.35, 0.015, 3, true, stitch(500.0)), 3);
    let height: Vec<f32> =
        warp.iter().zip(&weft).map(|(&a, &b)| q8(0.5 * a * b + 0.5 * a)).collect();
    let mut c = Canvas::new(size, size);
    if dark {
        // Tan threads whose opacity follows the light.
        let nl = diffuse(&height, size, size, (2.7 * scale, 2.7 * scale), 45.0, 60.0, true);
        let lc = hex_linear(0xA68B6D);
        let thread = srgb([0.78, 0.65, 0.48]);
        c.over_with(|i| {
            let sum: f32 = lc.iter().map(|&v| q8(v * nl[i])).sum();
            (thread, (0.33 * sum - 0.12).clamp(0.0, 1.0))
        });
    } else {
        let nl = diffuse(&height, size, size, (1.8 * scale, 1.8 * scale), 45.0, 65.0, true);
        c = lit_paper(size, size, &nl, 0xFFFFFF, 0xEFEADF, false);
    }
    Image::tile(c)
}

/// Big soft bumps plus fine tooth, lit from 60°. Only the relief is drawn:
/// crevices darken on light screens, peaks highlight on dark ones, and flat
/// areas stay transparent. 600px tile.
fn cold_press(p: &Perlin, dark: bool, scale: f32) -> Image {
    let size = dev(600.0, scale);
    let g = Grid::tile(size, size, 600.0, 600.0);
    let bumps = g.noise(p, &Turbulence::fractal(0.075, 3, stitch(600.0)), 3);
    let tooth = g.noise(p, &Turbulence::fractal(0.68, 2, stitch(600.0)), 3);
    let height: Vec<f32> =
        bumps.iter().zip(&tooth).map(|(&b, &t)| q8(0.4 * b * t + 0.6 * b)).collect();
    let nl = diffuse(&height, size, size, (1.8 * scale, 1.8 * scale), 60.0, 60.0, true);
    let mut c = Canvas::new(size, size);
    c.over_with(|i| {
        // A flat surface gets N·L = sin(60°) ≈ 0.866, which maps to zero alpha.
        let rgb_sum = 3.0 * q8(nl[i]);
        if dark {
            ([1.0; 3], (1.5 * rgb_sum - 3.9).clamp(0.0, 1.0))
        } else {
            ([0.0; 3], (3.9 - 1.5 * rgb_sum).clamp(0.0, 1.0))
        }
    });
    Image::tile(c)
}

/// A paper colour with two noise layers multiplied (or screened) into it:
/// fine grey grain on a 500px tile, and long horizontal fibres from absolute
/// turbulence on a 900px tile. The two periods don't line up, so this one is
/// rendered at screen size.
fn rice_paper(p: &Perlin, dark: bool, scale: f32, screen: (usize, usize)) -> Image {
    let layer = |size_css: f64, t: Turbulence| {
        let size = dev(size_css, scale);
        let g = Grid::tile(size, size, size_css, size_css);
        let [r, gr, b] = g.noise_rgb(p, &t);
        let a = g.noise(p, &t, 3);
        let lum: Vec<f32> =
            (0..size * size).map(|i| linear_to_srgb(q8(desaturate(r[i], gr[i], b[i])))).collect();
        (size, lum, a)
    };
    let (s1, l1, a1) = layer(500.0, Turbulence::fractal(0.6, 2, stitch(500.0)));
    let (s2, l2, a2) = layer(900.0, Turbulence::new(0.0044, 0.089, 1, false, stitch(900.0)));
    let paper = hex_srgb(if dark { 0x1e1b12 } else { 0xf6f1e2 });
    let blend = |cb: f32, cs: f32| if dark { cb + cs - cb * cs } else { cb * cs };
    let (w, h) = screen;
    let mut c = Canvas::new(w, h);
    c.over_with(|i| {
        let (x, y) = (i % w, i / w);
        let mut col = paper;
        for (s, l, a) in [(s2, &l2, &a2), (s1, &l1, &a1)] {
            let j = (y % s) * s + x % s;
            for ch in &mut col {
                *ch = a[j] * blend(*ch, l[j]) + (1.0 - a[j]) * *ch;
            }
        }
        (col, 1.0)
    });
    Image::full(c)
}

/// A faint blue-grey veil with slow clouds and a fine grain. 800px tile.
fn frost(p: &Perlin, dark: bool, scale: f32) -> Image {
    let size = dev(800.0, scale);
    let g = Grid::tile(size, size, 800.0, 800.0);
    let mut c = Canvas::new(size, size);
    if dark {
        c.fill(hex_srgb(0x8CAAC8), 0.1);
        let clouds = g.noise(p, &Turbulence::fractal(0.008, 4, stitch(800.0)), 0);
        let t = Turbulence::fractal(1.2, 2, stitch(800.0));
        let [r, gr, b] = g.noise_rgb(p, &t);
        let a = g.noise(p, &t, 3);
        // Coloured grain, masked by cloud density.
        c.over_with(|i| (srgb([r[i], gr[i], b[i]]), a[i] * clouds[i] * 0.85));
    } else {
        c.fill(hex_srgb(0x5E728A), 0.28);
        let sum = |t: &Turbulence| {
            let [r, gr, b] = g.noise_rgb(p, t);
            (0..size * size).map(move |i| r[i] + gr[i] + b[i]).collect::<Vec<f32>>()
        };
        let clouds = sum(&Turbulence::fractal(0.012, 5, stitch(800.0)));
        let tint = srgb([0.14, 0.19, 0.25]);
        c.over_with(|i| (tint, q8((0.42 * clouds[i]).clamp(0.0, 1.0)) * 0.95));
        let grain = sum(&Turbulence::fractal(1.35, 2, stitch(800.0)));
        let tint = srgb([0.08, 0.10, 0.13]);
        c.over_with(|i| (tint, q8((0.24 * grain[i]).clamp(0.0, 1.0)) * 0.70));
    }
    Image::tile(c)
}

/// Very fine noise (2.2 per unit, 5 octaves) thresholded into dense specks
/// on an opaque rose (light) or wine (dark) ground. 400px tile.
fn felt(p: &Perlin, dark: bool, scale: f32) -> Image {
    let size = dev(400.0, scale);
    let g = Grid::tile(size, size, 400.0, 400.0);
    let noise = g.noise(p, &Turbulence::fractal(2.2, 5, stitch(400.0)), 0);
    let (ground, speck) =
        if dark { (0x3D0E16, [1.0, 0.85, 0.88]) } else { (0xFAF2F4, [0.39, 0.08, 0.14]) };
    let speck = srgb(speck);
    let mut c = Canvas::new(size, size);
    c.fill(hex_srgb(ground), 1.0);
    c.over_with(|i| (speck, q8((12.0 * noise[i] - 5.8).clamp(0.0, 1.0)) * 0.85));
    Image::tile(c)
}

/// Fine dust on long horizontal folds, lit from straight below. 500px tile.
fn graphite(p: &Perlin, dark: bool, scale: f32) -> Image {
    let size = dev(500.0, scale);
    let g = Grid::tile(size, size, 500.0, 500.0);
    let dust = g.noise(p, &Turbulence::fractal(0.8, 3, stitch(500.0)), 3);
    let folds = g.noise(p, &Turbulence::new(0.005, 0.15, 2, true, stitch(500.0)), 3);
    let height: Vec<f32> =
        dust.iter().zip(&folds).map(|(&d, &f)| q8(0.4 * d * f + 0.6 * d)).collect();
    let (surface, light, paper) =
        if dark { (1.5, 0x78909C, 0x14171A) } else { (2.0, 0xFFFFFF, 0xECEFF1) };
    let nl = diffuse(&height, size, size, (surface * scale, surface * scale), 90.0, 65.0, true);
    Image::tile(lit_paper(size, size, &nl, light, paper, dark))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_names() {
        assert_eq!(TextureId::parse("matte"), Some(TextureId::Matte));
        assert_eq!(TextureId::parse("Cold Press"), Some(TextureId::ColdPress));
        assert_eq!(TextureId::parse("rice-paper"), Some(TextureId::RicePaper));
        assert_eq!(TextureId::parse("graph"), Some(TextureId::Graphite));
        assert_eq!(TextureId::parse("coarse"), Some(TextureId::CoarseMatte));
        assert_eq!(TextureId::parse("nope"), None);
    }

    #[test]
    fn two_line_names() {
        assert_eq!(TextureId::ColdPress.lines(), ("Cold", "Press"));
        assert_eq!(TextureId::Felt.lines(), ("Felt", ""));
    }

    #[test]
    fn defaults_within_ranges() {
        for t in TextureId::ALL {
            let (lo, hi) = t.range();
            for dark in [false, true] {
                let d = t.default_intensity(dark);
                assert!(lo <= d && d <= hi, "{t:?}");
            }
        }
    }

    #[test]
    fn renders_premultiplied() {
        for tex in TextureId::ALL {
            for dark in [false, true] {
                let img = render(tex, dark, 0.5, (320, 200));
                assert_eq!(img.bgra.len(), img.w * img.h * 4);
                for px in img.bgra.as_chunks::<4>().0.iter().step_by(331) {
                    assert!(px[0] <= px[3] && px[1] <= px[3] && px[2] <= px[3], "{tex:?}");
                }
            }
        }
    }
}
