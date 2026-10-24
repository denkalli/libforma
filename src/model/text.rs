use serde::{Deserialize, Serialize};

use crate::paint::Color;
use crate::serde_util;

/// A block of text.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct TextNode {
    pub content: String,
    pub style: TextStyle,
    #[serde(default, skip_serializing_if = "serde_util::is_default")]
    pub align: TextAlign,
    /// Width at which lines wrap. `None` means the text grows horizontally without wrapping.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    /// Fixed height of the text box. `None` means the box is as tall as the text. Only used
    /// together with `width`; text that does not fit overflows the box.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    /// Where the text sits in a box of fixed height.
    #[serde(default, skip_serializing_if = "serde_util::is_default")]
    pub vertical_align: TextVerticalAlign,
}

/// How a text box is sized, from its `width` and `height`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TextSizing {
    /// One line per paragraph; the box grows to the right.
    AutoWidth,
    /// Lines wrap at the width; the box grows down.
    AutoHeight,
    /// Both width and height are set.
    Fixed,
}

impl TextNode {
    pub fn new(content: impl Into<String>, style: TextStyle) -> Self {
        Self {
            content: content.into(),
            style,
            align: TextAlign::Start,
            width: None,
            height: None,
            vertical_align: TextVerticalAlign::Top,
        }
    }

    pub fn sizing(&self) -> TextSizing {
        match (self.width, self.height) {
            (None, _) => TextSizing::AutoWidth,
            (Some(_), None) => TextSizing::AutoHeight,
            (Some(_), Some(_)) => TextSizing::Fixed,
        }
    }
}

/// Character and paragraph style applied to a whole text node.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct TextStyle {
    /// A CSS-style family list, e.g. `"Inter, sans-serif"`.
    pub font_family: String,
    pub font_size: f64,
    /// 100 (thin) to 900 (black); 400 is regular, 700 is bold.
    pub font_weight: u16,
    #[serde(skip_serializing_if = "serde_util::is_false")]
    pub italic: bool,
    pub color: Color,
    /// Line height as a multiple of the font size.
    pub line_height: f32,
    /// Extra space between letters, in pixels.
    #[serde(skip_serializing_if = "is_zero_f32")]
    pub letter_spacing: f32,
}

fn is_zero_f32(v: &f32) -> bool {
    *v == 0.0
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font_family: "sans-serif".into(),
            font_size: 16.0,
            font_weight: 400,
            italic: false,
            color: Color::BLACK,
            line_height: 1.2,
            letter_spacing: 0.0,
        }
    }
}

/// Vertical position of the text in a box of fixed height.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextVerticalAlign {
    #[default]
    Top,
    Middle,
    Bottom,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextAlign {
    #[default]
    Start,
    Center,
    End,
    Justify,
}
