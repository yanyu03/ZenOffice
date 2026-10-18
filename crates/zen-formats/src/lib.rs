//! File-format boundary: engine snapshots -> OOXML / delimited interchange.
//! UI code never assembles XML, ZIP relationships or image resources itself.
mod formula_cache;
pub mod opc;
pub mod xlsx;
pub mod spreadsheet;

use std::{io::Write, path::Path};
use zen_engine::{csv_io::CsvIO, sheet::Sheet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Xlsx,
    Csv,
    Tsv,
}

impl OutputFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Xlsx => "xlsx",
            Self::Csv => "csv",
            Self::Tsv => "tsv",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Xlsx => "Excel 工作簿 (.xlsx)",
            Self::Csv => "CSV UTF-8 (.csv)",
            Self::Tsv => "制表符文本 UTF-8 (.tsv)",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FormatError {
    #[error("OOXML 输出失败：{0}")]
    Xlsx(#[from] rust_xlsxwriter::XlsxError),
    #[error("图片编码失败：{0}")]
    Image(#[from] image::ImageError),
    #[error("OOXML ZIP 包处理失败：{0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("OOXML XML 处理失败：{0}")]
    Xml(#[from] roxmltree::Error),
    #[error("文件写入失败：{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Invalid(String),
}

pub struct ExportResult {
    pub bytes: Vec<u8>,
    pub warnings: Vec<String>,
}

/// Snapshot calculation leaves the user's sheet and undo history untouched.
pub fn export_sheet(sheet: &Sheet, format: OutputFormat) -> Result<ExportResult, FormatError> {
    match format {
        OutputFormat::Xlsx => xlsx::export_workbook(&[sheet]),
        OutputFormat::Csv | OutputFormat::Tsv => {
            let mut snapshot = sheet.clone();
            snapshot.recalculate();
            let separator = if format == OutputFormat::Csv {
                ','
            } else {
                '\t'
            };
            let text = CsvIO::export_delimited(&snapshot, separator);
            let mut bytes = vec![0xef, 0xbb, 0xbf];
            bytes.extend_from_slice(text.as_bytes());
            Ok(ExportResult { bytes, warnings: vec!["文本格式仅保存当前工作表的计算结果；公式、样式、图片、链接、合并和页面设置请使用 XLSX 保存。".into()] })
        }
    }
}

/// Write beside the destination, flush, then replace. A failed export or locked
/// destination leaves the previous file intact. Parent folders must exist.
pub fn save_atomic(path: &Path, bytes: &[u8]) -> Result<(), FormatError> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|e| FormatError::Io(e.error))?;
    Ok(())
}
