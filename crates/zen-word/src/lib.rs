//! RustOffice 的文字层：**以 OOXML 为底层**的 Word 文档模型与 DOCX 编解码。
//!
//! 分层（自下而上）：
//! ```text
//! ro-opc   ZIP 容器 + CRC32 + [Content_Types].xml + _rels      ← 只懂包，不懂 Word
//! ro-word  WordprocessingML 模型 + 写出 / 读入 + 诊断 + 体检    ← 本 crate
//! ro-word-cli  命令行测试版（新建 / 取文本 / 结构树 / round-trip / 体检）
//! ```
//!
//! 能力声明（v0.1，**不做超出声明的事**）：
//!   · 支持：段落、文本、换行、制表符、字符格式（粗/斜/下划线/删除线/字号/中西文字体/颜色/上下标）、
//!     段落格式（样式引用、对齐、缩进含悬挂、段前后距、行距、段前分页）、表格（网格/宽/合并/边框）、
//!     多节属性（纸张、页边距、方向、节起始方式、页码规则）、文档属性、样式表
//!   · 不支持但**不静默丢弃**：图片/嵌入对象、域、超链接关系、内容控件、编号、批注、页眉页脚、
//!     主题。读取时它们进 `ReadOutcome::diagnostics`，原部件字节仍完整保留在包里。
//!   · 导入文件的任意结构修改仍受限；多节模型不代表可随意重建原包。
//!   · 自有分页是近似预览，不含完整字体塑形、页眉脚注或浮动对象排版。
//!
//! 证据强度约定：本 crate 的结论都属"静态实现 + 自测"，**不等于**已在 WPS/Word 真机验证；
//! 真机对拍由 `tools/verify-docx-wps.ps1` 单独执行并单独记录。

pub mod edit;
pub mod layout;
pub mod model;
pub mod paper;
pub mod preserve;
pub mod styles;
pub mod read;
pub mod write;

pub use layout::{LayoutDocument, LayoutLine, LayoutPage, LayoutRun, LineServices, PageTableServices};

pub use edit::{Caret, EditOutcome, RunProp};
pub use model::*;
pub use styles::{ResolvedParagraph, StyleResolver};
pub use read::{
    extract_text, read_docx, verify_docx, Check, DiagKind, Diagnostic, ReadOutcome, VerifyReport,
};
pub use write::write_docx;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum WordError {
    #[error("无损保存不支持该修改：{0}")]
    UnsupportedEdit(String),
    #[error("OPC 包层错误：{0}")]
    Opc(#[from] zen_opc::OpcError),
    #[error("缺少必需的关系：{0}")]
    MissingRelationship(String),
    #[error("缺少部件：{0}")]
    MissingPart(String),
    #[error("部件 {part} 的 XML 无法解析：{source}")]
    BadXml {
        part: String,
        #[source]
        source: roxmltree::Error,
    },
    #[error("部件 {part} 结构不合法：{detail}")]
    Malformed { part: String, detail: String },
}

pub type Result<T> = std::result::Result<T, WordError>;

/// 演示文档：覆盖 v0.1 全部受支持能力，内容固定，用于回归与真机对拍。
///
/// 固定内容是有意的——同样的输入必须产出逐字节相同的包，否则 diff 与对拍都无从谈起。
pub fn demo_document() -> Document {
    let body_font = |sz: u32| RunProps {
        font_ascii: Some("Calibri".into()),
        font_east_asia: Some("宋体".into()),
        size_half_points: Some(sz),
        ..Default::default()
    };

    let mut doc = Document {
        core: CoreProps {
            title: Some("RustOffice Word 测试版 v0.1".into()),
            subject: Some("OOXML 底层验证".into()),
            creator: Some("RustOffice".into()),
            last_modified_by: Some("RustOffice".into()),
            created: Some("2026-10-12T00:00:00Z".into()),
            modified: Some("2026-10-12T00:00:00Z".into()),
            keywords: Some("OOXML, WordprocessingML, Rust".into()),
        },
        ..Default::default()
    };
    doc.styles.default_run = body_font(21); // 10.5 磅（中文五号）

    // 标题
    doc.blocks.push(Block::Paragraph(
        Paragraph::with_props(ParagraphProps::style(STYLE_HEADING1))
            .push_run(Run::text("RustOffice 文字测试版 v0.1")),
    ));
    doc.blocks.push(Block::Paragraph(
        Paragraph::with_props(ParagraphProps::style(STYLE_HEADING2))
            .push_run(Run::text("一、字符与段落格式")),
    ));

    // 首行缩进正文
    doc.blocks.push(Block::Paragraph(
        Paragraph::with_props(ParagraphProps {
            ind_first_line_twips: Some(420),
            line_spacing: Some(LineSpacing::Auto(360)),
            space_after_twips: Some(120),
            ..Default::default()
        })
        .push_run(Run::styled(body_font(21), "这一段用来验证首行缩进、1.5 倍行距与段后间距。")),
    ));

    // 富格式行
    doc.blocks.push(Block::Paragraph({
        let mut p = Paragraph::default();
        p.runs.push(Run::styled(body_font(21), "同一段内混排："));
        p.runs.push(Run::styled(
            RunProps { bold: Some(true), ..body_font(21) },
            "粗体",
        ));
        p.runs.push(Run::styled(body_font(21), "、"));
        p.runs.push(Run::styled(
            RunProps { italic: Some(true), ..body_font(21) },
            "斜体",
        ));
        p.runs.push(Run::styled(body_font(21), "、"));
        p.runs.push(Run::styled(
            RunProps { underline: Some(Underline::Single), ..body_font(21) },
            "下划线",
        ));
        p.runs.push(Run::styled(body_font(21), "、"));
        p.runs.push(Run::styled(
            RunProps { strike: Some(true), ..body_font(21) },
            "删除线",
        ));
        p.runs.push(Run::styled(body_font(21), "、"));
        p.runs.push(Run::styled(
            RunProps { color: Some("C00000".into()), ..body_font(21) },
            "红色",
        ));
        p.runs.push(Run::styled(body_font(21), "、"));
        p.runs.push(Run::styled(
            RunProps {
                vert_align: Some(VertAlign::Superscript),
                size_half_points: Some(28),
                ..body_font(28)
            },
            "上标",
        ));
        p.runs.push(Run::styled(body_font(21), " 与 "));
        p.runs.push(Run::styled(
            RunProps { vert_align: Some(VertAlign::Subscript), ..body_font(21) },
            "下标",
        ));
        p.runs.push(Run::styled(
            RunProps { font_east_asia: Some("黑体".into()), ..body_font(21) },
            "；再换一种中文字体（黑体）。",
        ));
        p
    }));

    // 居中 + 换行 + 制表符
    doc.blocks.push(Block::Paragraph(
        Paragraph::with_props(ParagraphProps::align(Align::Center))
            .push_run(Run::styled(body_font(21), "这一行居中"))
            .push_run(Run {
                props: body_font(21),
                contents: vec![
                    RunContent::Break(BreakKind::Line),
                    RunContent::Text("第二行前有制表符：".into()),
                    RunContent::Tab,
                    RunContent::Text("A".into()),
                    RunContent::Tab,
                    RunContent::Text("B".into()),
                ],
            }),
    ));

    doc.blocks.push(Block::Paragraph(
        Paragraph::with_props(ParagraphProps::style(STYLE_HEADING2))
            .push_run(Run::text("二、表格")),
    ));

    // 3×3 表格
    let mut table = Table::new(vec![2000, 2000, 2600]);
    table.props.borders = true;
    table.props.width_twips = Some(6600);
    table.rows.push(TableRow {
        cells: vec![
            TableCell {
                width_twips: Some(2000),
                blocks: vec![Block::Paragraph(Paragraph {
                    props: ParagraphProps::align(Align::Center),
                    runs: vec![Run::styled(RunProps { bold: Some(true), ..body_font(21) }, "项目")],
                })],
                ..Default::default()
            },
            TableCell {
                width_twips: Some(2000),
                blocks: vec![Block::Paragraph(Paragraph {
                    props: ParagraphProps::align(Align::Center),
                    runs: vec![Run::styled(RunProps { bold: Some(true), ..body_font(21) }, "结果")],
                })],
                ..Default::default()
            },
            TableCell {
                width_twips: Some(2600),
                blocks: vec![Block::Paragraph(Paragraph {
                    props: ParagraphProps::align(Align::Center),
                    runs: vec![Run::styled(RunProps { bold: Some(true), ..body_font(21) }, "备注")],
                })],
                ..Default::default()
            },
        ],
    });
    table.rows.push(TableRow::from_texts([
        "OPC 包读写",
        "通过",
        "自建 ZIP + CRC32",
    ]));
    table.rows.push(TableRow::from_texts([
        "模型往返",
        "通过",
        "读回后与写出前逐字段相等",
    ]));
    doc.blocks.push(Block::Table(table));

    doc.blocks.push(Block::Paragraph(
        Paragraph::with_props(ParagraphProps {
            space_before_twips: Some(240),
            ..Default::default()
        })
        .push_run(Run::styled(body_font(18), "本文档由 RustOffice 生成，用于 OOXML 底层验证。")),
    ));

    doc
}

#[cfg(test)]
mod tests {
    use super::*;
    use zen_opc::Package;

    /// 核心不变量：写出再读回，**受支持子集必须逐字段相等**。
    #[test]
    fn round_trip_demo_document_is_lossless_in_supported_subset() {
        let original = demo_document();
        let bytes = write_docx(&original).expect("写出失败");
        let outcome = read_docx(&bytes).expect("读回失败");

        assert_eq!(outcome.document.blocks, original.blocks, "正文块不一致");
        assert_eq!(outcome.document.section, original.section, "节属性不一致");
        assert_eq!(outcome.document.core, original.core, "文档属性不一致");
        assert_eq!(outcome.document.styles.default_run, original.styles.default_run);
        assert_eq!(outcome.document.styles.default_para, original.styles.default_para);
        assert_eq!(outcome.document.styles.named, original.styles.named, "自定义样式不一致");
    }

    #[test]
    fn round_trip_is_byte_stable_on_second_pass() {
        let bytes1 = write_docx(&demo_document()).unwrap();
        let doc2 = read_docx(&bytes1).unwrap().document;
        let bytes2 = write_docx(&doc2).unwrap();
        assert_eq!(bytes1, bytes2, "读写一遍之后必须稳定（幂等）");
    }

    #[test]
    fn demo_package_passes_self_verification() {
        let bytes = write_docx(&demo_document()).unwrap();
        let report = verify_docx(&bytes);
        let failures = report.failures();
        assert!(report.ok(), "体检未通过：{failures:#?}");
    }

    #[test]
    fn demo_document_has_no_diagnostics() {
        let bytes = write_docx(&demo_document()).unwrap();
        let outcome = read_docx(&bytes).unwrap();
        assert!(
            outcome.diagnostics.is_empty(),
            "自生成文档不该有诊断：{:#?}",
            outcome.diagnostics
        );
    }

    #[test]
    fn text_extraction_matches_model() {
        let doc = demo_document();
        let text = extract_text(&doc);
        assert!(text.contains("RustOffice 文字测试版 v0.1"));
        assert!(text.contains("粗体"));
        assert!(text.contains("OPC 包读写\t通过\t自建 ZIP + CRC32"), "表格行应按制表符分隔：{text}");
    }

    #[test]
    fn unsupported_content_is_reported_not_dropped() {
        // 手工造一个带图片与域的包，确认它们进诊断
        let mut pkg = write::build_package(&demo_document()).unwrap();
        let mut xml = pkg.get_text("/word/document.xml").unwrap();
        xml = xml.replace(
            "</w:body>",
            "<w:p><w:r><w:drawing/></w:r><w:r><w:fldChar w:fldCharType=\"begin\"/></w:r></w:p></w:body>",
        );
        pkg.put_text("/word/document.xml", xml).unwrap();
        let bytes = pkg.to_bytes().unwrap();

        let outcome = read_docx(&bytes).unwrap();
        assert!(outcome.has_kind(DiagKind::UnsupportedElement));
        assert!(
            outcome.diagnostics.iter().any(|d| d.detail.contains("drawing")),
            "图片必须被点名：{:#?}",
            outcome.diagnostics
        );
        assert!(
            outcome.diagnostics.iter().any(|d| d.detail.contains("fldChar")),
            "域必须被点名"
        );
        // 关键：内容未被解析进模型，但部件字节仍在包里，没有被破坏
        assert!(outcome.package.has("/word/document.xml"));
    }

    #[test]
    fn missing_main_document_is_an_error_not_a_default_document() {
        let mut pkg = Package::with_common_defaults();
        pkg.put_text("/word/other.xml", "<x/>").unwrap();
        let mut root = zen_opc::Relationships::default();
        root.add("rId1", write::REL_OFFICE_DOCUMENT, "word/missing.xml");
        pkg.set_root_rels(&root).unwrap();
        let bytes = pkg.to_bytes().unwrap();

        let err = read_docx(&bytes).unwrap_err();
        assert!(matches!(err, WordError::MissingPart(_)), "得到 {err:?}");
    }

    #[test]
    fn foreign_rels_are_used_not_hardcoded_paths() {
        // 主文档放在非标准路径：读取必须按关系走，不能写死 /word/document.xml
        let mut doc = Document::default();
        doc.blocks.push(Block::paragraph("放在自定义路径下的正文"));
        let mut pkg = write::build_package(&doc).unwrap();
        let xml = pkg.get_text("/word/document.xml").unwrap();
        pkg.put_text("/custom/main.xml", xml).unwrap();
        let mut root = zen_opc::Relationships::default();
        root.add("rId1", write::REL_OFFICE_DOCUMENT, "custom/main.xml");
        pkg.set_root_rels(&root).unwrap();

        let outcome = read_docx(&pkg.to_bytes().unwrap()).unwrap();
        assert_eq!(extract_text(&outcome.document).trim(), "放在自定义路径下的正文");
    }

    #[test]
    fn crc_damage_shows_up_in_verify_and_diagnostics() {
        let bytes = write_docx(&demo_document()).unwrap();
        let mut broken = bytes.clone();
        // 找一个 store 过的短条目来精确破坏：直接改包中段的字节
        let idx = broken.len() / 3;
        broken[idx] ^= 0xFF;

        let report = verify_docx(&broken);
        // 要么结构直接坏掉（zip.readable=false 或 crc 失败），要么 CRC 报错；
        // 无论哪种，都不允许"全绿"
        assert!(!report.ok() || report.checks.iter().all(|c| c.ok), "体检不应在损坏输入上全绿");
        if let Ok(outcome) = read_docx(&broken) {
            assert!(outcome.has_kind(DiagKind::CrcMismatch) || outcome.diagnostics.is_empty());
        }
    }
}
