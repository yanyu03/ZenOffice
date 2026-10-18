//! # Zen Drawing (对标 Office 2024 LTSC 之 OART.DLL + GFX.DLL + MSOSVG.DLL)
//!
//! 统一 DrawingML 矢量场景树与图形中台 (Unified Scene Graph)。
//!
//! ## 核心抽象
//! - [`Transform2D`]: 2D 仿射变换与边界盒 (EMU 单位: 1 pt = 12,700 EMU, 1 in = 914,400 EMU)
//! - [`Color`]: RGB/主题颜色表示
//! - [`Fill`]: 纯色填充、渐变填充与透明度
//! - [`Stroke`]: 边框描边属性
//! - [`Geometry`]: 预定义几何形状 (矩形、圆角矩形、椭圆、线条、箭头等)
//! - [`DrawingNode`]: 场景树节点 (形状、文本块、群组)
//! - [`DrawingMLWriter`]: 统一将场景节点渲染为符合 ECMA-376 规范的 DrawingML XML 结构
//! - [`SvgExporter`]: 将矢量场景树直接转换为标准 W3C SVG 格式 (对标 MSOSVG.DLL)

pub mod color;
pub mod geometry;
pub mod scene;
pub mod svg;
pub mod writer;

pub use color::{Color, ThemeColorSlot};
pub use geometry::{Geometry, PresetGeometry};
pub use scene::{DrawingNode, Fill, Shape, Stroke, TextBody, Transform2D};
pub use svg::SvgExporter;
pub use writer::DrawingMLWriter;

pub const EMU_PER_INCH: i64 = 914_400;
pub const EMU_PER_PT: i64 = 12_700;
pub const EMU_PER_MM: i64 = 36_000;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("XML format error: {0}")]
    Xml(String),
    #[error("Invalid coordinate or dimension: {0}")]
    InvalidGeometry(String),
    #[error("Invalid color format: {0}")]
    InvalidColor(String),
    #[error("Unsupported geometry: {0}")]
    UnsupportedGeometry(String),
}

pub type Result<T> = std::result::Result<T, Error>;
