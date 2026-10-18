//! 矢量场景树节点与属性模型 (Scene Graph Model)

use crate::color::Color;
use crate::geometry::{Geometry, PresetGeometry};

/// 2D 变换参数（以 EMU 为单位：1 pt = 12700 EMU, 1 in = 914400 EMU）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Transform2D {
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
    pub rotation_degrees: i32,
    pub flip_h: bool,
    pub flip_v: bool,
}

impl Transform2D {
    pub fn new(x: i64, y: i64, width: i64, height: i64) -> Self {
        Self {
            x,
            y,
            width,
            height,
            rotation_degrees: 0,
            flip_h: false,
            flip_v: false,
        }
    }
}

/// 填充属性
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fill {
    None,
    Solid(Color),
}

/// 描边属性
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stroke {
    pub color: Color,
    pub width_emu: i64,
    pub dashed: bool,
}

impl Default for Stroke {
    fn default() -> Self {
        Self {
            color: Color::rgb(0x1F, 0x49, 0x7D),
            width_emu: 12_700, // 1pt
            dashed: false,
        }
    }
}

/// 简单文本体（用于形状内部包含的说明文字）
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TextBody {
    pub paragraphs: Vec<TextParagraph>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TextParagraph {
    pub runs: Vec<TextRun>,
    pub align: TextAlign,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextRun {
    pub text: String,
    pub font_size_hundredth_pt: u32, // 2400 = 24pt
    pub bold: bool,
    pub italic: bool,
    pub color: Option<Color>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

/// 场景树通用形状 (Shape)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shape {
    pub id: u32,
    pub name: String,
    pub transform: Transform2D,
    pub geometry: Geometry,
    pub fill: Fill,
    pub stroke: Option<Stroke>,
    pub text_body: Option<TextBody>,
}

impl Shape {
    pub fn new(id: u32, name: impl Into<String>, transform: Transform2D, geom: Geometry) -> Self {
        Self {
            id,
            name: name.into(),
            transform,
            geometry: geom,
            fill: Fill::None,
            stroke: None,
            text_body: None,
        }
    }

    pub fn rectangle(id: u32, x: i64, y: i64, w: i64, h: i64, fill_color: Color) -> Self {
        let mut s = Self::new(id, "Rectangle", Transform2D::new(x, y, w, h), Geometry::Preset(PresetGeometry::Rectangle));
        s.fill = Fill::Solid(fill_color);
        s
    }

    pub fn rounded_rectangle(id: u32, x: i64, y: i64, w: i64, h: i64, fill_color: Color) -> Self {
        let mut s = Self::new(id, "RoundedRectangle", Transform2D::new(x, y, w, h), Geometry::Preset(PresetGeometry::RoundedRectangle));
        s.fill = Fill::Solid(fill_color);
        s
    }

    pub fn ellipse(id: u32, x: i64, y: i64, w: i64, h: i64, fill_color: Color) -> Self {
        let mut s = Self::new(id, "Ellipse", Transform2D::new(x, y, w, h), Geometry::Preset(PresetGeometry::Ellipse));
        s.fill = Fill::Solid(fill_color);
        s
    }
}

/// 场景节点树
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DrawingNode {
    Shape(Shape),
    Group {
        id: u32,
        name: String,
        transform: Transform2D,
        children: Vec<DrawingNode>,
    },
}
