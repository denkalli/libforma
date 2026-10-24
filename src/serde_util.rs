//! Serde helpers that give kurbo types a compact, readable JSON form.
//!
//! - `Point`, `Vec2`, `Size` → `[x, y]`
//! - `Affine` → `[a, b, c, d, e, f]`
//! - `BezPath` → SVG path data string

use kurbo::{Affine, BezPath, Point, Size};
use serde::{Deserialize, Deserializer, Serializer};

pub mod point {
    use super::*;

    pub fn serialize<S: Serializer>(p: &Point, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq([p.x, p.y])
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Point, D::Error> {
        let [x, y] = <[f64; 2]>::deserialize(d)?;
        Ok(Point::new(x, y))
    }
}

pub mod size {
    use super::*;

    pub fn serialize<S: Serializer>(v: &Size, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq([v.width, v.height])
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Size, D::Error> {
        let [w, h] = <[f64; 2]>::deserialize(d)?;
        Ok(Size::new(w, h))
    }
}

pub mod affine {
    use super::*;

    pub fn serialize<S: Serializer>(a: &Affine, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(a.as_coeffs())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Affine, D::Error> {
        Ok(Affine::new(<[f64; 6]>::deserialize(d)?))
    }
}

pub mod bez_path {
    use super::*;

    pub fn serialize<S: Serializer>(p: &BezPath, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&p.to_svg())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<BezPath, D::Error> {
        let data = String::deserialize(d)?;
        BezPath::from_svg(&data).map_err(serde::de::Error::custom)
    }
}

pub fn is_identity(a: &Affine) -> bool {
    *a == Affine::IDENTITY
}

pub fn is_true(b: &bool) -> bool {
    *b
}

pub fn is_false(b: &bool) -> bool {
    !*b
}

pub fn is_one(v: &f32) -> bool {
    *v == 1.0
}

pub fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

pub fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

pub fn yes() -> bool {
    true
}

pub fn one() -> f32 {
    1.0
}
