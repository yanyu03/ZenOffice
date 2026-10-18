//! 几何形态定义 (Preset Geometry 对标 DrawingML a:prstGeom)

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Geometry {
    Preset(PresetGeometry),
    CustomPath(String),
}

/// 预定义几何形状（ECMA-376 DrawingML 标准形状库）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetGeometry {
    Rectangle,
    RoundedRectangle,
    Ellipse,
    Triangle,
    Diamond,
    RightArrow,
    LeftArrow,
    Star5,
    Line,
}

impl PresetGeometry {
    pub fn xml_name(&self) -> &'static str {
        match self {
            Self::Rectangle => "rect",
            Self::RoundedRectangle => "roundRect",
            Self::Ellipse => "ellipse",
            Self::Triangle => "triangle",
            Self::Diamond => "diamond",
            Self::RightArrow => "rightArrow",
            Self::LeftArrow => "leftArrow",
            Self::Star5 => "star5",
            Self::Line => "line",
        }
    }
}
