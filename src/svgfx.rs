//! Software versions of the few SVG filter primitives the textures use:
//! `feTurbulence`, `feDiffuseLighting` with a distant light, and the
//! linear-RGB colour handling of `color-interpolation-filters`.
//!
//! The noise follows the reference code from the SVG 1.1 spec (seeded
//! lattice, `stitchTiles` and all), so a texture comes out the same as the
//! equivalent SVG in a browser. Two details matter for that: samples are
//! taken at the pixel centre plus half a unit, and values are rounded to
//! 8 bits between stages, like a browser's RGBA8 buffers.

use std::thread;

const BSIZE: usize = 0x100;
const BM: i32 = 0xff;
const PERLIN_N: i32 = 0x1000;

const RAND_M: i64 = 2_147_483_647; // 2^31 - 1
const RAND_A: i64 = 16_807; // 7^5
const RAND_Q: i64 = 127_773; // m / a
const RAND_R: i64 = 2_836; // m % a

fn setup_seed(mut seed: i64) -> i64 {
    if seed <= 0 {
        seed = -(seed % (RAND_M - 1)) + 1;
    }
    seed.min(RAND_M - 1)
}

fn random(seed: i64) -> i64 {
    let r = RAND_A * (seed % RAND_Q) - RAND_R * (seed / RAND_Q);
    if r <= 0 { r + RAND_M } else { r }
}

/// The lattice and gradient tables of `feTurbulence` for one `seed`.
pub struct Perlin {
    lattice: [usize; BSIZE * 2 + 2],
    grad: [[[f64; 2]; BSIZE * 2 + 2]; 4],
}

/// Lattice wrapping state used when `stitchTiles="stitch"`.
#[derive(Debug, Clone, Copy)]
struct Stitch {
    width: i32,
    height: i32,
    wrap_x: i32,
    wrap_y: i32,
}

/// One configured `feTurbulence` (frequencies already adjusted for stitching).
#[derive(Debug, Clone, Copy)]
pub struct Turbulence {
    fx: f64,
    fy: f64,
    octaves: u32,
    fractal: bool,
    stitch: Option<Stitch>,
}

impl Turbulence {
    /// Fractal noise (`fractal`) or absolute "turbulence". `tile` is the
    /// stitching rectangle `(x, y, w, h)` in units, if any.
    pub fn new(
        fx: f64,
        fy: f64,
        octaves: u32,
        fractal: bool,
        tile: Option<(f64, f64, f64, f64)>,
    ) -> Self {
        let (mut fx, mut fy) = (fx, fy);
        let stitch = tile.map(|(tx, ty, tw, th)| {
            let adjust = |f: f64, size: f64| {
                if f == 0.0 {
                    return f;
                }
                let lo = (size * f).floor() / size;
                let hi = (size * f).ceil() / size;
                if f / lo < hi / f { lo } else { hi }
            };
            fx = adjust(fx, tw);
            fy = adjust(fy, th);
            let width = (tw * fx + 0.5) as i32;
            let height = (th * fy + 0.5) as i32;
            Stitch {
                width,
                height,
                wrap_x: (tx * fx + PERLIN_N as f64 + width as f64) as i32,
                wrap_y: (ty * fy + PERLIN_N as f64 + height as f64) as i32,
            }
        });
        Turbulence { fx, fy, octaves, fractal, stitch }
    }

    pub fn fractal(f: f64, octaves: u32, tile: Option<(f64, f64, f64, f64)>) -> Self {
        Self::new(f, f, octaves, true, tile)
    }
}

impl Perlin {
    pub fn new(seed: i64) -> Box<Perlin> {
        let mut p =
            Box::new(Perlin { lattice: [0; BSIZE * 2 + 2], grad: [[[0.0; 2]; BSIZE * 2 + 2]; 4] });
        let mut s = setup_seed(seed);
        for k in 0..4 {
            for i in 0..BSIZE {
                p.lattice[i] = i;
                for g in p.grad[k][i].iter_mut() {
                    s = random(s);
                    *g = ((s % (2 * BSIZE as i64)) - BSIZE as i64) as f64 / BSIZE as f64;
                }
                let [a, b] = p.grad[k][i];
                let n = (a * a + b * b).sqrt();
                if n > 0.0 {
                    p.grad[k][i] = [a / n, b / n];
                }
            }
        }
        let mut i = BSIZE;
        loop {
            i -= 1;
            if i == 0 {
                break;
            }
            let k = p.lattice[i];
            s = random(s);
            let j = (s % BSIZE as i64) as usize;
            p.lattice[i] = p.lattice[j];
            p.lattice[j] = k;
        }
        for i in 0..BSIZE + 2 {
            p.lattice[BSIZE + i] = p.lattice[i];
            for g in p.grad.iter_mut() {
                g[BSIZE + i] = g[i];
            }
        }
        p
    }

    #[inline]
    fn noise2(&self, ch: usize, vx: f64, vy: f64, st: Option<&Stitch>) -> f64 {
        let t = vx + PERLIN_N as f64;
        let mut bx0 = t as i32;
        let mut bx1 = bx0 + 1;
        let rx0 = t - t.trunc();
        let rx1 = rx0 - 1.0;
        let t = vy + PERLIN_N as f64;
        let mut by0 = t as i32;
        let mut by1 = by0 + 1;
        let ry0 = t - t.trunc();
        let ry1 = ry0 - 1.0;
        if let Some(s) = st {
            if bx0 >= s.wrap_x {
                bx0 -= s.width;
            }
            if bx1 >= s.wrap_x {
                bx1 -= s.width;
            }
            if by0 >= s.wrap_y {
                by0 -= s.height;
            }
            if by1 >= s.wrap_y {
                by1 -= s.height;
            }
        }
        let (bx0, bx1, by0, by1) =
            ((bx0 & BM) as usize, (bx1 & BM) as usize, (by0 & BM) as usize, (by1 & BM) as usize);
        let i = self.lattice[bx0];
        let j = self.lattice[bx1];
        let b00 = self.lattice[i + by0];
        let b10 = self.lattice[j + by0];
        let b01 = self.lattice[i + by1];
        let b11 = self.lattice[j + by1];
        let sx = rx0 * rx0 * (3.0 - 2.0 * rx0);
        let sy = ry0 * ry0 * (3.0 - 2.0 * ry0);
        let g = &self.grad[ch];
        let u = rx0 * g[b00][0] + ry0 * g[b00][1];
        let v = rx1 * g[b10][0] + ry0 * g[b10][1];
        let a = u + sx * (v - u);
        let u = rx0 * g[b01][0] + ry1 * g[b01][1];
        let v = rx1 * g[b11][0] + ry1 * g[b11][1];
        let b = u + sx * (v - u);
        a + sy * (b - a)
    }

    /// The value (0–1) of channel `ch` (0 = R … 3 = A) at `(x, y)` in units.
    pub fn sample(&self, t: &Turbulence, ch: usize, x: f64, y: f64) -> f32 {
        let mut st = t.stitch;
        let (mut vx, mut vy) = (x * t.fx, y * t.fy);
        let mut ratio = 1.0;
        let mut sum = 0.0;
        for _ in 0..t.octaves {
            let n = self.noise2(ch, vx, vy, st.as_ref());
            sum += if t.fractal { n } else { n.abs() } / ratio;
            vx *= 2.0;
            vy *= 2.0;
            ratio *= 2.0;
            if let Some(s) = st.as_mut() {
                s.width *= 2;
                s.wrap_x = 2 * s.wrap_x - PERLIN_N;
                s.height *= 2;
                s.wrap_y = 2 * s.wrap_y - PERLIN_N;
            }
        }
        let v = if t.fractal { (sum + 1.0) * 0.5 } else { sum };
        q8(v as f32)
    }
}

/// Maps device pixels to noise units.
#[derive(Debug, Clone, Copy)]
pub struct Grid {
    pub w: usize,
    pub h: usize,
    /// Device pixels per user unit.
    pub sx: f64,
    pub sy: f64,
    /// Device position of the unit origin.
    pub ox: f64,
    pub oy: f64,
}

impl Grid {
    /// A `w`×`h` pixel tile spanning `vw`×`vh` units.
    pub fn tile(w: usize, h: usize, vw: f64, vh: f64) -> Grid {
        Grid { w, h, sx: w as f64 / vw, sy: h as f64 / vh, ox: 0.0, oy: 0.0 }
    }

    /// Renders one channel of a turbulence over the grid.
    pub fn noise(&self, p: &Perlin, t: &Turbulence, ch: usize) -> Vec<f32> {
        let g = *self;
        par_map(g.w, g.h, |x, y| {
            // Browsers sample at the pixel centre, shifted by half a unit.
            p.sample(
                t,
                ch,
                (x as f64 + 0.5 - g.ox) / g.sx + 0.5,
                (y as f64 + 0.5 - g.oy) / g.sy + 0.5,
            )
        })
    }

    /// Renders the R, G and B channels of a turbulence.
    pub fn noise_rgb(&self, p: &Perlin, t: &Turbulence) -> [Vec<f32>; 3] {
        [self.noise(p, t, 0), self.noise(p, t, 1), self.noise(p, t, 2)]
    }
}

/// Computes `f(x, y)` for every pixel, spreading rows over all cores.
pub fn par_map(w: usize, h: usize, f: impl Fn(usize, usize) -> f32 + Sync) -> Vec<f32> {
    let mut out = vec![0.0f32; w * h];
    if w == 0 || h == 0 {
        return out;
    }
    let threads = thread::available_parallelism().map_or(4, |n| n.get()).min(16);
    let rows_per = h.div_ceil(threads);
    thread::scope(|s| {
        for (ci, chunk) in out.chunks_mut(rows_per * w).enumerate() {
            let f = &f;
            s.spawn(move || {
                for (k, v) in chunk.iter_mut().enumerate() {
                    let y = ci * rows_per + k / w;
                    *v = f(k % w, y);
                }
            });
        }
    });
    out
}

/// Rounds to 8 bits, like the RGBA8 buffers between filter stages.
#[inline]
pub fn q8(v: f32) -> f32 {
    (v.clamp(0.0, 1.0) * 255.0).round() / 255.0
}

#[inline]
pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.040_45 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

#[inline]
pub fn linear_to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

/// `#rrggbb` → linear RGB.
pub fn hex_linear(hex: u32) -> [f32; 3] {
    let s = hex_srgb(hex);
    [srgb_to_linear(s[0]), srgb_to_linear(s[1]), srgb_to_linear(s[2])]
}

/// `#rrggbb` → sRGB 0–1.
pub fn hex_srgb(hex: u32) -> [f32; 3] {
    [
        ((hex >> 16) & 0xff) as f32 / 255.0,
        ((hex >> 8) & 0xff) as f32 / 255.0,
        (hex & 0xff) as f32 / 255.0,
    ]
}

/// The `feColorMatrix type="saturate" values="0"` luminance row.
#[inline]
pub fn desaturate(r: f32, g: f32, b: f32) -> f32 {
    (0.213 * r + 0.715 * g + 0.072 * b).clamp(0.0, 1.0)
}

/// `feDiffuseLighting` with `feDistantLight`: N·L per pixel for the height
/// field `alpha`, using the spec's Sobel normals.
///
/// The surface scale is per axis and should already be multiplied by device
/// pixels per unit, which keeps the relief the same at any resolution. `wrap`
/// treats the field as a tile; otherwise edges are clamped.
pub fn diffuse(
    alpha: &[f32],
    w: usize,
    h: usize,
    surface_scale: (f32, f32),
    azimuth: f32,
    elevation: f32,
    wrap: bool,
) -> Vec<f32> {
    let (az, el) = (azimuth.to_radians(), elevation.to_radians());
    let l = [az.cos() * el.cos(), az.sin() * el.cos(), el.sin()];
    let at = |x: isize, y: isize| -> f32 {
        let (x, y) = if wrap {
            (x.rem_euclid(w as isize) as usize, y.rem_euclid(h as isize) as usize)
        } else {
            (x.clamp(0, w as isize - 1) as usize, y.clamp(0, h as isize - 1) as usize)
        };
        alpha[y * w + x]
    };
    par_map(w, h, |x, y| {
        let (x, y) = (x as isize, y as isize);
        let nx = -surface_scale.0
            * 0.25
            * ((at(x + 1, y - 1) + 2.0 * at(x + 1, y) + at(x + 1, y + 1))
                - (at(x - 1, y - 1) + 2.0 * at(x - 1, y) + at(x - 1, y + 1)));
        let ny = -surface_scale.1
            * 0.25
            * ((at(x - 1, y + 1) + 2.0 * at(x, y + 1) + at(x + 1, y + 1))
                - (at(x - 1, y - 1) + 2.0 * at(x, y - 1) + at(x + 1, y - 1)));
        let norm = (nx * nx + ny * ny + 1.0).sqrt();
        ((nx * l[0] + ny * l[1] + l[2]) / norm).max(0.0)
    })
}

/// A premultiplied RGBA float image in sRGB for stacking texture layers.
pub struct Canvas {
    pub w: usize,
    pub h: usize,
    pub px: Vec<[f32; 4]>,
}

impl Canvas {
    pub fn new(w: usize, h: usize) -> Canvas {
        Canvas { w, h, px: vec![[0.0; 4]; w * h] }
    }

    /// Fills with an opaque or translucent sRGB colour (source-over).
    pub fn fill(&mut self, srgb: [f32; 3], alpha: f32) {
        for d in &mut self.px {
            over(d, srgb, alpha);
        }
    }

    /// Source-over a layer given as non-premultiplied sRGB colour and alpha
    /// per pixel.
    pub fn over_with(&mut self, f: impl Fn(usize) -> ([f32; 3], f32)) {
        for (i, d) in self.px.iter_mut().enumerate() {
            let (c, a) = f(i);
            over(d, c, a);
        }
    }

    /// Converts to premultiplied BGRA bytes (X11 ARGB32, little-endian).
    pub fn to_bgra(&self) -> Vec<u8> {
        let mut out = vec![0u8; self.px.len() * 4];
        for (o, p) in out.as_chunks_mut::<4>().0.iter_mut().zip(&self.px) {
            let a = p[3].clamp(0.0, 1.0);
            let c = |v: f32| (v.clamp(0.0, a) * 255.0 + 0.5) as u8;
            *o = [c(p[2]), c(p[1]), c(p[0]), (a * 255.0 + 0.5) as u8];
        }
        out
    }
}

#[inline]
fn over(d: &mut [f32; 4], c: [f32; 3], a: f32) {
    let a = a.clamp(0.0, 1.0);
    let inv = 1.0 - a;
    d[0] = c[0] * a + d[0] * inv;
    d[1] = c[1] * a + d[1] * inv;
    d[2] = c[2] * a + d[2] * inv;
    d[3] = a + d[3] * inv;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rng_matches_park_miller() {
        // Park–Miller minimal standard: x1 = 16807 for seed 1.
        assert_eq!(random(1), 16_807);
        assert_eq!(random(16_807), 282_475_249);
        assert_eq!(setup_seed(0), 1);
    }

    #[test]
    fn stitched_noise_tiles() {
        // With stitching, the right/bottom edge of the tile continues into the
        // left/top edge, so a repeated tile has no seam.
        let p = Perlin::new(0);
        let t = Turbulence::fractal(0.07, 3, Some((0.0, 0.0, 100.0, 100.0)));
        for &y in &[0.0, 7.0, 50.5, 99.0] {
            for ch in 0..4 {
                let a = p.sample(&t, ch, 0.0, y);
                let b = p.sample(&t, ch, 100.0, y);
                assert!((a - b).abs() <= 1.0 / 255.0 + 1e-6, "x seam ch{ch}: {a} vs {b}");
                let a = p.sample(&t, ch, y, 0.0);
                let b = p.sample(&t, ch, y, 100.0);
                assert!((a - b).abs() <= 1.0 / 255.0 + 1e-6, "y seam ch{ch}: {a} vs {b}");
            }
        }
    }

    #[test]
    fn fractal_noise_is_centred() {
        let p = Perlin::new(0);
        let t = Turbulence::fractal(0.05, 2, None);
        let g = Grid::tile(64, 64, 64.0, 64.0);
        let v = g.noise(&p, &t, 3);
        let mean = v.iter().sum::<f32>() / v.len() as f32;
        assert!((mean - 0.5).abs() < 0.08, "mean {mean}");
    }

    #[test]
    fn flat_surface_lighting() {
        let n = diffuse(&[0.5; 16], 4, 4, (3.0, 3.0), 45.0, 60.0, true);
        assert!(n.iter().all(|&v| (v - 60f32.to_radians().sin()).abs() < 1e-5));
    }
}
