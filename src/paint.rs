//! Colors, paints, strokes and blend modes.

use std::fmt;
use std::str::FromStr;

use kurbo::Point;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::serde_util;

/// An sRGB color with straight (non-premultiplied) alpha, 8 bits per channel.
///
/// Serialized as `#RRGGBBAA`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const BLACK: Color = Color::rgb(0, 0, 0);
    pub const WHITE: Color = Color::rgb(255, 255, 255);
    pub const TRANSPARENT: Color = Color::rgba(0, 0, 0, 0);

    /// An opaque color.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Returns the same color with a different alpha.
    pub const fn with_alpha(self, a: u8) -> Self {
        Self { a, ..self }
    }

    /// Formats as `#RRGGBBAA`.
    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
    }

    /// Parses `#RGB`, `#RRGGBB` or `#RRGGBBAA` (the `#` is optional).
    pub fn from_hex(s: &str) -> Option<Self> {
        let s = s.strip_prefix('#').unwrap_or(s);
        if !s.is_ascii() {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).ok();
        match s.len() {
            3 => {
                let nibble = |i: usize| u8::from_str_radix(&s[i..=i], 16).ok().map(|v| v * 17);
                Some(Self::rgb(nibble(0)?, nibble(1)?, nibble(2)?))
            }
            6 => Some(Self::rgb(byte(0)?, byte(2)?, byte(4)?)),
            8 => Some(Self::rgba(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
            _ => None,
        }
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl FromStr for Color {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_hex(s).ok_or_else(|| format!("invalid color: {s:?}"))
    }
}

impl Serialize for Color {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// A color at a position along a gradient.
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct GradientStop {
    /// Position between 0 and 1.
    pub offset: f32,
    pub color: Color,
}

impl GradientStop {
    pub const fn new(offset: f32, color: Color) -> Self {
        Self { offset, color }
    }
}

/// How a shape is filled or stroked. Gradient coordinates are in the node's local space.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Paint {
    Solid {
        color: Color,
    },
    LinearGradient {
        #[serde(with = "serde_util::point")]
        start: Point,
        #[serde(with = "serde_util::point")]
        end: Point,
        stops: Vec<GradientStop>,
    },
    RadialGradient {
        #[serde(with = "serde_util::point")]
        center: Point,
        radius: f64,
        stops: Vec<GradientStop>,
    },
}

impl Paint {
    pub const fn solid(color: Color) -> Self {
        Paint::Solid { color }
    }
}

impl From<Color> for Paint {
    fn from(color: Color) -> Self {
        Paint::solid(color)
    }
}

/// Rule deciding which regions of a self-intersecting path are inside.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FillRule {
    #[default]
    NonZero,
    EvenOdd,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LineJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LineCap {
    #[default]
    Butt,
    Round,
    Square,
}

/// Outline of a shape.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Stroke {
    pub paint: Paint,
    pub width: f64,
    #[serde(default, skip_serializing_if = "serde_util::is_default")]
    pub join: LineJoin,
    #[serde(default, skip_serializing_if = "serde_util::is_default")]
    pub cap: LineCap,
    #[serde(default = "default_miter_limit")]
    pub miter_limit: f64,
    /// Alternating dash and gap lengths. Empty means a solid line.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dash: Vec<f64>,
    #[serde(default, skip_serializing_if = "serde_util::is_zero")]
    pub dash_offset: f64,
}

fn default_miter_limit() -> f64 {
    4.0
}

impl Stroke {
    /// A solid stroke with default joins and caps.
    pub fn new(paint: impl Into<Paint>, width: f64) -> Self {
        Self {
            paint: paint.into(),
            width,
            join: LineJoin::default(),
            cap: LineCap::default(),
            miter_limit: default_miter_limit(),
            dash: Vec::new(),
            dash_offset: 0.0,
        }
    }

    /// Converts to the kurbo stroke style used for outlining and rendering.
    pub fn to_kurbo(&self) -> kurbo::Stroke {
        let mut stroke = kurbo::Stroke::new(self.width)
            .with_join(match self.join {
                LineJoin::Miter => kurbo::Join::Miter,
                LineJoin::Round => kurbo::Join::Round,
                LineJoin::Bevel => kurbo::Join::Bevel,
            })
            .with_caps(match self.cap {
                LineCap::Butt => kurbo::Cap::Butt,
                LineCap::Round => kurbo::Cap::Round,
                LineCap::Square => kurbo::Cap::Square,
            })
            .with_miter_limit(self.miter_limit);
        if !self.dash.is_empty() {
            stroke = stroke.with_dashes(self.dash_offset, self.dash.iter().copied());
        }
        stroke
    }
}

/// How a node is composited with what is below it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip() {
        let c = Color::rgba(0x12, 0xab, 0xff, 0x80);
        assert_eq!(c.to_hex(), "#12abff80");
        assert_eq!(Color::from_hex("#12abff80"), Some(c));
        assert_eq!(Color::from_hex("12ABFF"), Some(c.with_alpha(255)));
        assert_eq!(Color::from_hex("#fff"), Some(Color::WHITE));
        assert_eq!(Color::from_hex("#ffff"), None);
        assert_eq!(Color::from_hex("#ggg"), None);
    }

    #[test]
    fn paint_json_shape() {
        let json = serde_json::to_string(&Paint::solid(Color::BLACK)).unwrap();
        assert_eq!(json, r##"{"type":"solid","color":"#000000ff"}"##);
    }
}
