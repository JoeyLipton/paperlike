//! Drawing primitives in logical (unscaled) coordinates on top of tiny-skia.

use tiny_skia::{
    FillRule, GradientStop, LineCap, LineJoin, LinearGradient, Mask, Paint, Path, PathBuilder,
    Pixmap, Point, SpreadMode, Stroke, Transform,
};

use super::text::{Clip, FontKind, Fonts};

/// Non-premultiplied RGBA colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Col(pub u8, pub u8, pub u8, pub u8);

impl Col {
    /// Same colour with its alpha multiplied by `f`.
    pub fn fade(self, f: f32) -> Col {
        Col(self.0, self.1, self.2, (self.3 as f32 * f.clamp(0.0, 1.0)).round() as u8)
    }
    fn sk(self) -> tiny_skia::Color {
        tiny_skia::Color::from_rgba8(self.0, self.1, self.2, self.3)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct R {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl R {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> R {
        R { x, y, w, h }
    }
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    pub fn cx(&self) -> f32 {
        self.x + self.w * 0.5
    }
    pub fn cy(&self) -> f32 {
        self.y + self.h * 0.5
    }
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }
    pub fn intersect(&self, o: &R) -> Option<R> {
        let x0 = self.x.max(o.x);
        let y0 = self.y.max(o.y);
        let x1 = self.right().min(o.right());
        let y1 = self.bottom().min(o.bottom());
        (x1 > x0 && y1 > y0).then(|| R::new(x0, y0, x1 - x0, y1 - y0))
    }
}

pub fn rrect_path(r: R, rad: f32) -> Option<Path> {
    let rad = rad.min(r.w * 0.5).min(r.h * 0.5).max(0.0);
    let k = rad * 0.447_715; // 1 - 0.552285 (circle approximation)
    let (x0, y0, x1, y1) = (r.x, r.y, r.right(), r.bottom());
    let mut pb = PathBuilder::new();
    pb.move_to(x0 + rad, y0);
    pb.line_to(x1 - rad, y0);
    pb.cubic_to(x1 - k, y0, x1, y0 + k, x1, y0 + rad);
    pb.line_to(x1, y1 - rad);
    pb.cubic_to(x1, y1 - k, x1 - k, y1, x1 - rad, y1);
    pb.line_to(x0 + rad, y1);
    pb.cubic_to(x0 + k, y1, x0, y1 - k, x0, y1 - rad);
    pb.line_to(x0, y0 + rad);
    pb.cubic_to(x0, y0 + k, x0 + k, y0, x0 + rad, y0);
    pb.close();
    pb.finish()
}

pub struct Painter<'a> {
    pub pm: &'a mut Pixmap,
    pub fonts: &'a mut Fonts,
    /// Device pixels per logical pixel.
    pub s: f32,
    /// Vertical translation applied to everything (used for scrolling).
    pub dy: f32,
    clip: Option<R>,
    mask: Option<Mask>,
}

impl<'a> Painter<'a> {
    pub fn new(pm: &'a mut Pixmap, fonts: &'a mut Fonts, s: f32) -> Painter<'a> {
        Painter { pm, fonts, s, dy: 0.0, clip: None, mask: None }
    }

    fn tf(&self) -> Transform {
        Transform::from_row(self.s, 0.0, 0.0, self.s, 0.0, self.dy * self.s)
    }

    /// Restricts drawing to `r` (in surface coordinates, unaffected by `dy`).
    pub fn set_clip(&mut self, r: Option<R>) {
        self.clip = r;
        self.mask = r.and_then(|r| {
            let mut m = Mask::new(self.pm.width(), self.pm.height())?;
            let p = PathBuilder::from_rect(tiny_skia::Rect::from_xywh(
                r.x * self.s,
                r.y * self.s,
                r.w * self.s,
                r.h * self.s,
            )?);
            m.fill_path(&p, FillRule::Winding, false, Transform::identity());
            Some(m)
        });
    }

    /// The part of `r` (content coordinates) that is visible on the surface.
    pub fn visible(&self, r: R) -> Option<R> {
        let r = R::new(r.x, r.y + self.dy, r.w, r.h);
        match self.clip {
            Some(c) => r.intersect(&c),
            None => Some(r),
        }
    }

    fn paint(c: Col) -> Paint<'static> {
        let mut p = Paint::default();
        p.set_color_rgba8(c.0, c.1, c.2, c.3);
        p.anti_alias = true;
        p
    }

    pub fn fill_path(&mut self, path: &Path, c: Col) {
        let tf = self.tf();
        self.pm.fill_path(path, &Self::paint(c), FillRule::Winding, tf, self.mask.as_ref());
    }

    pub fn stroke_path(&mut self, path: &Path, width: f32, c: Col) {
        let stroke = Stroke {
            width,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Default::default()
        };
        let tf = self.tf();
        self.pm.stroke_path(path, &Self::paint(c), &stroke, tf, self.mask.as_ref());
    }

    pub fn rrect(&mut self, r: R, rad: f32, c: Col) {
        if let Some(p) = rrect_path(r, rad) {
            self.fill_path(&p, c);
        }
    }

    pub fn rrect_stroke(&mut self, r: R, rad: f32, width: f32, c: Col) {
        let h = width * 0.5;
        if let Some(p) = rrect_path(R::new(r.x + h, r.y + h, r.w - width, r.h - width), rad - h) {
            self.stroke_path(&p, width, c);
        }
    }

    /// Rounded rectangle with a vertical gradient.
    pub fn rrect_vgrad(&mut self, r: R, rad: f32, top: Col, bottom: Col) {
        let Some(path) = rrect_path(r, rad) else {
            return;
        };
        let Some(shader) = LinearGradient::new(
            Point::from_xy(0.0, r.y),
            Point::from_xy(0.0, r.bottom()),
            vec![GradientStop::new(0.0, top.sk()), GradientStop::new(1.0, bottom.sk())],
            SpreadMode::Pad,
            Transform::identity(),
        ) else {
            return self.rrect(r, rad, top);
        };
        let paint = Paint { shader, anti_alias: true, ..Default::default() };
        let tf = self.tf();
        self.pm.fill_path(&path, &paint, FillRule::Winding, tf, self.mask.as_ref());
    }

    pub fn circle(&mut self, cx: f32, cy: f32, r: f32, c: Col) {
        if let Some(p) = PathBuilder::from_circle(cx, cy, r) {
            self.fill_path(&p, c);
        }
    }

    pub fn circle_stroke(&mut self, cx: f32, cy: f32, r: f32, width: f32, c: Col) {
        if let Some(p) = PathBuilder::from_circle(cx, cy, r) {
            self.stroke_path(&p, width, c);
        }
    }

    /// Radial "glow" swatch.
    pub fn radial(&mut self, cx: f32, cy: f32, r: f32, inner: Col, outer: Col) {
        let Some(path) = PathBuilder::from_circle(cx, cy, r) else {
            return;
        };
        let Some(shader) = tiny_skia::RadialGradient::new(
            Point::from_xy(cx, cy - r * 0.3),
            0.0,
            Point::from_xy(cx, cy),
            r,
            vec![GradientStop::new(0.0, inner.sk()), GradientStop::new(1.0, outer.sk())],
            SpreadMode::Pad,
            Transform::identity(),
        ) else {
            return self.circle(cx, cy, r, inner);
        };
        let paint = Paint { shader, anti_alias: true, ..Default::default() };
        let tf = self.tf();
        self.pm.fill_path(&path, &paint, FillRule::Winding, tf, self.mask.as_ref());
    }

    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, width: f32, c: Col) {
        let mut pb = PathBuilder::new();
        pb.move_to(x0, y0);
        pb.line_to(x1, y1);
        if let Some(p) = pb.finish() {
            self.stroke_path(&p, width, c);
        }
    }

    // ---- text -------------------------------------------------------------

    fn dclip(&self) -> Clip {
        match self.clip {
            Some(r) => Clip(
                (r.x * self.s).floor() as i32,
                (r.y * self.s).floor() as i32,
                (r.right() * self.s).ceil() as i32,
                (r.bottom() * self.s).ceil() as i32,
            ),
            None => Clip(0, 0, i32::MAX, i32::MAX),
        }
    }

    pub fn text_w(&mut self, text: &str, f: FontKind, size: f32) -> f32 {
        self.fonts.width(f, text, size * self.s) / self.s
    }

    /// Height of the line box (ascent − descent) in logical pixels.
    pub fn line_h(&self, f: FontKind, size: f32) -> f32 {
        let px = size * self.s;
        (self.fonts.ascent(f, px) - self.fonts.descent(f, px)) / self.s
    }

    /// Draws text with its line box starting at `top`. Returns the width.
    pub fn text(&mut self, x: f32, top: f32, text: &str, f: FontKind, size: f32, c: Col) -> f32 {
        let px = size * self.s;
        let base = ((top + self.dy) * self.s + self.fonts.ascent(f, px)).round();
        let clip = self.dclip();
        let w = self.fonts.draw(
            self.pm,
            clip,
            (x * self.s).round(),
            base,
            text,
            f,
            px,
            [c.0, c.1, c.2, c.3],
        );
        w / self.s
    }

    /// Text vertically centred on `cy`.
    pub fn text_vc(&mut self, x: f32, cy: f32, text: &str, f: FontKind, size: f32, c: Col) -> f32 {
        let h = self.line_h(f, size);
        self.text(x, cy - h * 0.5, text, f, size, c)
    }

    /// Text centred in `r`.
    pub fn text_center(&mut self, r: R, text: &str, f: FontKind, size: f32, c: Col) {
        let w = self.text_w(text, f, size);
        self.text_vc(r.cx() - w * 0.5, r.cy(), text, f, size, c);
    }

    /// Right-aligned text ending at `right`, vertically centred on `cy`.
    pub fn text_right(&mut self, right: f32, cy: f32, text: &str, f: FontKind, size: f32, c: Col) {
        let w = self.text_w(text, f, size);
        self.text_vc(right - w, cy, text, f, size, c);
    }

    /// Shortens `text` with an ellipsis to fit `max_w`.
    pub fn ellipsize(&mut self, text: &str, f: FontKind, size: f32, max_w: f32) -> String {
        if self.text_w(text, f, size) <= max_w {
            return text.to_string();
        }
        let chars: Vec<char> = text.chars().collect();
        for n in (0..chars.len()).rev() {
            let s: String = chars[..n].iter().collect::<String>() + "…";
            if self.text_w(&s, f, size) <= max_w {
                return s;
            }
        }
        "…".into()
    }

    /// Greedy word wrap.
    pub fn wrap(&mut self, text: &str, f: FontKind, size: f32, max_w: f32) -> Vec<String> {
        let mut lines = Vec::new();
        for para in text.split('\n') {
            let mut line = String::new();
            for word in para.split(' ') {
                let cand =
                    if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
                if !line.is_empty() && self.text_w(&cand, f, size) > max_w {
                    lines.push(std::mem::take(&mut line));
                    line = word.to_string();
                } else {
                    line = cand;
                }
            }
            lines.push(line);
        }
        lines
    }

    /// Draws wrapped text and returns the height used.
    #[allow(clippy::too_many_arguments)]
    pub fn paragraph(
        &mut self,
        x: f32,
        top: f32,
        max_w: f32,
        text: &str,
        f: FontKind,
        size: f32,
        c: Col,
    ) -> f32 {
        let lh = (size * 1.45).round();
        let lines = self.wrap(text, f, size, max_w);
        for (i, l) in lines.iter().enumerate() {
            self.text(x, top + i as f32 * lh, l, f, size, c);
        }
        lines.len() as f32 * lh
    }
}
