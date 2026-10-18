//! DOCX 包 → 模型（读入）+ 诊断 + 包体检。
//!
//! 读取原则（与工程约定一致）：
//!   · **不静默丢弃**：认识的解析成模型；不认识的进 `diagnostics`，并保留原部件字节。
//!   · **不猜**：关系（`_rels`）决定去哪里拿 document/styles/core，而不是硬编码路径。
//!   · **部分事实也保留**：CRC 不符、缺部件都进诊断，能读多少读多少。
//!   · **诊断要带对部件名**：解析 styles 时产生的诊断必须标 `styles.xml`，不能一律标 document.xml。

use crate::model::*;
use crate::write::{self, W_NS};
use crate::{Result, WordError};
use zen_opc::{Package, Relationships, ZipArchive};
use roxmltree::{Document as XmlDoc, Node};

/// 诊断收集器：把"哪个部件、哪一类问题、什么细节"绑在一起。
struct Ctx<'a> {
    part: &'a str,
    diags: Vec<Diagnostic>,
    text_sources: Vec<Option<zen_opc::preserve::TextLocation>>,
}

impl<'a> Ctx<'a> {
    fn new(part: &'a str) -> Self {
        Self { part, diags: Vec::new(), text_sources: Vec::new() }
    }

    fn unsupported(&mut self, detail: impl Into<String>) {
        self.push(DiagKind::UnsupportedElement, detail);
    }

    fn unsupported_value(&mut self, detail: impl Into<String>) {
        self.push(DiagKind::UnsupportedValue, detail);
    }

    fn missing(&mut self, detail: impl Into<String>) {
        self.push(DiagKind::MissingPart, detail);
    }

    fn malformed(&mut self, detail: impl Into<String>) {
        self.push(DiagKind::Malformed, detail);
    }

    fn push(&mut self, kind: DiagKind, detail: impl Into<String>) {
        self.diags.push(Diagnostic { part: self.part.to_string(), kind, detail: detail.into() });
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub part: String,
    pub kind: DiagKind,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagKind {
    /// 部件里的元素/属性本版不处理（内容未进模型，但原字节仍在包里）
    UnsupportedElement,
    /// 值不认识（例如未知的对齐取值）
    UnsupportedValue,
    /// 缺部件或缺关系
    MissingPart,
    /// CRC 校验失败
    CrcMismatch,
    /// 结构不合法
    Malformed,
}

impl DiagKind {
    pub fn as_str(self) -> &'static str {
        match self {
            DiagKind::UnsupportedElement => "unsupported-element",
            DiagKind::UnsupportedValue => "unsupported-value",
            DiagKind::MissingPart => "missing-part",
            DiagKind::CrcMismatch => "crc-mismatch",
            DiagKind::Malformed => "malformed",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReadOutcome {
    pub document: Document,
    pub diagnostics: Vec<Diagnostic>,
    pub package: Package,
}

impl ReadOutcome {
    pub fn text(&self) -> String {
        extract_text(&self.document)
    }

    pub fn has_kind(&self, kind: DiagKind) -> bool {
        self.diagnostics.iter().any(|d| d.kind == kind)
    }

    /// 只关心某个部件的诊断（例如只想看 styles.xml 的问题）。
    pub fn diagnostics_for(&self, part_suffix: &str) -> Vec<&Diagnostic> {
        self.diagnostics.iter().filter(|d| d.part.ends_with(part_suffix)).collect()
    }
}

// ---------------------------------------------------------------- 主入口

pub fn read_docx(bytes: &[u8]) -> Result<ReadOutcome> {
    let archive = ZipArchive::read(bytes)?;
    let package = Package::from_archive(&archive)?;
    let mut diags: Vec<Diagnostic> = Vec::new();

    for (name, ok) in archive.crc_report() {
        if !ok {
            diags.push(Diagnostic {
                part: format!("/{name}"),
                kind: DiagKind::CrcMismatch,
                detail: "条目 CRC 与头部不符，内容可能已损坏".into(),
            });
        }
    }

    // 根关系 → 主文档部件（不硬编码路径）
    let root_rels = package.root_rels()?;
    let doc_part = root_rels
        .by_type(write::REL_OFFICE_DOCUMENT)
        .map(|r| zen_opc::opc::resolve_target("/", &r.target))
        .transpose()?
        .ok_or_else(|| WordError::MissingRelationship("officeDocument".into()))?;

    let doc_xml = package
        .get_text(&doc_part)
        .ok_or_else(|| WordError::MissingPart(format!("主文档部件 {doc_part} 不存在（关系指向它）")))?;

    let xml = XmlDoc::parse(&doc_xml)
        .map_err(|e| WordError::BadXml { part: doc_part.clone(), source: e })?;

    let body = xml
        .descendants()
        .find(|n| is_w(*n, "body"))
        .ok_or_else(|| WordError::Malformed {
            part: doc_part.clone(),
            detail: "document 根下找不到 w:body（本版不支持非 body 结构）".into(),
        })?;

    let mut document = Document::default();
    let mut section_seen = false;
    let text_sources;
    {
        let mut ctx = Ctx::new(&doc_part);
        for child in body.children().filter(|n| n.is_element()) {
            if is_w(child, "p") {
                document.blocks.push(Block::Paragraph(parse_paragraph(child, &mut ctx)));
            } else if is_w(child, "tbl") {
                document.blocks.push(Block::Table(parse_table(child, &mut ctx)));
            } else if is_w(child, "sectPr") {
                document.section = parse_section(child, &mut ctx);
                section_seen = true;
            } else if is_w(child, "sdt") {
                ctx.unsupported("w:sdt（内容控件）本版不解析，其内部内容未进入模型");
            } else {
                let name = child.tag_name().name().to_string();
                ctx.unsupported(format!("w:body 下的 <{name}> 本版不处理"));
            }
        }
        if !section_seen {
            ctx.missing("没有 w:sectPr，节属性按默认 A4 处理");
        }
        text_sources = ctx.text_sources;
        diags.append(&mut ctx.diags);
    }

    // 样式（诊断必须标 styles 部件）
    let doc_rels = package.rels_for(&doc_part)?;
    // Follow the relationship rather than assuming /word/settings.xml.
    document.settings.default_tab_stop_twips = 720;
    let settings_rels: Vec<_> = doc_rels.items.iter()
        .filter(|r| r.ty == write::REL_SETTINGS).collect();
    if settings_rels.len() > 1 {
        return Err(WordError::Malformed {
            part: doc_part.clone(), detail: "multiple settings relationships".into(),
        });
    }
    if let Some(rel) = settings_rels.first() {
        if rel.mode.as_deref().is_some_and(|mode| mode != "Internal") {
            return Err(WordError::UnsupportedEdit("external settings relationship".into()));
        }
        let part = zen_opc::opc::resolve_target(&doc_part, &rel.target)?;
        let text = package.get_text(&part)
            .ok_or_else(|| WordError::MissingPart(part.clone()))?;
        document.settings = parse_settings(&text, &part)?;
    }
    let styles_part = doc_rels
        .by_type(write::REL_STYLES)
        .map(|r| zen_opc::opc::resolve_target(&doc_part, &r.target))
        .transpose()?;
    match &styles_part {
        Some(p) => match package.get_text(p) {
            Some(xml_text) => {
                let mut ctx = Ctx::new(p);
                match parse_styles(&xml_text, &mut ctx) {
                    Ok(styles) => document.styles = styles,
                    Err(e) => {
                        ctx.malformed(format!("styles.xml 解析失败：{e}"));
                    }
                }
                diags.append(&mut ctx.diags);
            }
            None => diags.push(Diagnostic {
                part: p.clone(),
                kind: DiagKind::MissingPart,
                detail: "关系指向 styles 部件，但包里没有它".into(),
            }),
        },
        None => diags.push(Diagnostic {
            part: doc_part.clone(),
            kind: DiagKind::MissingPart,
            detail: "主文档没有 styles 关系，样式按默认处理".into(),
        }),
    }

    // 文档属性
    if let Some(r) = root_rels.by_type(write::REL_CORE) {
        let p = zen_opc::opc::resolve_target("/", &r.target)?;
        match package.get_text(&p) {
            Some(xml_text) => document.core = parse_core(&xml_text),
            None => diags.push(Diagnostic {
                part: p,
                kind: DiagKind::MissingPart,
                detail: "关系指向 core 属性部件，但包里没有它".into(),
            }),
        }
    }

    document.preservation = Some(std::sync::Arc::new(crate::preserve::PreservedDocument {
        bytes: std::sync::Arc::new(bytes.to_vec()), baseline: Box::new(document.clone()), locations: text_sources,
    }));
    Ok(ReadOutcome { document, diagnostics: diags, package })
}

// ---------------------------------------------------------------- 段落 / 运行

fn parse_settings(text: &str, part: &str) -> Result<DocumentSettings> {
    let xml = XmlDoc::parse(text)
        .map_err(|source| WordError::BadXml { part: part.into(), source })?;
    let malformed = |detail: &str| WordError::Malformed {
        part: part.into(), detail: detail.into(),
    };
    let root = xml.root_element();
    if !is_w(root, "settings") {
        return Err(malformed("expected w:settings root"));
    }
    let tabs: Vec<_> = root.children().filter(|n| is_w(*n, "defaultTabStop")).collect();
    if tabs.len() > 1 {
        return Err(malformed("duplicate w:defaultTabStop"));
    }
    let mut settings = DocumentSettings { default_tab_stop_twips: 720 };
    if let Some(tab) = tabs.first() {
        let val = w_attr(*tab, "val").ok_or_else(|| malformed("defaultTabStop missing w:val"))?;
        settings.default_tab_stop_twips = val.trim().parse::<u32>()
            .map_err(|_| malformed("defaultTabStop is not an unsigned twip value"))?;
    }
    settings.validate()?;
    Ok(settings)
}

fn parse_paragraph(node: Node, ctx: &mut Ctx) -> Paragraph {
    let mut p = Paragraph::default();
    for child in node.children().filter(|n| n.is_element()) {
        if is_w(child, "pPr") {
            p.props = parse_paragraph_props(child, ctx);
        } else if is_w(child, "r") {
            let run = parse_run(child, ctx);
            if !run.is_effectively_empty() {
                p.runs.push(run);
            }
        } else if is_w(child, "hyperlink") {
            ctx.unsupported("w:hyperlink 仅将文字投影到当前模型；原包保存保留关系与样式");
            for r in child.children().filter(|n| is_w(*n, "r")) {
                let run = parse_run(r, ctx);
                if !run.is_effectively_empty() {
                    p.runs.push(run);
                }
            }
        } else if is_w(child, "bookmarkStart")
            || is_w(child, "bookmarkEnd")
            || is_w(child, "proofErr")
            || is_w(child, "commentRangeStart")
            || is_w(child, "commentRangeEnd")
        {
            // 锚点类元素：无内容，忽略不算丢内容
        } else {
            let name = child.tag_name().name().to_string();
            ctx.unsupported(format!("段落内的 <{name}> 本版不处理"));
        }
    }
    p
}

fn parse_paragraph_props(node: Node, ctx: &mut Ctx) -> ParagraphProps {
    let mut p = ParagraphProps::default();
    for c in node.children().filter(|n| n.is_element()) {
        if c.tag_name().namespace() != Some(W_NS) {
            ctx.unsupported("foreign paragraph property retained only in original package");
            continue;
        }
        match c.tag_name().name() {
            "sectPr" => p.section_end = Some(parse_section(c, ctx)),
            "pStyle" => p.style_id = w_attr(c, "val"),
            "keepNext" => { p.keep_next = w_on_off(c); p.keep_next_off = !p.keep_next; }
            "pageBreakBefore" => { p.page_break_before = w_on_off(c); p.page_break_before_off = !p.page_break_before; }
            "outlineLvl" => {
                p.outline_level = w_attr(c, "val").and_then(|v| v.parse::<u8>().ok());
            }
            "spacing" => {
                p.space_before_twips = w_attr(c, "before").and_then(|v| v.parse().ok());
                p.space_after_twips = w_attr(c, "after").and_then(|v| v.parse().ok());
                if let Some(line) = w_attr(c, "line").and_then(|v| v.parse::<u32>().ok()) {
                    p.line_spacing = Some(match w_attr(c, "lineRule").as_deref() {
                        Some("exact") => LineSpacing::Exact(line),
                        Some("atLeast") => LineSpacing::AtLeast(line),
                        // 缺省即 auto（OOXML 规则），所以 None 也归 auto
                        _ => LineSpacing::Auto(line),
                    });
                }
            }
            "ind" => {
                p.ind_left_twips = w_attr(c, "left").and_then(|v| v.parse().ok());
                p.ind_right_twips = w_attr(c, "right").and_then(|v| v.parse().ok());
                p.ind_first_line_twips = match (
                    w_attr(c, "firstLine").and_then(|v| v.parse::<i32>().ok()),
                    w_attr(c, "hanging").and_then(|v| v.parse::<i32>().ok()),
                ) {
                    (Some(v), _) => Some(v),
                    (None, Some(v)) => Some(-v),
                    (None, None) => None,
                };
            }
            "jc" => {
                let v = w_attr(c, "val").unwrap_or_default();
                match Align::from_ooxml(&v) {
                    Some(a) => p.align = Some(a),
                    None => ctx.unsupported_value(format!("未知的 w:jc 取值 {v:?}")),
                }
            }
            "shd" => {
                if let Some(fill) = w_attr(c, "fill") {
                    if !fill.eq_ignore_ascii_case("auto") && !fill.eq_ignore_ascii_case("none") {
                        p.shading = Some(Shading { fill_hex: fill.to_ascii_uppercase() });
                    }
                }
            }
            // 这些不影响本版能表达的内容：明确忽略
            "rPr" => p.mark_run = parse_run_props(c, ctx),
            "keepLines" | "widowControl" | "snapToGrid" | "contextualSpacing" => {}
            other => ctx.unsupported(format!("段落属性 <{other}> 本版不处理")),
        }
    }
    p
}

fn parse_run(node: Node, ctx: &mut Ctx) -> Run {
    let mut r = Run::default();
    let mut locations = Vec::new();
    for c in node.children().filter(|n| n.is_element()) {
        match c.tag_name().name() {
            "rPr" => r.props = parse_run_props(c, ctx),
            "t" => {
                r.contents.push(RunContent::Text(c.text().unwrap_or("").to_string()));
                let complex = node.ancestors().any(|a| a.is_element() && matches!(a.tag_name().name(), "ins" | "del" | "fldSimple" | "sdt"))
                    || node.parent().is_some_and(|p| p.descendants().any(|n| n.is_element() && matches!(n.tag_name().name(), "fldChar" | "instrText")));
                let plain_text=c.has_tag_name((W_NS,"t")) && node.has_tag_name((W_NS,"r")) && c.children().all(|n|n.is_text()) && c.children().count()<=1;
                locations.push(Some(zen_opc::preserve::TextLocation { part: ctx.part.into(), range: c.range(), editable: !complex && plain_text }));
            }
            "br" => {
                let k = match w_attr(c, "type").as_deref() {
                    Some("page") => BreakKind::Page,
                    Some("column") => BreakKind::Column,
                    _ => BreakKind::Line,
                };
                r.contents.push(RunContent::Break(k));
            }
            "tab" => r.contents.push(RunContent::Tab),
            "drawing" | "pict" | "object" => ctx.unsupported(format!(
                "<{}>（图片/嵌入对象）本版不解析，其字节仍保留在包里",
                c.tag_name().name()
            )),
            "fldChar" | "instrText" => {
                ctx.unsupported("域代码（w:fldChar / w:instrText）本版不解析")
            }
            "noBreakHyphen" => { r.contents.push(RunContent::Text("\u{2011}".into())); locations.push(None); },
            "softHyphen" => { r.contents.push(RunContent::Text("\u{00AD}".into())); locations.push(None); },
            other => ctx.unsupported(format!("运行内容 <{other}> 本版不处理")),
        }
    }
    if !r.is_effectively_empty() { ctx.text_sources.extend(locations); }
    r
}

fn parse_run_props(node: Node, ctx: &mut Ctx) -> RunProps {
    let mut r = RunProps::default();
    for c in node.children().filter(|n| n.is_element()) {
        if c.tag_name().namespace() != Some(W_NS) {
            ctx.unsupported("foreign run property retained only in original package");
            continue;
        }
        match c.tag_name().name() {
            "rStyle" => r.style_id = w_attr(c, "val"),
            "rFonts" => {
                r.font_ascii = w_attr(c, "ascii").or_else(|| w_attr(c, "hAnsi"));
                r.font_east_asia = w_attr(c, "eastAsia");
                if ["asciiTheme", "hAnsiTheme", "eastAsiaTheme", "cstheme"].iter().any(|a| w_attr(c,a).is_some()) {
                    ctx.unsupported("theme font references are retained in the package; effective theme font resolution is not implemented");
                }
            }
            "b" => r.bold = Some(w_on_off(c)),
            "i" => r.italic = Some(w_on_off(c)),
            "caps" => r.caps = Some(w_on_off(c)),
            "strike" => r.strike = Some(w_on_off(c)),
            "color" => {
                let v = w_attr(c, "val").unwrap_or_default();
                if v.eq_ignore_ascii_case("auto") {
                    r.color = None;
                    r.color_auto = true;
                } else {
                    r.color = Some(v.to_ascii_uppercase());
                    r.color_auto = false;
                }
            }
            "sz" => r.size_half_points = w_attr(c, "val").and_then(|v| v.parse().ok()),
            "u" => {
                let v = w_attr(c, "val").unwrap_or_else(|| "single".into());
                match Underline::from_ooxml(&v) {
                    Some(u) => r.underline = Some(u),
                    None => ctx.unsupported_value(format!("未知的下划线类型 {v:?}")),
                }
            }
            "vertAlign" => {
                let v = w_attr(c, "val").unwrap_or_default();
                if let Some(va) = VertAlign::from_ooxml(&v) {
                    r.vert_align = Some(va);
                }
            }
            "highlight" => {
                let v = w_attr(c, "val").unwrap_or_default();
                if let Some(h) = HighlightColor::from_ooxml(&v) {
                    r.highlight = Some(h);
                }
            }
            // 这些不影响本版能表达的内容：明确忽略而不是报"不支持"
            "szCs" | "lang" | "noProof" | "kern" | "spacing" | "position" => {}
            other => ctx.unsupported(format!("字符属性 <{other}> 本版不处理")),
        }
    }
    r
}

// ---------------------------------------------------------------- 表格

fn parse_table(node: Node, ctx: &mut Ctx) -> Table {
    let mut t = Table::default();
    for c in node.children().filter(|n| n.is_element()) {
        match c.tag_name().name() {
            "tblPr" => {
                for p in c.children().filter(|n| n.is_element()) {
                    match p.tag_name().name() {
                        "tblW" => {
                            let ty = w_attr(p, "type").unwrap_or_else(|| "dxa".into());
                            t.props.width_twips = if ty == "auto" {
                                None
                            } else {
                                w_attr(p, "w").and_then(|v| v.parse().ok())
                            };
                        }
                        "tblBorders" => t.props.borders = p.children().any(|n| n.is_element()),
                        "tblStyle" => t.props.style_id = w_attr(p, "val"),
                        "tblLayout" | "tblLook" | "jc" | "tblCellMar" => {}
                        other => ctx.unsupported(format!("表格属性 <{other}> 本版不处理")),
                    }
                }
            }
            "tblGrid" => {
                for col in c.children().filter(|n| is_w(*n, "gridCol")) {
                    t.grid_twips
                        .push(w_attr(col, "w").and_then(|v| v.parse().ok()).unwrap_or(0));
                }
            }
            "tr" => t.rows.push(parse_row(c, ctx)),
            other => ctx.unsupported(format!("表格内的 <{other}> 本版不处理")),
        }
    }
    t
}

fn parse_row(node: Node, ctx: &mut Ctx) -> TableRow {
    let mut row = TableRow::default();
    for c in node.children().filter(|n| n.is_element()) {
        match c.tag_name().name() {
            "tc" => {
                let mut cell = TableCell::default();
                for cc in c.children().filter(|n| n.is_element()) {
                    match cc.tag_name().name() {
                        "tcPr" => {
                            for p in cc.children().filter(|n| n.is_element()) {
                                match p.tag_name().name() {
                                    "tcW" => {
                                        let ty =
                                            w_attr(p, "type").unwrap_or_else(|| "dxa".into());
                                        cell.width_twips = if ty == "auto" {
                                            None
                                        } else {
                                            w_attr(p, "w").and_then(|v| v.parse().ok())
                                        };
                                    }
                                    "gridSpan" => {
                                        cell.grid_span = w_attr(p, "val")
                                            .and_then(|v| v.parse().ok())
                                            .unwrap_or(1);
                                    }
                                    "vMerge" => {
                                        cell.v_merge = Some(match w_attr(p, "val").as_deref() {
                                            Some("restart") => VMerge::Restart,
                                            _ => VMerge::Continue,
                                        });
                                    }
                                    "shd" => {
                                        if let Some(fill) = w_attr(p, "fill") {
                                            if !fill.eq_ignore_ascii_case("auto") && !fill.eq_ignore_ascii_case("none") {
                                                cell.shading = Some(Shading { fill_hex: fill.to_ascii_uppercase() });
                                            }
                                        }
                                    }
                                    "tcMar" | "vAlign" | "noWrap" => {}
                                    other => {
                                        ctx.unsupported(format!("单元格属性 <{other}> 本版不处理"))
                                    }
                                }
                            }
                        }
                        "p" => cell.blocks.push(Block::Paragraph(parse_paragraph(cc, ctx))),
                        "tbl" => cell.blocks.push(Block::Table(parse_table(cc, ctx))),
                        other => ctx.unsupported(format!("单元格内的 <{other}> 本版不处理")),
                    }
                }
                row.cells.push(cell);
            }
            "trPr" => {}
            other => ctx.unsupported(format!("行内的 <{other}> 本版不处理")),
        }
    }
    row
}

// ---------------------------------------------------------------- 节属性

fn parse_section(node: Node, ctx: &mut Ctx) -> SectionProps {
    let mut s = SectionProps::default();
    for c in node.children().filter(|n| n.is_element()) {
        if c.tag_name().namespace() != Some(W_NS) {
            ctx.unsupported("foreign section property retained only in original package");
            continue;
        }
        match c.tag_name().name() {
            "type" => {
                let value = w_attr(c, "val").unwrap_or_else(|| "nextPage".into());
                s.start = SectionStart::from_ooxml(&value);
                if matches!(s.start, SectionStart::Unknown(_)) {
                    ctx.unsupported_value(format!("unknown section start {value:?}"));
                }
            }
            "pgNumType" => {
                s.page_number_format = w_attr(c, "fmt");
                if let Some(value) = w_attr(c, "start") {
                    s.page_number_start = value.parse().ok();
                    if s.page_number_start.is_none() {
                        ctx.unsupported_value(format!("invalid page number start {value:?}"));
                    }
                }
            }
            "pgSz" => {
                if let Some(v) = w_attr(c, "w").and_then(|v| v.parse().ok()) {
                    s.page_width_twips = v;
                }
                if let Some(v) = w_attr(c, "h").and_then(|v| v.parse().ok()) {
                    s.page_height_twips = v;
                }
                if let Some(o) = w_attr(c, "orient") {
                    match Orientation::from_ooxml(&o) {
                        Some(o) => s.orientation = o,
                        None => ctx.unsupported_value(format!("未知的纸张方向 {o:?}")),
                    }
                }
            }
            "pgMar" => {
                s.margin_top_twips = w_attr(c, "top").and_then(|v| v.parse().ok()).unwrap_or(0);
                s.margin_bottom_twips =
                    w_attr(c, "bottom").and_then(|v| v.parse().ok()).unwrap_or(0);
                s.margin_left_twips = w_attr(c, "left").and_then(|v| v.parse().ok()).unwrap_or(0);
                s.margin_right_twips = w_attr(c, "right").and_then(|v| v.parse().ok()).unwrap_or(0);
                s.header_distance_twips = w_attr(c, "header").and_then(|v| v.parse().ok()).unwrap_or(851);
                s.footer_distance_twips = w_attr(c, "footer").and_then(|v| v.parse().ok()).unwrap_or(992);
                s.gutter_twips = w_attr(c, "gutter").and_then(|v| v.parse().ok()).unwrap_or(0);
            }
            "cols" => {
                if let Some(v) = w_attr(c, "num").and_then(|v| v.parse().ok()) {
                    s.column_count = v;
                }
                if let Some(v) = w_attr(c, "space").and_then(|v| v.parse().ok()) {
                    s.column_space_twips = v;
                }
            }
            "titlePg" => s.title_page = w_on_off(c),
            other => ctx.unsupported(format!("节属性 <{other}> 本版不处理")),
        }
    }
    s
}

// ---------------------------------------------------------------- 样式与属性

fn parse_styles(xml_text: &str, ctx: &mut Ctx) -> Result<Styles> {
    let xml = XmlDoc::parse(xml_text).map_err(|e| WordError::BadXml {
        part: ctx.part.to_string(),
        source: e,
    })?;
    let mut styles = Styles::default();
    styles.imported_style_ids = Some(Vec::new());

    if let Some(dd) = xml.root_element().children().find(|n| is_w(*n, "docDefaults")) {
        if let Some(rpr) = dd.children().find(|n| is_w(*n, "rPrDefault")) {
            if let Some(rpr) = rpr.children().find(|n| is_w(*n, "rPr")) {
                styles.default_run = parse_run_props(rpr, ctx);
            }
        }
        if let Some(ppr) = dd.children().find(|n| is_w(*n, "pPrDefault")) {
            if let Some(ppr) = ppr.children().find(|n| is_w(*n, "pPr")) {
                styles.default_para = parse_paragraph_props(ppr, ctx);
            }
        }
    }

    let templates = template_style_defs();

    for st in xml.root_element().children().filter(|n| is_w(*n, "style")) {
        let style_id = match w_attr(st, "styleId") {
            Some(v) => v,
            None => continue,
        };
        let kind = match w_attr(st, "type").as_deref().and_then(StyleKind::from_ooxml) {
            Some(kind) => kind,
            None => {
                ctx.unsupported(format!("style {style_id} has a missing or unknown type; it cannot become a default paragraph style"));
                continue;
            }
        };
        styles.imported_style_ids.as_mut().unwrap().push(style_id.clone());
        if w_attr(st, "default").is_some_and(|v| matches!(v.as_str(), "1" | "true" | "on")) {
            match kind {
                StyleKind::Paragraph => styles.default_paragraph_style_id = Some(style_id.clone()),
                StyleKind::Character => styles.default_character_style_id = Some(style_id.clone()),
                StyleKind::Table | StyleKind::Numbering => {},
            }
        }
        let name = st
            .children()
            .find(|n| is_w(*n, "name"))
            .and_then(|n| w_attr(n, "val"))
            .unwrap_or_else(|| style_id.clone());
        let based_on = st.children().find(|n| is_w(*n, "basedOn")).and_then(|n| w_attr(n, "val"));
        let mut run = RunProps::default();
        let mut para = ParagraphProps::default();
        for c in st.children().filter(|n| n.is_element()) {
            if c.tag_name().namespace() != Some(W_NS) { continue; }
            match c.tag_name().name() {
                "rPr" => run = parse_run_props(c, ctx),
                "pPr" => para = parse_paragraph_props(c, ctx),
                _ => {}
            }
        }

        // Canonical template styles are implicit. External overrides remain
        // available to the resolver without changing the custom-style list.
        if let Some(tpl) = templates.iter().find(|t| t.style_id == style_id) {
            if run != tpl.run || para != tpl.para || kind != tpl.kind || based_on != tpl.based_on {
                styles.builtin_overrides.push(StyleDef { style_id, name, kind, based_on, run, para });
            }
            continue;
        }

        styles.named.push(StyleDef { style_id, name, kind, based_on, run, para });
    }

    Ok(styles)
}

fn parse_core(xml_text: &str) -> CoreProps {
    let mut core = CoreProps::default();
    let Ok(xml) = XmlDoc::parse(xml_text) else {
        return core;
    };
    for n in xml.root_element().children().filter(|n| n.is_element()) {
        let text = n.text().unwrap_or("").trim().to_string();
        if text.is_empty() {
            continue;
        }
        match n.tag_name().name() {
            "title" => core.title = Some(text),
            "subject" => core.subject = Some(text),
            "creator" => core.creator = Some(text),
            "lastModifiedBy" => core.last_modified_by = Some(text),
            "created" => core.created = Some(text),
            "modified" => core.modified = Some(text),
            "keywords" => core.keywords = Some(text),
            _ => {}
        }
    }
    core
}

// ---------------------------------------------------------------- 纯文本

/// 文档 → 纯文本：段落一行；表格每行用制表符分隔单元格。
pub fn extract_text(doc: &Document) -> String {
    let mut out = String::new();
    for b in &doc.blocks {
        match b {
            Block::Paragraph(p) => {
                out.push_str(&p.plain_text());
                out.push('\n');
            }
            Block::Table(t) => {
                for row in &t.rows {
                    let cells: Vec<String> = row
                        .cells
                        .iter()
                        .map(|c| c.blocks.iter().map(block_text).collect::<Vec<_>>().join(" "))
                        .collect();
                    out.push_str(&cells.join("\t"));
                    out.push('\n');
                }
            }
        }
    }
    out
}

fn block_text(b: &Block) -> String {
    match b {
        Block::Paragraph(p) => p.plain_text(),
        Block::Table(t) => t
            .rows
            .iter()
            .map(|r| {
                r.cells
                    .iter()
                    .map(|c| c.blocks.iter().map(block_text).collect::<Vec<_>>().join(" "))
                    .collect::<Vec<_>>()
                    .join("\t")
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

// ---------------------------------------------------------------- XML 小工具

fn is_w(node: Node, name: &str) -> bool {
    node.is_element()
        && node.tag_name().name() == name
        && node.tag_name().namespace() == Some(W_NS)
}

/// 取 w 命名空间下的属性值（`w:val`、`w:w` 等）。
fn w_attr(node: Node, name: &str) -> Option<String> {
    node.attribute((W_NS, name)).map(|s| s.to_string())
}

/// OOXML 的开关属性：`<w:b/>` = true，`<w:b w:val="0"/>` = false。
fn w_on_off(node: Node) -> bool {
    !matches!(w_attr(node, "val").as_deref(), Some("0") | Some("false") | Some("off"))
}

// ---------------------------------------------------------------- 包体检

#[derive(Debug, Clone)]
pub struct Check {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Default)]
pub struct VerifyReport {
    pub checks: Vec<Check>,
}

impl VerifyReport {
    pub fn ok(&self) -> bool {
        self.checks.iter().all(|c| c.ok)
    }

    pub fn failures(&self) -> Vec<&Check> {
        self.checks.iter().filter(|c| !c.ok).collect()
    }
}

/// 只读体检：CRC、必备部件、关系目标存在、XML 可解析、body 以 sectPr 收尾。
pub fn verify_docx(bytes: &[u8]) -> VerifyReport {
    let mut r = VerifyReport::default();

    let archive = match ZipArchive::read(bytes) {
        Ok(a) => {
            r.checks.push(Check {
                name: "zip 可读".into(),
                ok: true,
                detail: format!("{} 个条目", a.entries.len()),
            });
            a
        }
        Err(e) => {
            r.checks.push(Check { name: "zip 可读".into(), ok: false, detail: e.to_string() });
            return r;
        }
    };

    let bad: Vec<String> = archive
        .crc_report()
        .into_iter()
        .filter(|(_, ok)| !ok)
        .map(|(n, _)| n)
        .collect();
    r.checks.push(Check {
        name: "zip CRC 全部通过".into(),
        ok: bad.is_empty(),
        detail: if bad.is_empty() { "无失败条目".into() } else { bad.join(", ") },
    });

    let package = match Package::from_archive(&archive) {
        Ok(p) => p,
        Err(e) => {
            r.checks.push(Check { name: "OPC 包装配".into(), ok: false, detail: e.to_string() });
            return r;
        }
    };
    r.checks.push(Check {
        name: "[Content_Types].xml 可解析".into(),
        ok: true,
        detail: format!(
            "{} 个 Default / {} 个 Override",
            package.content_types.defaults.len(),
            package.content_types.overrides.len()
        ),
    });
    r.checks.push(Check {
        name: "OPC 包装配".into(),
        ok: true,
        detail: format!("{} 个部件", package.parts.len()),
    });

    let root_rels = match package.root_rels() {
        Ok(rels) => rels,
        Err(e) => {
            r.checks.push(Check {
                name: "根关系 officeDocument 可解析".into(),
                ok: false,
                detail: format!("根关系 .rels 解析失败: {e}"),
            });
            return r;
        }
    };
    let doc_part = root_rels
        .by_type(write::REL_OFFICE_DOCUMENT)
        .and_then(|rel| zen_opc::opc::resolve_target("/", &rel.target).ok());
    let doc_part = match doc_part {
        Some(p) => {
            r.checks.push(Check {
                name: "根关系 officeDocument 可解析".into(),
                ok: true,
                detail: p.clone(),
            });
            p
        }
        None => {
            r.checks.push(Check {
                name: "根关系 officeDocument 可解析".into(),
                ok: false,
                detail: "找不到或无法解析".into(),
            });
            return r;
        }
    };

    r.checks.push(Check {
        name: "主文档部件存在".into(),
        ok: package.has(&doc_part),
        detail: doc_part.clone(),
    });

    let ct = package.content_types.content_type_for(&doc_part);
    r.checks.push(Check {
        name: "主文档内容类型正确".into(),
        ok: ct == Some(write::CT_DOCUMENT),
        detail: ct.unwrap_or("(缺失)").to_string(),
    });

    let doc_rels = match package.rels_for(&doc_part) {
        Ok(rels) => rels,
        Err(e) => {
            r.checks.push(Check {
                name: format!("{doc_part} 关系解析"),
                ok: false,
                detail: format!("关系 XML 解析失败: {e}"),
            });
            return r;
        }
    };
    let mut missing = Vec::new();
    for rel in &doc_rels.items {
        if rel.mode.as_deref() == Some("External") {
            continue;
        }
        match zen_opc::opc::resolve_target(&doc_part, &rel.target) {
            Ok(t) if package.has(&t) => {}
            Ok(t) => missing.push(t),
            Err(e) => missing.push(format!("{} ({e})", rel.target)),
        }
    }
    r.checks.push(Check {
        name: "主文档关系目标均存在".into(),
        ok: missing.is_empty(),
        detail: if missing.is_empty() {
            format!("{} 条关系", doc_rels.items.len())
        } else {
            missing.join(", ")
        },
    });

    match package.get_text(&doc_part) {
        Some(text) => match XmlDoc::parse(&text) {
            Ok(xml) => {
                r.checks.push(Check {
                    name: "document.xml 可解析".into(),
                    ok: true,
                    detail: format!("{} 字节", text.len()),
                });
                let last_is_sect = xml
                    .descendants()
                    .find(|n| is_w(*n, "body"))
                    .and_then(|b| b.children().filter(|n| n.is_element()).next_back())
                    .map(|n| is_w(n, "sectPr"))
                    .unwrap_or(false);
                r.checks.push(Check {
                    name: "body 以 sectPr 收尾".into(),
                    ok: last_is_sect,
                    detail: String::new(),
                });
            }
            Err(e) => r.checks.push(Check {
                name: "document.xml 可解析".into(),
                ok: false,
                detail: e.to_string(),
            }),
        },
        None => r.checks.push(Check {
            name: "document.xml 可解析".into(),
            ok: false,
            detail: "部件缺失".into(),
        }),
    }

    for (label, text) in [
        ("_rels/.rels", package.get_text(zen_opc::opc::ROOT_RELS)),
        ("word/_rels/document.xml.rels", package.get_text("/word/_rels/document.xml.rels")),
    ] {
        match text {
            Some(t) => match Relationships::parse(&t) {
                Ok(parsed) => {
                    r.checks.push(Check {
                        name: format!("{label} 可解析"),
                        ok: true,
                        detail: format!("{} 条关系", parsed.items.len()),
                    });
                }
                Err(e) => {
                    r.checks.push(Check {
                        name: format!("{label} 可解析"),
                        ok: false,
                        detail: format!("XML 损坏: {e}"),
                    });
                }
            },
            None => r.checks.push(Check {
                name: format!("{label} 可解析"),
                ok: false,
                detail: "不存在".into(),
            }),
        }
    }

    r
}
