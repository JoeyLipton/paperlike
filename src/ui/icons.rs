//! Line icons, drawn on a 16×16 grid and scaled to any size.

use std::f32::consts::{PI, TAU};

use tiny_skia::{Path, PathBuilder};

use super::paint::{Col, Painter, R, rrect_path};
use crate::texture::TextureId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Texture(TextureId),
    Sun,
    Moon,
    Auto,
    Screen,
    Bell,
    Shield,
    Lamp,
    Monitor,
    Clock,
    Palette,
    Ban,
    Keyboard,
    Info,
    Pin,
    Check,
    Close,
    Chevron,
    Refresh,
    Power,
    Warning,
    Plus,
    Minus,
}

/// Maps grid coordinates to screen coordinates and draws with one colour.
struct Pen<'a, 'b> {
    p: &'a mut Painter<'b>,
    x: f32,
    y: f32,
    k: f32,
    c: Col,
}

impl Pen<'_, '_> {
    fn at(&self, x: f32, y: f32) -> (f32, f32) {
        (self.x + (x - 8.0) * self.k, self.y + (y - 8.0) * self.k)
    }

    fn width(&self) -> f32 {
        1.35 * self.k
    }

    fn path(&self, pts: &[(f32, f32)], close: bool) -> Option<Path> {
        let mut pb = PathBuilder::new();
        for (i, &(x, y)) in pts.iter().enumerate() {
            let (px, py) = self.at(x, y);
            if i == 0 {
                pb.move_to(px, py);
            } else {
                pb.line_to(px, py);
            }
        }
        if close {
            pb.close();
        }
        pb.finish()
    }

    fn stroke(&mut self, pts: &[(f32, f32)], close: bool) {
        if let Some(path) = self.path(pts, close) {
            let w = self.width();
            self.p.stroke_path(&path, w, self.c);
        }
    }

    fn line(&mut self, a: (f32, f32), b: (f32, f32)) {
        self.stroke(&[a, b], false);
    }

    fn fill(&mut self, pts: &[(f32, f32)]) {
        if let Some(path) = self.path(pts, true) {
            self.p.fill_path(&path, self.c);
        }
    }

    fn dot(&mut self, x: f32, y: f32, r: f32) {
        let (px, py) = self.at(x, y);
        self.p.circle(px, py, r * self.k, self.c);
    }

    fn ring(&mut self, x: f32, y: f32, r: f32) {
        let (px, py) = self.at(x, y);
        let w = self.width();
        self.p.circle_stroke(px, py, r * self.k, w, self.c);
    }

    fn frame(&mut self, x: f32, y: f32, w: f32, h: f32, radius: f32) {
        let (px, py) = self.at(x, y);
        let r = R::new(px, py, w * self.k, h * self.k);
        if let Some(path) = rrect_path(r, radius * self.k) {
            let sw = self.width();
            self.p.stroke_path(&path, sw, self.c);
        }
    }

    fn curve(&mut self, pts: [(f32, f32); 4], width: f32) {
        let [a, b, c, d] = pts.map(|(x, y)| self.at(x, y));
        let mut pb = PathBuilder::new();
        pb.move_to(a.0, a.1);
        pb.cubic_to(b.0, b.1, c.0, c.1, d.0, d.1);
        if let Some(path) = pb.finish() {
            self.p.stroke_path(&path, width * self.k, self.c);
        }
    }

    fn fill_curve(&mut self, start: (f32, f32), segs: &[Seg]) {
        let (x, y) = self.at(start.0, start.1);
        let mut pb = PathBuilder::new();
        pb.move_to(x, y);
        for seg in segs {
            match *seg {
                Seg::Line(x, y) => {
                    let (x, y) = self.at(x, y);
                    pb.line_to(x, y);
                }
                Seg::Cubic(a, b, c) => {
                    let [a, b, c] = [a, b, c].map(|(x, y)| self.at(x, y));
                    pb.cubic_to(a.0, a.1, b.0, b.1, c.0, c.1);
                }
            }
        }
        pb.close();
        if let Some(path) = pb.finish() {
            self.p.fill_path(&path, self.c);
        }
    }
}

enum Seg {
    Line(f32, f32),
    Cubic((f32, f32), (f32, f32), (f32, f32)),
}

/// Points along an arc, angles in radians.
fn arc(cx: f32, cy: f32, r: f32, a0: f32, a1: f32) -> Vec<(f32, f32)> {
    const STEPS: usize = 28;
    (0..=STEPS)
        .map(|i| {
            let a = a0 + (a1 - a0) * i as f32 / STEPS as f32;
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect()
}

pub fn draw(p: &mut Painter, icon: Icon, cx: f32, cy: f32, size: f32, c: Col) {
    let mut pen = Pen { p, x: cx, y: cy, k: size / 16.0, c };
    match icon {
        Icon::Texture(t) => texture(&mut pen, t),
        Icon::Sun => {
            pen.ring(8.0, 8.0, 2.7);
            for i in 0..8 {
                let a = i as f32 * PI / 4.0;
                let (s, c) = a.sin_cos();
                pen.line((8.0 + 4.8 * c, 8.0 + 4.8 * s), (8.0 + 6.3 * c, 8.0 + 6.3 * s));
            }
        }
        Icon::Moon => {
            // A disc with a smaller disc bitten out of its upper right. The
            // outline is the outer arc between the two intersection points,
            // then back along the bite.
            let ((x1, y1), r1) = ((8.0f32, 8.0f32), 5.8f32);
            let ((x2, y2), r2) = ((11.2f32, 5.2f32), 4.9f32);
            let (dx, dy) = (x2 - x1, y2 - y1);
            let d = dx.hypot(dy);
            let a = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
            let towards = dy.atan2(dx);
            let half1 = (a / r1).acos();
            let half2 = ((d - a) / r2).acos();
            let mut pts = arc(x1, y1, r1, towards + half1, towards + TAU - half1);
            pts.extend(arc(x2, y2, r2, towards + PI + half2, towards + PI - half2));
            pen.fill(&pts);
        }
        Icon::Auto => {
            pen.ring(8.0, 8.0, 5.8);
            pen.fill(&arc(8.0, 8.0, 5.8, PI / 2.0, PI * 1.5));
        }
        Icon::Screen => {
            pen.frame(1.8, 2.8, 12.4, 8.6, 1.6);
            pen.fill(&arc(8.0, 7.1, 2.4, PI / 2.0, PI * 1.5));
            pen.ring(8.0, 7.1, 2.4);
            pen.line((5.5, 13.6), (10.5, 13.6));
        }
        Icon::Bell => {
            pen.fill_curve(
                (3.6, 11.2),
                &[
                    Seg::Line(4.4, 7.0),
                    Seg::Cubic((4.4, 4.4), (6.0, 2.6), (8.0, 2.6)),
                    Seg::Cubic((10.0, 2.6), (11.6, 4.4), (11.6, 7.0)),
                    Seg::Line(12.4, 11.2),
                ],
            );
            pen.dot(8.0, 13.2, 1.4);
        }
        Icon::Shield => pen.fill_curve(
            (8.0, 1.8),
            &[
                Seg::Line(13.2, 3.8),
                Seg::Line(13.2, 7.6),
                Seg::Cubic((13.2, 11.0), (10.6, 13.3), (8.0, 14.3)),
                Seg::Cubic((5.4, 13.3), (2.8, 11.0), (2.8, 7.6)),
                Seg::Line(2.8, 3.8),
            ],
        ),
        Icon::Lamp => {
            pen.fill(&[(5.2, 2.2), (10.4, 2.2), (13.0, 7.6), (2.6, 7.6)]);
            pen.line((7.8, 7.6), (7.8, 13.2));
            pen.line((4.6, 13.8), (11.0, 13.8));
            pen.dot(7.8, 9.0, 1.1);
        }
        Icon::Monitor => {
            pen.frame(1.8, 2.6, 12.4, 8.8, 1.6);
            pen.line((8.0, 11.4), (8.0, 13.6));
            pen.line((5.2, 13.6), (10.8, 13.6));
        }
        Icon::Clock => {
            pen.ring(8.0, 8.0, 5.9);
            pen.stroke(&[(8.0, 4.6), (8.0, 8.0), (10.6, 9.6)], false);
        }
        Icon::Palette => {
            pen.ring(8.0, 8.0, 5.9);
            pen.dot(5.6, 6.4, 1.0);
            pen.dot(9.0, 4.9, 1.0);
            pen.dot(11.2, 7.6, 1.0);
            pen.dot(7.0, 10.6, 1.4);
        }
        Icon::Ban => {
            pen.ring(8.0, 8.0, 5.9);
            pen.line((3.9, 3.9), (12.1, 12.1));
        }
        Icon::Keyboard => {
            pen.frame(1.4, 3.8, 13.2, 8.6, 1.6);
            for x in [4.2, 6.8, 9.4, 12.0] {
                pen.dot(x, 6.8, 0.75);
            }
            pen.line((5.2, 9.6), (10.8, 9.6));
        }
        Icon::Info => {
            pen.ring(8.0, 8.0, 5.9);
            pen.line((8.0, 7.4), (8.0, 11.0));
            pen.dot(8.0, 5.0, 0.95);
        }
        Icon::Pin => {
            pen.dot(8.0, 5.6, 3.0);
            pen.line((8.0, 8.0), (8.0, 14.2));
        }
        Icon::Check => pen.stroke(&[(3.4, 8.6), (6.6, 11.6), (12.6, 4.6)], false),
        Icon::Close => {
            pen.line((4.4, 4.4), (11.6, 11.6));
            pen.line((11.6, 4.4), (4.4, 11.6));
        }
        Icon::Chevron => pen.stroke(&[(6.0, 3.4), (10.6, 8.0), (6.0, 12.6)], false),
        Icon::Refresh => {
            let (a0, a1) = (-40f32.to_radians(), 250f32.to_radians());
            pen.stroke(&arc(8.0, 8.0, 5.2, a0, a1), false);
            let (ex, ey) = (8.0 + 5.2 * a0.cos(), 8.0 + 5.2 * a0.sin());
            pen.fill(&[(ex + 1.8, ey - 2.6), (ex + 1.4, ey + 1.6), (ex - 2.6, ey - 0.4)]);
        }
        Icon::Power => {
            pen.stroke(&arc(8.0, 8.6, 5.4, -50f32.to_radians(), 230f32.to_radians()), false);
            pen.line((8.0, 1.8), (8.0, 7.6));
        }
        Icon::Warning => {
            pen.stroke(&[(8.0, 2.0), (14.4, 13.4), (1.6, 13.4)], true);
            pen.line((8.0, 6.4), (8.0, 9.6));
            pen.dot(8.0, 11.6, 0.9);
        }
        Icon::Plus => {
            pen.line((8.0, 3.0), (8.0, 13.0));
            pen.line((3.0, 8.0), (13.0, 8.0));
        }
        Icon::Minus => pen.line((3.0, 8.0), (13.0, 8.0)),
    }
}

fn texture(pen: &mut Pen, t: TextureId) {
    match t {
        TextureId::Matte | TextureId::CoarseMatte => {
            let r = if t == TextureId::Matte { 0.85 } else { 1.15 };
            for (x, y) in [
                (5.5, 2.5),
                (10.5, 2.5),
                (3.0, 6.2),
                (8.0, 6.2),
                (13.0, 6.2),
                (5.5, 9.8),
                (10.5, 9.8),
                (3.0, 13.5),
                (8.0, 13.5),
                (13.0, 13.5),
            ] {
                pen.dot(x, y, r);
            }
        }
        TextureId::Crosshatch => {
            pen.line((2.5, 5.0), (11.0, 13.5));
            pen.line((5.0, 2.5), (13.5, 11.0));
            pen.line((2.5, 11.0), (11.0, 2.5));
            pen.line((5.0, 13.5), (13.5, 5.0));
        }
        TextureId::Parchment => {
            for y in [4.0, 8.0, 12.0] {
                let wave: Vec<_> = (0..=24)
                    .map(|i| {
                        let x = 2.0 + 12.0 * i as f32 / 24.0;
                        (x, y + 0.9 * (x * 1.05).sin())
                    })
                    .collect();
                pen.stroke(&wave, false);
            }
        }
        TextureId::Linen => {
            pen.line((5.0, 3.8), (11.0, 3.8));
            pen.line((2.0, 6.6), (14.0, 6.6));
            pen.line((4.0, 9.4), (12.0, 9.4));
            pen.line((6.0, 12.2), (10.0, 12.2));
        }
        TextureId::ColdPress => {
            pen.frame(2.5, 2.5, 11.0, 11.0, 3.2);
            for (x, y) in [(6.0, 6.2), (10.2, 6.6), (6.8, 10.2), (10.4, 10.4)] {
                pen.dot(x, y, 0.8);
            }
        }
        TextureId::RicePaper => {
            pen.curve([(3.0, 13.0), (2.0, 5.0), (14.0, 11.0), (13.0, 3.0)], 1.55);
        }
        TextureId::Frost => {
            pen.stroke(&[(8.0, 1.6), (14.4, 8.0), (8.0, 14.4), (1.6, 8.0)], true);
            for (x, y) in [(8.0, 5.0), (5.0, 8.0), (8.0, 8.0), (11.0, 8.0), (8.0, 11.0)] {
                pen.dot(x, y, 0.75);
            }
        }
        TextureId::Felt => {
            for (a, b) in [
                ((2.5, 4.5), (6.0, 3.0)),
                ((8.0, 3.2), (12.5, 5.0)),
                ((3.5, 8.6), (7.5, 9.6)),
                ((9.5, 8.8), (13.5, 7.4)),
                ((2.5, 12.6), (6.0, 11.4)),
                ((8.5, 12.0), (12.5, 13.2)),
            ] {
                pen.line(a, b);
            }
        }
        TextureId::Graphite => {
            pen.line((3.5, 2.5), (3.5, 13.5));
            for y in [4.5, 8.0, 11.5] {
                pen.line((6.0, y), (13.5, y));
            }
        }
    }
}
