//! ro-opc —— RustOffice 的 OPC / OOXML 包层。
//!
//! 这一层只做三件事，别的一概不做：
//!   1. ZIP 容器读写（本地文件头 + 中央目录 + EOCD；写入用 deflate，必要时回落 store）
//!   2. CRC32（写完自己能验，读入也要验）
//!   3. OPC 元数据：`[Content_Types].xml` 与 `_rels/*.rels`
//!
//! 设计边界：
//!   · 不用第三方 zip 库：包层是 OOXML 的地基，未知部件的原样保留要靠自己控制。
//!   · 不解析 WordprocessingML 语义——那是 `ro-word` 的事。

pub mod crc32;
pub mod opc;
pub mod preserve;
pub mod zip;

pub use opc::{
    normalize_part_name, resolve_target, ContentTypes, Package, Part, Relationship, Relationships,
};
pub use zip::{ZipArchive, ZipEntry, ZipWriter};

use thiserror::Error;

/// 本层所有可预期失败的分类。保留"部分事实"：出错时尽量带上已经读到的信息。
#[derive(Debug, Error)]
pub enum OpcError {
    #[error("ZIP 结构损坏：{0}")]
    BadZip(String),
    #[error("不是 OOXML ZIP 包：{0}")]
    NotAZip(String),
    #[error("ZIP64 暂不支持：{0}")]
    Zip64Unsupported(String),
    #[error("不支持的压缩方法 {0}（只支持 0=store / 8=deflate）")]
    UnsupportedMethod(u16),
    #[error("条目 {name} 解压失败：{source}")]
    Inflate {
        name: String,
        #[source]
        source: std::io::Error,
    },
    #[error("条目 {name} CRC 不匹配：头部 {expected:08x}，实算 {actual:08x}")]
    CrcMismatch {
        name: String,
        expected: u32,
        actual: u32,
    },
    #[error("OOXML 部件名非法：{0}")]
    BadPartName(String),
    #[error("超出解压资源限制（防 ZIP 炸弹）：{0}")]
    ResourceLimitExceeded(String),
    #[error("XML 解析错误：{0}")]
    Xml(String),
    #[error("IO 错误：{0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, OpcError>;
