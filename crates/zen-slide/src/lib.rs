//! Zen PresentationML 引擎 (ECMA-376 Part 1 Chapter 19).
//! 提供 PPTX 的无头文档模型、生成与读取。

pub mod model;
pub mod preserve;
pub mod read;
pub mod write;

pub use model::*;
pub use read::{read_pptx, read_pptx_bytes};
pub use write::{build_package, write_pptx};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SlideError {
    #[error("OPC 容器错误: {0}")]
    Opc(#[from] zen_opc::OpcError),
    #[error("XML 解析错误: {0}")]
    Xml(#[from] roxmltree::Error),
    #[error("PresentationML 结构错误: {0}")]
    Invalid(String),
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, SlideError>;
