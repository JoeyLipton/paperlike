//! Desk Lamp: a single elliptical radial gradient over the texture. It starts
//! as a warm white hot spot near the lamp, fades through the lamp colour to a
//! dim brown, then to nothing. The whole layer is drawn at the "dimming"
//! opacity.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LampPosition {
    TopLeft,
    Top,
    TopRight,
    Left,
    Right,
}

impl LampPosition {
    pub const ALL: [LampPosition; 5] = [
        LampPosition::TopLeft,
        LampPosition::Top,
        LampPosition::TopRight,
        LampPosition::Left,
        LampPosition::Right,
    ];

    pub fn label(self) -> &'static str {
        match self {
            LampPosition::TopLeft => "Top left",
            LampPosition::Top => "Top",
            LampPosition::TopRight => "Top right",
            LampPosition::Left => "Left",
            LampPosition::Right => "Right",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LampPreset {
    Candle,
    LateNight,
    SideLamp,
    Spotlight,
    Sunset,
    Ambient,
}

/// Lamp settings. `warmth`, `spread` and `dimming` are percentages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct LampParams {
    pub warmth: u8,
    pub spread: u8,
    /// Opacity of the whole lamp layer.
    pub dimming: u8,
    pub position: LampPosition,
}

impl Default for LampParams {
    fn default() -> Self {
        LampPreset::Ambient.params()
    }
}

impl LampPreset {
    pub const ALL: [LampPreset; 6] = [
        LampPreset::Candle,
        LampPreset::LateNight,
        LampPreset::SideLamp,
        LampPreset::Spotlight,
        LampPreset::Sunset,
        LampPreset::Ambient,
    ];

    pub fn name(self) -> &'static str {
        match self {
            LampPreset::Candle => "Candle",
            LampPreset::LateNight => "Late Night",
            LampPreset::SideLamp => "Side Lamp",
            LampPreset::Spotlight => "Spotlight",
            LampPreset::Sunset => "Sunset",
            LampPreset::Ambient => "Ambient",
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            LampPreset::Candle => "candle",
            LampPreset::LateNight => "late-night",
            LampPreset::SideLamp => "side-lamp",
            LampPreset::Spotlight => "spotlight",
            LampPreset::Sunset => "sunset",
            LampPreset::Ambient => "ambient",
        }
    }

    /// Parses a slug or display name, ignoring case and punctuation.
    pub fn parse(s: &str) -> Option<Self> {
        let key: String = s
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        LampPreset::ALL.into_iter().find(|p| p.slug().replace('-', "") == key)
    }

    pub fn params(self) -> LampParams {
        use LampPosition::*;
        let p =
            |warmth, spread, dimming, position| LampParams { warmth, spread, dimming, position };
        match self {
            LampPreset::Candle => p(90, 25, 33, Top),
            LampPreset::LateNight => p(100, 25, 35, TopLeft),
            LampPreset::SideLamp => p(55, 45, 25, Right),
            LampPreset::Spotlight => p(20, 60, 15, TopRight),
            LampPreset::Sunset => p(75, 90, 37, Top),
            LampPreset::Ambient => p(47, 82, 12, TopLeft),
        }
    }

    /// The preset with exactly these settings, if any.
    pub fn matching(p: &LampParams) -> Option<LampPreset> {
        LampPreset::ALL.into_iter().find(|x| x.params() == *p)
    }
}

/// Lamp colour for a warmth percentage, as sRGB bytes.
pub fn color(warmth: u8) -> [u8; 3] {
    let w = warmth.min(100) as f32 / 100.0;
    [
        (250.0 - w * 35.0).round() as u8,
        (210.0 - w * 100.0).round() as u8,
        (160.0 - w * 140.0).round() as u8,
    ]
}

/// A colour stop: position 0–1 and straight (non-premultiplied) sRGBA.
pub type Stop = (f32, [f32; 4]);

/// An elliptical gradient centred on `(cx, cy)` with radii `rx`, `ry`.
pub struct Gradient {
    pub cx: f32,
    pub cy: f32,
    pub rx: f32,
    pub ry: f32,
    pub stops: Vec<Stop>,
    pub opacity: f32,
}

/// The lamp gradient for a `w`×`h` screen.
pub fn gradient(p: &LampParams, w: f32, h: f32) -> Gradient {
    let [r, g, b] = color(p.warmth).map(f32::from);
    let s = p.spread.min(100) as f32 / 100.0;
    let inner = (15.0 + s * 25.0).round() / 100.0;
    let outer = (65.0 + s * 30.0).round() / 100.0;
    let (rx, ry, cx, cy) = match p.position {
        LampPosition::TopLeft => (1.5 * w, 1.5 * h, 0.0, 0.0),
        LampPosition::TopRight => (1.5 * w, 1.5 * h, w, 0.0),
        LampPosition::Left => (w, 1.5 * h, 0.0, h * 0.5),
        LampPosition::Right => (w, 1.5 * h, w, h * 0.5),
        LampPosition::Top => (1.5 * w, h, w * 0.5, 0.0),
    };
    let rgba = |r: f32, g: f32, b: f32, a: f32| [r / 255.0, g / 255.0, b / 255.0, a];
    let edge = [(r * 0.75).round(), (g * 0.5).round(), (b * 0.25).round()];
    Gradient {
        cx,
        cy,
        rx,
        ry,
        stops: vec![
            (0.0, rgba(255.0, 248.0, 235.0, 1.0)),
            (inner, rgba(r, g, b, 1.0)),
            (outer, rgba(edge[0], edge[1], edge[2], 1.0)),
            // Fade out without shifting hue. XRender interpolates straight
            // colour, so the transparent end keeps the previous stop's RGB.
            (1.0, rgba(edge[0], edge[1], edge[2], 0.0)),
        ],
        opacity: p.dimming.min(100) as f32 / 100.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warmth_colours() {
        assert_eq!(color(0), [250, 210, 160]);
        assert_eq!(color(100), [215, 110, 20]);
        assert_eq!(color(47), [234, 163, 94]);
    }

    #[test]
    fn parse_presets() {
        assert_eq!(LampPreset::parse("late-night"), Some(LampPreset::LateNight));
        assert_eq!(LampPreset::parse("Side Lamp"), Some(LampPreset::SideLamp));
        assert_eq!(LampPreset::parse("nope"), None);
        assert_eq!(LampPreset::matching(&LampPreset::Sunset.params()), Some(LampPreset::Sunset));
    }
}
