//! `paperlike preview <out-dir> [texture] [--dark] [--intensity N] [--scale S]
//!                    [--size WxH] [--bg rrggbb]`
//!
//! Renders textures into PPM files without an X server: over a synthetic page
//! by default, or over a solid `--bg` colour (handy for comparing against a
//! reference renderer; use `--intensity 100` for the raw texture).

use std::path::PathBuf;
use std::process::ExitCode;

use crate::texture::{self, TextureId};

/// A page with lines of "text" and a colourful block.
fn page(w: usize, h: usize, dark: bool) -> Vec<[f32; 3]> {
    let (bg, ink) = if dark { (0.11, 0.86) } else { (1.0, 0.08) };
    let mut px = vec![[bg; 3]; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut c = [bg; 3];
            let line = (y as i32 - 40) / 18;
            let in_line = (y as i32 - 40).rem_euclid(18) < 9;
            if (0..16).contains(&line) && in_line && x > 40 && x < w * 6 / 10 {
                let word = (x as i32 * 7 + line * 13) % 53;
                if word > 6 {
                    c = [ink; 3];
                }
            }
            if x > w * 2 / 3 && x < w * 15 / 16 && y > h / 7 && y < h * 3 / 4 {
                let t = (x - w * 2 / 3) as f32 / (w as f32 * 0.27);
                c = [0.9 - 0.5 * t, 0.35 + 0.4 * t, 0.25 + 0.6 * (y as f32 / h as f32)];
            }
            px[y * w + x] = c;
        }
    }
    px
}

fn write_ppm(path: &PathBuf, w: usize, h: usize, px: &[[f32; 3]]) -> std::io::Result<()> {
    let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
    for p in px {
        for c in p {
            out.push((c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
        }
    }
    std::fs::write(path, out)
}

pub fn run(args: &[String]) -> ExitCode {
    let Some(dir) = args.first().map(PathBuf::from) else {
        eprintln!(
            "usage: paperlike preview <out-dir> [texture] [--dark] [--intensity N] [--scale S] [--size WxH] [--bg rrggbb]"
        );
        return ExitCode::from(2);
    };
    let dark = args.iter().any(|a| a == "--dark");
    let value = |name: &str| args.windows(2).find(|w| w[0] == name).map(|w| w[1].clone());
    let num = |name: &str| value(name).and_then(|v| v.parse::<f32>().ok());
    let intensity = num("--intensity");
    let scale = num("--scale").unwrap_or(1.0);
    let (w, h) = value("--size")
        .and_then(|s| s.split_once('x').and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?))))
        .unwrap_or((640usize, 400usize));
    let solid = value("--bg").and_then(|s| u32::from_str_radix(s.trim_start_matches('#'), 16).ok());
    let only = args.get(1).filter(|a| !a.starts_with("--")).and_then(|a| TextureId::parse(a));
    if std::fs::create_dir_all(&dir).is_err() {
        eprintln!("cannot create {}", dir.display());
        return ExitCode::FAILURE;
    }
    let base = match solid {
        Some(c) => vec![crate::svgfx::hex_srgb(c); w * h],
        None => page(w, h, dark),
    };
    let theme = if dark { "dark" } else { "light" };
    if solid.is_none() {
        write_ppm(&dir.join(format!("plain-{theme}.ppm")), w, h, &base).ok();
    }
    for tex in TextureId::ALL.into_iter().filter(|t| only.is_none_or(|o| o == *t)) {
        let img = texture::render(tex, dark, scale, (w, h));
        let a = intensity.unwrap_or(tex.default_intensity(dark) as f32) / 100.0;
        let mut out = base.clone();
        for y in 0..h {
            for x in 0..w {
                let (tx, ty) = if img.repeat { (x % img.w, y % img.h) } else { (x, y) };
                if tx >= img.w || ty >= img.h {
                    continue;
                }
                let i = (ty * img.w + tx) * 4;
                // Premultiplied texel at layer opacity `a`, composited over the base.
                let ta = img.bgra[i + 3] as f32 / 255.0 * a;
                let src =
                    [img.bgra[i + 2], img.bgra[i + 1], img.bgra[i]].map(|v| v as f32 / 255.0 * a);
                let d = &mut out[y * w + x];
                for k in 0..3 {
                    d[k] = src[k] + d[k] * (1.0 - ta);
                }
            }
        }
        let suffix = solid.map_or(String::new(), |c| format!("-bg{c:06x}"));
        let path = dir.join(format!("{}-{theme}{suffix}.ppm", tex.slug()));
        if let Err(e) = write_ppm(&path, w, h, &out) {
            eprintln!("{}: {e}", path.display());
            return ExitCode::FAILURE;
        }
        println!("{}", path.display());
    }
    ExitCode::SUCCESS
}
