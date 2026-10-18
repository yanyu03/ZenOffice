//! 模型 → DOCX 包（写出）。
//!
//! 元素顺序按 ECMA-376 的**序列**书写（`w:pPr` / `w:rPr` / `w:tblPr` / `w:tcPr` 都有固定顺序）。
//! Word 往往宽容，但 WPS 与严格校验器不一定，所以这里不靠"能打开"赌顺序。
//!
//! 刻意不写的东西（v0.1）：`mc:AlternateContent`、主题、编号、批注、域、图片、页眉页脚。

use crate::model::*;
use crate::Result;
use zen_opc::{ContentTypes, Package, Relationships};

pub const W_NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
pub const R_NS: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
pub const CP_NS: &str =
    "http://schemas.openxmlformats.org/package/2006/metadata/core-properties";
pub const DC_NS: &str = "http://purl.org/dc/elements/1.1/";
pub const DCTERMS_NS: &str = "http://purl.org/dc/terms/";
pub const XSI_NS: &str = "http://www.w3.org/2001/XMLSchema-instance";
pub const EXT_PROPS_NS: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/extended-properties";

pub const CT_DOCUMENT: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
pub const CT_STYLES: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml";
pub const CT_SETTINGS: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml";
pub const CT_FONT_TABLE: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.fontTable+xml";
pub const CT_CORE: &str = "application/vnd.openxmlformats-package.core-properties+xml";
pub const CT_APP: &str =
    "application/vnd.openxmlformats-officedocument.extended-properties+xml";

pub const REL_OFFICE_DOCUMENT: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
pub const REL_STYLES: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles";
pub const REL_SETTINGS: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings";
pub const REL_FONT_TABLE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/fontTable";
pub const REL_CORE: &str =
    "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties";
pub const REL_APP: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties";

pub const PART_DOCUMENT: &str = "/word/document.xml";
pub const PART_STYLES: &str = "/word/styles.xml";
pub const PART_SETTINGS: &str = "/word/settings.xml";
pub const PART_FONT_TABLE: &str = "/word/fontTable.xml";
pub const PART_CORE: &str = "/docProps/core.xml";
pub const PART_APP: &str = "/docProps/app.xml";

const XML_DECL: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#;

/// 模型 → DOCX 字节。
pub fn write_docx(doc: &Document) -> Result<Vec<u8>> {
    if let Some(source) = &doc.preservation { return crate::preserve::save(doc, source); }
    let pkg = build_package(doc)?;
    Ok(pkg.to_bytes()?)
}

/// 模型 → 完整的 OPC 包（测试或需要再加工时用）。
pub fn build_package(doc: &Document) -> Result<Package> {
    if let Some(source) = &doc.preservation {
        return Ok(Package::from_archive(&zen_opc::ZipArchive::read(&crate::preserve::save(doc, source)?)?)?);
    }
    doc.validate_sections()?;
    doc.settings.validate()?;
    crate::styles::StyleResolver::new(&doc.styles)?;
    let mut pkg = Package::with_common_defaults();

    pkg.put_text(PART_DOCUMENT, document_xml(doc))?;
    pkg.put_text(PART_STYLES, styles_xml(doc))?;
    pkg.put_text(PART_SETTINGS, settings_xml_for(&doc.settings))?;
    pkg.put_text(PART_FONT_TABLE, font_table_xml(doc))?;
    pkg.put_text(PART_CORE, core_xml(&doc.core))?;
    pkg.put_text(PART_APP, app_xml())?;

    // 文档自己的关系
    let mut doc_rels = Relationships::default();
    doc_rels.add("rId1", REL_STYLES, "styles.xml");
    doc_rels.add("rId2", REL_SETTINGS, "settings.xml");
    doc_rels.add("rId3", REL_FONT_TABLE, "fontTable.xml");
    pkg.set_rels_for(PART_DOCUMENT, &doc_rels)?;

    // 包根关系
    let mut root = Relationships::default();
    root.add("rId1", REL_OFFICE_DOCUMENT, "word/document.xml");
    root.add("rId2", REL_CORE, "docProps/core.xml");
    root.add("rId3", REL_APP, "docProps/app.xml");
    pkg.set_root_rels(&root)?;

    // Content Types
    let mut ct: ContentTypes = pkg.content_types.clone();
    ct.add_override(PART_DOCUMENT, CT_DOCUMENT)?;
    ct.add_override(PART_STYLES, CT_STYLES)?;
    ct.add_override(PART_SETTINGS, CT_SETTINGS)?;
    ct.add_override(PART_FONT_TABLE, CT_FONT_TABLE)?;
    ct.add_override(PART_CORE, CT_CORE)?;
    ct.add_override(PART_APP, CT_APP)?;
    pkg.content_types = ct;

    for part in &pkg.parts {
        if part.name.ends_with(".xml") {
            let text = std::str::from_utf8(&part.data).map_err(|_| crate::WordError::UnsupportedEdit("non UTF-8 XML output".into()))?;
            roxmltree::Document::parse(text).map_err(|source| crate::WordError::BadXml { part: part.name.clone(), source })?;
        }
    }
    Ok(pkg)
}

// ---------------------------------------------------------------- document.xml

pub fn document_xml(doc: &Document) -> String {
    let mut s = String::with_capacity(1024 + doc.blocks.len() * 128);
    s.push_str(XML_DECL);
    s.push_str(&format!(r#"<w:document xmlns:w="{W_NS}" xmlns:r="{R_NS}"><w:body>"#));
    for b in &doc.blocks {
        write_block(&mut s, b);
    }
    write_section(&mut s, &doc.section);
    s.push_str("</w:body></w:document>");
    s
}

fn write_block(s: &mut String, block: &Block) {
    match block {
        Block::Paragraph(p) => write_paragraph(s, p),
        Block::Table(t) => write_table(s, t),
    }
}

fn write_paragraph(s: &mut String, p: &Paragraph) {
    s.push_str("<w:p>");
    write_paragraph_props(s, &p.props);
    for r in &p.runs {
        // 规范化：只含空文本的 run 不写出（模型里允许它存在，包里不留）
        if r.is_effectively_empty() {
            continue;
        }
        write_run(s, r);
    }
    s.push_str("</w:p>");
}

fn write_paragraph_props(s: &mut String, p: &ParagraphProps) {
    if p.is_empty() {
        return;
    }
    s.push_str("<w:pPr>");
    // CT_PPrBase 序列顺序：pStyle → keepNext → pageBreakBefore → spacing → ind → jc → outlineLvl
    if let Some(st) = &p.style_id {
        s.push_str(&format!(r#"<w:pStyle w:val="{}"/>"#, zen_opc::preserve::xml_text(st)));
    }
    if p.keep_next_off {
        s.push_str(r#"<w:keepNext w:val="0"/>"#);
    } else if p.keep_next {
        s.push_str("<w:keepNext/>");
    }
    if p.page_break_before_off {
        s.push_str(r#"<w:pageBreakBefore w:val="0"/>"#);
    } else if p.page_break_before {
        s.push_str("<w:pageBreakBefore/>");
    }
    if let Some(shd) = &p.shading {
        s.push_str(&format!(r#"<w:shd w:val="clear" w:color="auto" w:fill="{}"/>"#, zen_opc::preserve::xml_text(&shd.fill_hex)));
    }
    if p.space_before_twips.is_some() || p.space_after_twips.is_some() || p.line_spacing.is_some() {
        s.push_str("<w:spacing");
        if let Some(v) = p.space_before_twips {
            s.push_str(&format!(r#" w:before="{v}""#));
        }
        if let Some(v) = p.space_after_twips {
            s.push_str(&format!(r#" w:after="{v}""#));
        }
        if let Some(ls) = p.line_spacing {
            match ls {
                LineSpacing::Auto(v) => {
                    s.push_str(&format!(r#" w:line="{v}" w:lineRule="auto""#));
                }
                LineSpacing::Exact(v) => {
                    s.push_str(&format!(r#" w:line="{v}" w:lineRule="exact""#));
                }
                LineSpacing::AtLeast(v) => {
                    s.push_str(&format!(r#" w:line="{v}" w:lineRule="atLeast""#));
                }
            }
        }
        s.push_str("/>");
    }
    if p.ind_left_twips.is_some() || p.ind_right_twips.is_some() || p.ind_first_line_twips.is_some()
    {
        s.push_str("<w:ind");
        if let Some(v) = p.ind_left_twips {
            s.push_str(&format!(r#" w:left="{v}""#));
        }
        if let Some(v) = p.ind_right_twips {
            s.push_str(&format!(r#" w:right="{v}""#));
        }
        // 悬挂缩进在 OOXML 里是 w:hanging（无符号）；负数用 hanging 表达
        match p.ind_first_line_twips {
            Some(v) if v < 0 => s.push_str(&format!(r#" w:hanging="{}""#, -v)),
            Some(v) => s.push_str(&format!(r#" w:firstLine="{v}""#)),
            None => {}
        }
        s.push_str("/>");
    }
    if let Some(a) = p.align {
        s.push_str(&format!(r#"<w:jc w:val="{}"/>"#, a.as_ooxml()));
    }
    if let Some(lvl) = p.outline_level {
        s.push_str(&format!(r#"<w:outlineLvl w:val="{lvl}"/>"#));
    }
    write_run_props(s, &p.mark_run);
    if let Some(section) = &p.section_end {
        write_section(s, section);
    }
    s.push_str("</w:pPr>");
}

fn write_run(s: &mut String, r: &Run) {
    s.push_str("<w:r>");
    write_run_props(s, &r.props);
    for c in &r.contents {
        match c {
            RunContent::Text(t) => {
                let mut t = t.clone();
                // 文本里的换行/制表符不是合法 XML 内容，必须转成对应元素
                let mut seg = String::new();
                for ch in t.drain(..) {
                    match ch {
                        '\n' => {
                            flush_text(s, &mut seg);
                            s.push_str("<w:br/>");
                        }
                        '\t' => {
                            flush_text(s, &mut seg);
                            s.push_str("<w:tab/>");
                        }
                        '\r' => {}
                        _ => seg.push(ch),
                    }
                }
                flush_text(s, &mut seg);
            }
            RunContent::Break(k) => {
                let ty = match k {
                    BreakKind::Line => "",
                    BreakKind::Page => r#" w:type="page""#,
                    BreakKind::Column => r#" w:type="column""#,
                };
                s.push_str(&format!("<w:br{ty}/>"));
            }
            RunContent::Tab => s.push_str("<w:tab/>"),
        }
    }
    s.push_str("</w:r>");
}

/// 样式定义里的 `w:pPr` 不允许再带 `w:pStyle`（会造成样式自引用）。
fn strip_style_ref(p: &ParagraphProps) -> ParagraphProps {
    let mut c = p.clone();
    c.style_id = None;
    c
}

fn flush_text(s: &mut String, seg: &mut String) {    if seg.is_empty() {
        return;
    }
    let preserve = if zen_opc::opc::needs_space_preserve(seg) {
        r#" xml:space="preserve""#
    } else {
        ""
    };
    s.push_str(&format!("<w:t{preserve}>{}</w:t>", zen_opc::preserve::xml_text(seg)));
    seg.clear();
}

fn write_run_props(s: &mut String, r: &RunProps) {
    if r.is_empty() {
        return;
    }
    s.push_str("<w:rPr>");
    if let Some(style) = &r.style_id {
        s.push_str(&format!(r#"<w:rStyle w:val="{}"/>"#, zen_opc::preserve::xml_text(style)));
    }
    // Supported CT_RPr order: rFonts → b → i → caps → strike → color → sz.
    if r.font_ascii.is_some() || r.font_east_asia.is_some() {
        s.push_str("<w:rFonts");
        if let Some(f) = &r.font_ascii {
            s.push_str(&format!(r#" w:ascii="{}" w:hAnsi="{}""#, zen_opc::preserve::xml_text(f), zen_opc::preserve::xml_text(f)));
        }
        if let Some(f) = &r.font_east_asia {
            s.push_str(&format!(r#" w:eastAsia="{}""#, zen_opc::preserve::xml_text(f)));
        }
        s.push_str("/>");
    }
    if r.bold == Some(true) {
        s.push_str("<w:b/>");
    } else if r.bold == Some(false) {
        s.push_str(r#"<w:b w:val="0"/>"#);
    }
    if r.italic == Some(true) {
        s.push_str("<w:i/>");
    } else if r.italic == Some(false) {
        s.push_str(r#"<w:i w:val="0"/>"#);
    }
    if r.caps == Some(true) {
        s.push_str("<w:caps/>");
    } else if r.caps == Some(false) {
        s.push_str(r#"<w:caps w:val="0"/>"#);
    }
    if r.strike == Some(true) {
        s.push_str("<w:strike/>");
    } else if r.strike == Some(false) {
        s.push_str(r#"<w:strike w:val="0"/>"#);
    }
    if r.color_auto {
        s.push_str(r#"<w:color w:val="auto"/>"#);
    } else if let Some(c) = &r.color {
        s.push_str(&format!(r#"<w:color w:val="{}"/>"#, zen_opc::preserve::xml_text(c)));
    }
    if let Some(sz) = r.size_half_points {
        s.push_str(&format!(r#"<w:sz w:val="{sz}"/><w:szCs w:val="{sz}"/>"#));
    }
    if let Some(h) = r.highlight {
        s.push_str(&format!(r#"<w:highlight w:val="{}"/>"#, h.as_ooxml()));
    }
    if let Some(u) = r.underline {
        s.push_str(&format!(r#"<w:u w:val="{}"/>"#, u.as_ooxml()));
    }
    if let Some(va) = r.vert_align {
        s.push_str(&format!(r#"<w:vertAlign w:val="{}"/>"#, va.as_ooxml()));
    }
    s.push_str("</w:rPr>");
}

fn write_table(s: &mut String, t: &Table) {
    s.push_str("<w:tbl><w:tblPr>");
    // CT_TblPr 序列顺序：tblStyle → tblW → tblBorders
    if let Some(st) = &t.props.style_id {
        s.push_str(&format!(r#"<w:tblStyle w:val="{}"/>"#, zen_opc::preserve::xml_text(st)));
    }
    match t.props.width_twips {
        Some(w) => s.push_str(&format!(r#"<w:tblW w:w="{w}" w:type="dxa"/>"#)),
        None => s.push_str(r#"<w:tblW w:w="0" w:type="auto"/>"#),
    }
    if t.props.borders {
        s.push_str(
            r#"<w:tblBorders><w:top w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:left w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:bottom w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:right w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:insideH w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:insideV w:val="single" w:sz="4" w:space="0" w:color="auto"/></w:tblBorders>"#,
        );
    }
    s.push_str("</w:tblPr>");

    if !t.grid_twips.is_empty() {
        s.push_str("<w:tblGrid>");
        for w in &t.grid_twips {
            s.push_str(&format!(r#"<w:gridCol w:w="{w}"/>"#));
        }
        s.push_str("</w:tblGrid>");
    }

    for row in &t.rows {
        s.push_str("<w:tr>");
        for cell in &row.cells {
            s.push_str("<w:tc><w:tcPr>");
            // CT_TcPr 序列顺序：tcW → gridSpan → vMerge
            if let Some(w) = cell.width_twips {
                s.push_str(&format!(r#"<w:tcW w:w="{w}" w:type="dxa"/>"#));
            }
            if cell.grid_span > 1 {
                s.push_str(&format!(r#"<w:gridSpan w:val="{}"/>"#, cell.grid_span));
            }
            match cell.v_merge {
                Some(VMerge::Restart) => s.push_str(r#"<w:vMerge w:val="restart"/>"#),
                Some(VMerge::Continue) => s.push_str("<w:vMerge/>"),
                None => {}
            }
            if let Some(shd) = &cell.shading {
                s.push_str(&format!(r#"<w:shd w:val="clear" w:color="auto" w:fill="{}"/>"#, zen_opc::preserve::xml_text(&shd.fill_hex)));
            }
            s.push_str("</w:tcPr>");
            if cell.blocks.is_empty() {
                // 单元格必须至少有一个块级元素
                s.push_str("<w:p/>");
            } else {
                for b in &cell.blocks {
                    write_block(s, b);
                }
            }
            s.push_str("</w:tc>");
        }
        s.push_str("</w:tr>");
    }
    s.push_str("</w:tbl>");
}

fn write_section(s: &mut String, sec: &SectionProps) {
    s.push_str("<w:sectPr>");
    if sec.start != SectionStart::NextPage {
        s.push_str(&format!(r#"<w:type w:val="{}"/>"#, zen_opc::preserve::xml_text(sec.start.as_ooxml())));
    }
    s.push_str(&format!(r#"<w:pgSz w:w="{}" w:h="{}""#, sec.page_width_twips, sec.page_height_twips));
    if sec.orientation == Orientation::Landscape {
        s.push_str(r#" w:orient="landscape""#);
    }
    s.push_str("/>");
    s.push_str(&format!(
        r#"<w:pgMar w:top="{}" w:right="{}" w:bottom="{}" w:left="{}" w:header="{}" w:footer="{}" w:gutter="{}"/>"#,
        sec.margin_top_twips, sec.margin_right_twips, sec.margin_bottom_twips, sec.margin_left_twips,
        sec.header_distance_twips, sec.footer_distance_twips, sec.gutter_twips
    ));
    if sec.page_number_start.is_some() || sec.page_number_format.is_some() {
        s.push_str("<w:pgNumType");
        if let Some(format) = &sec.page_number_format {
            s.push_str(&format!(r#" w:fmt="{}""#, zen_opc::preserve::xml_text(format)));
        }
        if let Some(start) = sec.page_number_start {
            s.push_str(&format!(r#" w:start="{start}""#));
        }
        s.push_str("/>");
    }
    if sec.column_count > 1 {
        s.push_str(&format!(r#"<w:cols w:num="{}" w:space="{}"/>"#, sec.column_count, sec.column_space_twips));
    }
    if sec.title_page {
        s.push_str("<w:titlePg/>");
    }
    s.push_str("</w:sectPr>");
}

// ---------------------------------------------------------------- styles.xml

pub fn styles_xml(doc: &Document) -> String {
    let mut s = String::with_capacity(1024);
    s.push_str(XML_DECL);
    s.push_str(&format!(r#"<w:styles xmlns:w="{W_NS}">"#));

    // docDefaults
    s.push_str("<w:docDefaults>");
    s.push_str("<w:rPrDefault>");
    if doc.styles.default_run.is_empty() {
        s.push_str("<w:rPr/>");
    } else {
        s.push_str("<w:rPr>");
        let mut tmp = String::new();
        write_run_props(&mut tmp, &doc.styles.default_run);
        s.push_str(tmp.trim_start_matches("<w:rPr>").trim_end_matches("</w:rPr>"));
        s.push_str("</w:rPr>");
    }
    s.push_str("</w:rPrDefault>");
    if doc.styles.default_para.is_empty() {
        s.push_str("<w:pPrDefault/>");
    } else {
        s.push_str("<w:pPrDefault>");
        let mut tmp = String::new();
        write_paragraph_props(&mut tmp, &strip_style_ref(&doc.styles.default_para));
        s.push_str(tmp.trim_start_matches("<w:pPr>").trim_end_matches("</w:pPr>"));
        s.push_str("</w:pPrDefault>");
    }
    s.push_str("</w:docDefaults>");

    // 内置样式：由 `template_style_defs()` 单一事实源驱动生成
    for template in template_style_defs() {
        let st = doc.styles.builtin_overrides.iter().find(|s| s.style_id == template.style_id).unwrap_or(&template);
        let default_id = doc.styles.default_paragraph_style_id.as_deref().unwrap_or(STYLE_NORMAL);
        let default_attr = if st.kind == StyleKind::Paragraph && st.style_id == default_id { r#" w:default="1""# } else { "" };
        s.push_str(&format!(
            r#"<w:style w:type="{}"{default_attr} w:styleId="{}"><w:name w:val="{}"/>"#,
            st.kind.as_ooxml(),
            zen_opc::preserve::xml_text(&st.style_id),
            zen_opc::preserve::xml_text(&st.name)
        ));
        if let Some(b) = &st.based_on {
            s.push_str(&format!(
                r#"<w:basedOn w:val="{}"/><w:next w:val="{}"/>"#,
                zen_opc::preserve::xml_text(b),
                zen_opc::preserve::xml_text(b)
            ));
        }
        s.push_str("<w:qFormat/>");
        if !st.para.is_empty() {
            let mut tmp = String::new();
            write_paragraph_props(&mut tmp, &strip_style_ref(&st.para));
            s.push_str(&tmp);
        }
        if !st.run.is_empty() {
            let mut tmp = String::new();
            write_run_props(&mut tmp, &st.run);
            s.push_str(&tmp);
        }
        s.push_str("</w:style>");
    }

    // 模型里的自定义样式（跳过内置 ID —— 它们已由模板生成）
    for st in &doc.styles.named {
        if is_template_style_id(&st.style_id) {
            continue;
        }
        s.push_str(&format!(
            r#"<w:style w:type="{}"{} w:styleId="{}"><w:name w:val="{}"/>"#,
            st.kind.as_ooxml(),
            if (st.kind == StyleKind::Paragraph && doc.styles.default_paragraph_style_id.as_deref() == Some(&st.style_id)) || (st.kind == StyleKind::Character && doc.styles.default_character_style_id.as_deref() == Some(&st.style_id)) { r#" w:default="1""# } else { "" },
            zen_opc::preserve::xml_text(&st.style_id),
            zen_opc::preserve::xml_text(&st.name)
        ));
        if let Some(b) = &st.based_on {
            s.push_str(&format!(r#"<w:basedOn w:val="{}"/>"#, zen_opc::preserve::xml_text(b)));
        }
        if !st.para.is_empty() {
            let mut tmp = String::new();
            write_paragraph_props(&mut tmp, &strip_style_ref(&st.para));
            s.push_str(&tmp);
        }
        if !st.run.is_empty() {
            let mut tmp = String::new();
            write_run_props(&mut tmp, &st.run);
            s.push_str(&tmp);
        }
        s.push_str("</w:style>");
    }

    s.push_str("</w:styles>");
    s
}

// ---------------------------------------------------------------- settings / fontTable / docProps

pub fn settings_xml() -> String {
    settings_xml_for(&DocumentSettings::default())
}

fn settings_xml_for(settings: &DocumentSettings) -> String {
    // compatibilityMode=15（Word 2013+）：避免 Word 以兼容模式打开后改写我们的内容
    format!(
        r#"{XML_DECL}<w:settings xmlns:w="{W_NS}"><w:zoom w:percent="100"/><w:defaultTabStop w:val="{}"/><w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat></w:settings>"#,
        settings.default_tab_stop_twips
    )
}

/// 收集文档里用到的字体名，生成最小 fontTable。
pub fn referenced_fonts(doc: &Document) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |r: &RunProps, out: &mut Vec<String>| {
        for f in [&r.font_ascii, &r.font_east_asia].into_iter().flatten() {
            if !out.iter().any(|x| x == f) {
                out.push(f.clone());
            }
        }
    };
    push(&doc.styles.default_run, &mut out);
    for st in doc.styles.named.iter().chain(&doc.styles.builtin_overrides) {
        push(&st.run, &mut out);
    }
    fn walk(blocks: &[Block], push: &mut impl FnMut(&RunProps, &mut Vec<String>), out: &mut Vec<String>) {
        for b in blocks {
            match b {
                Block::Paragraph(p) => {
                    push(&p.props.mark_run, out);
                    for r in &p.runs {
                        push(&r.props, out);
                    }
                }
                Block::Table(t) => {
                    for row in &t.rows {
                        for c in &row.cells {
                            walk(&c.blocks, push, out);
                        }
                    }
                }
            }
        }
    }
    walk(&doc.blocks, &mut push, &mut out);
    out
}

pub fn font_table_xml(doc: &Document) -> String {
    let mut s = format!(r#"{XML_DECL}<w:fonts xmlns:w="{W_NS}">"#);
    for f in referenced_fonts(doc) {
        s.push_str(&format!(
            r#"<w:font w:name="{}"><w:family w:val="auto"/><w:pitch w:val="variable"/></w:font>"#,
            zen_opc::preserve::xml_text(&f)
        ));
    }
    s.push_str("</w:fonts>");
    s
}

pub fn core_xml(core: &CoreProps) -> String {
    let mut s = format!(
        r#"{XML_DECL}<cp:coreProperties xmlns:cp="{CP_NS}" xmlns:dc="{DC_NS}" xmlns:dcterms="{DCTERMS_NS}" xmlns:xsi="{XSI_NS}">"#
    );
    // cp 序列顺序：created → creator → keywords → lastModifiedBy → modified → subject → title
    if let Some(v) = &core.created {
        s.push_str(&format!(
            r#"<dcterms:created xsi:type="dcterms:W3CDTF">{}</dcterms:created>"#,
            zen_opc::preserve::xml_text(v)
        ));
    }
    if let Some(v) = &core.creator {
        s.push_str(&format!("<dc:creator>{}</dc:creator>", zen_opc::preserve::xml_text(v)));
    }
    if let Some(v) = &core.keywords {
        s.push_str(&format!("<cp:keywords>{}</cp:keywords>", zen_opc::preserve::xml_text(v)));
    }
    if let Some(v) = &core.last_modified_by {
        s.push_str(&format!(
            "<cp:lastModifiedBy>{}</cp:lastModifiedBy>",
            zen_opc::preserve::xml_text(v)
        ));
    }
    if let Some(v) = &core.modified {
        s.push_str(&format!(
            r#"<dcterms:modified xsi:type="dcterms:W3CDTF">{}</dcterms:modified>"#,
            zen_opc::preserve::xml_text(v)
        ));
    }
    if let Some(v) = &core.subject {
        s.push_str(&format!("<dc:subject>{}</dc:subject>", zen_opc::preserve::xml_text(v)));
    }
    if let Some(v) = &core.title {
        s.push_str(&format!("<dc:title>{}</dc:title>", zen_opc::preserve::xml_text(v)));
    }
    s.push_str("</cp:coreProperties>");
    s
}

pub fn app_xml() -> String {
    format!(
        r#"{XML_DECL}<Properties xmlns="{EXT_PROPS_NS}"><DocSecurity>0</DocSecurity><Application>RustOffice</Application><AppVersion>0.1</AppVersion></Properties>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_xml_has_required_shape() {
        let doc = Document { blocks: vec![Block::paragraph("你好")], ..Default::default() };
        let xml = document_xml(&doc);
        assert!(xml.starts_with(XML_DECL));
        assert!(xml.contains("<w:body>"));
        assert!(xml.ends_with("</w:body></w:document>"));
        assert!(xml.contains("<w:sectPr>"), "必须有节属性");
        // sectPr 必须是 body 的最后一个子元素
        let body_end = xml.find("</w:body>").unwrap();
        let sect = xml.find("<w:sectPr>").unwrap();
        assert!(sect < body_end);
    }

    #[test]
    fn text_escaping_and_space_preserve() {
        let doc = Document {
            blocks: vec![Block::paragraph("  a<b>&c  ")],
            ..Default::default()
        };
        let xml = document_xml(&doc);
        assert!(xml.contains(r#"<w:t xml:space="preserve">  a&lt;b&gt;&amp;c  </w:t>"#));
    }

    #[test]
    fn newline_becomes_break_element_not_raw_control_char() {
        let doc = Document { blocks: vec![Block::paragraph("第一行\n第二行")], ..Default::default() };
        let xml = document_xml(&doc);
        assert!(xml.contains("<w:br/>"));
        assert!(!xml.contains('\n') || !xml.contains("第一行\n"), "文本里的裸换行不能进 XML");
    }

    #[test]
    fn empty_run_is_normalized_away() {
        let doc = Document {
            blocks: vec![Block::Paragraph(Paragraph {
                props: ParagraphProps::default(),
                runs: vec![Run::text(""), Run::text("有效")],
            })],
            ..Default::default()
        };
        let xml = document_xml(&doc);
        assert_eq!(xml.matches("<w:r>").count(), 1);
    }

    #[test]
    fn property_order_follows_schema_sequence() {
        let doc = Document {
            blocks: vec![Block::Paragraph(Paragraph {
                props: ParagraphProps {
                    style_id: Some("Heading1".into()),
                    page_break_before: true,
                    space_after_twips: Some(120),
                    ind_first_line_twips: Some(420),
                    align: Some(Align::Center),
                    ..Default::default()
                },
                runs: vec![Run::styled(
                    RunProps {
                        font_ascii: Some("Calibri".into()),
                        bold: Some(true),
                        color: Some("FF0000".into()),
                        size_half_points: Some(24),
                        underline: Some(Underline::Single),
                        ..Default::default()
                    },
                    "x",
                )],
            })],
            ..Default::default()
        };
        let xml = document_xml(&doc);
        let order = ["<w:pStyle", "<w:pageBreakBefore", "<w:spacing", "<w:ind", "<w:jc"];
        let mut last = 0usize;
        for tag in order {
            let at = xml.find(tag).unwrap_or_else(|| panic!("缺少 {tag}"));
            assert!(at > last, "{tag} 顺序不对");
            last = at;
        }
        let rorder = ["<w:rFonts", "<w:b/>", "<w:color", "<w:sz ", "<w:u "];
        let mut last = 0usize;
        for tag in rorder {
            let at = xml.find(tag).unwrap_or_else(|| panic!("缺少 {tag}"));
            assert!(at > last, "{tag} 顺序不对");
            last = at;
        }
    }

    #[test]
    fn table_cell_always_has_a_block() {
        let doc = Document {
            blocks: vec![Block::Table(
                Table::new(vec![2000, 2000]).push_row(TableRow {
                    cells: vec![
                        TableCell { width_twips: Some(2000), ..Default::default() },
                        TableCell {
                            width_twips: Some(2000),
                            blocks: vec![Block::paragraph("有内容")],
                            ..Default::default()
                        },
                    ],
                }),
            )],
            ..Default::default()
        };
        let xml = document_xml(&doc);
        assert!(xml.contains("<w:tc><w:tcPr><w:tcW w:w=\"2000\" w:type=\"dxa\"/></w:tcPr><w:p/></w:tc>"));
        assert!(xml.contains("<w:tblGrid><w:gridCol w:w=\"2000\"/><w:gridCol w:w=\"2000\"/></w:tblGrid>"));
    }

    #[test]
    fn portrait_orientation_is_omitted() {
        let doc = Document::default();
        let xml = document_xml(&doc);
        assert!(!xml.contains("w:orient"), "纵向是缺省值，不该写出来");
    }

    #[test]
    fn styles_xml_always_has_normal() {
        let doc = Document::default();
        let xml = styles_xml(&doc);
        assert!(xml.contains(r#"w:default="1" w:styleId="Normal""#), "{xml}");
        assert!(xml.contains(r#"<w:outlineLvl w:val="0"/>"#), "Heading1 要有大纲级别");
        assert!(xml.contains(r#"<w:keepNext/>"#), "Heading1 要 keepNext");
    }

    #[test]
    fn rich_formatting_order_and_content() {
        let mut doc = Document::default();
        doc.section.column_count = 2;
        doc.section.column_space_twips = 720;

        let mut p = Paragraph::default();
        p.props.shading = Some(Shading::new("F2F4F7"));
        let mut r = Run::text("Highlighted Text");
        r.props.highlight = Some(HighlightColor::Yellow);
        p.runs.push(r);
        doc.blocks.push(Block::Paragraph(p));

        let mut tbl = Table::new(vec![4000]);
        let mut cell = TableCell::default();
        cell.width_twips = Some(4000);
        cell.shading = Some(Shading::new("E0F2FE"));
        cell.blocks.push(Block::paragraph("Cell with shading"));
        let row = TableRow { cells: vec![cell] };
        tbl.rows.push(row);
        doc.blocks.push(Block::Table(tbl));

        let xml = document_xml(&doc);
        assert!(xml.contains(r#"<w:shd w:val="clear" w:color="auto" w:fill="F2F4F7"/>"#));
        assert!(xml.contains(r#"<w:highlight w:val="yellow"/>"#));
        assert!(xml.contains(r#"<w:shd w:val="clear" w:color="auto" w:fill="E0F2FE"/>"#));
        assert!(xml.contains(r#"<w:cols w:num="2" w:space="720"/>"#));

        // 验证读写往返
        let bytes = write_docx(&doc).unwrap();
        let read_back = crate::read::read_docx(&bytes).unwrap().document;
        assert_eq!(read_back.section.column_count, 2);
        assert_eq!(read_back.section.column_space_twips, 720);
        if let Block::Paragraph(p) = &read_back.blocks[0] {
            assert_eq!(p.props.shading.as_ref().unwrap().fill_hex, "F2F4F7");
            assert_eq!(p.runs[0].props.highlight, Some(HighlightColor::Yellow));
        } else {
            panic!("First block should be paragraph");
        }
        if let Block::Table(t) = &read_back.blocks[1] {
            assert_eq!(t.rows[0].cells[0].shading.as_ref().unwrap().fill_hex, "E0F2FE");
        } else {
            panic!("Second block should be table");
        }
    }
}
