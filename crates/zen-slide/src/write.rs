//! Presentation 模型 → PPTX 包（写出）
//! 遵循 ECMA-376 Part 1 Chapter 19 (PresentationML) 及 DrawingML 规范。

use crate::model::*;
use crate::Result;
use zen_opc::{ContentTypes, Package, Relationships};

pub const P_NS: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
pub const A_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
pub const R_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

pub const CT_PRESENTATION: &str =
    "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml";
pub const CT_SLIDE: &str =
    "application/vnd.openxmlformats-officedocument.presentationml.slide+xml";
pub const CT_SLIDE_MASTER: &str =
    "application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml";
pub const CT_SLIDE_LAYOUT: &str =
    "application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml";
pub const CT_CORE: &str = "application/vnd.openxmlformats-package.core-properties+xml";
pub const CT_THEME: &str =
    "application/vnd.openxmlformats-officedocument.theme+xml";
pub const CT_TABLE_STYLES: &str =
    "application/vnd.openxmlformats-officedocument.presentationml.tableStyles+xml";
pub const CT_PRES_PROPS: &str =
    "application/vnd.openxmlformats-officedocument.presentationml.presProps+xml";

pub const REL_THEME: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme";
pub const REL_TABLE_STYLES: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/tableStyles";
pub const REL_PRES_PROPS: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/presProps";


pub const REL_OFFICE_DOCUMENT: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
pub const REL_SLIDE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide";
pub const REL_SLIDE_LAYOUT: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout";
pub const REL_SLIDE_MASTER: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster";
pub const REL_CORE: &str =
    "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties";

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

pub fn write_pptx(presentation: &Presentation) -> Result<Vec<u8>> {
    if let Some(source) = &presentation.preservation { return crate::preserve::save(presentation, source); }
    let pkg = build_package(presentation)?;
    Ok(pkg.to_bytes()?)
}

pub fn build_package(presentation: &Presentation) -> Result<Package> {
    if let Some(source) = &presentation.preservation {
        return Ok(Package::from_archive(&zen_opc::ZipArchive::read(&crate::preserve::save(presentation, source)?)?)?);
    }
    if presentation.slides.iter().flat_map(|s| &s.shapes).any(|s| s.width <= 0 || s.height <= 0) {
        return Err(crate::SlideError::Invalid("new shapes require positive dimensions".into()));
    }
    let mut pkg = Package::with_common_defaults();

    // 1. Core Metadata
    pkg.put_text("/docProps/core.xml", core_xml(presentation))?;

    // 2. Presentation.xml
    pkg.put_text("/ppt/presentation.xml", presentation_xml(presentation))?;

    // 3. Theme, TableStyles, PresProps (Office 2024 LTSC 必需部件)
    pkg.put_text("/ppt/theme/theme1.xml", theme_xml())?;
    pkg.put_text("/ppt/tableStyles.xml", table_styles_xml())?;
    pkg.put_text("/ppt/presProps.xml", pres_props_xml())?;

    // 3. SlideMaster & SlideLayout (最小有效骨架，保证在 PowerPoint 与 WPS 中秒开不报错)
    pkg.put_text("/ppt/slideMasters/slideMaster1.xml", slide_master_xml())?;
    pkg.put_text("/ppt/slideLayouts/slideLayout1.xml", slide_layout_xml())?;

    let mut master_rels = Relationships::default();
    master_rels.add("rId1", REL_SLIDE_LAYOUT, "../slideLayouts/slideLayout1.xml");
    master_rels.add("rId2", REL_THEME, "../theme/theme1.xml");
    pkg.set_rels_for("/ppt/slideMasters/slideMaster1.xml", &master_rels)?;

    let mut layout_rels = Relationships::default();
    layout_rels.add("rId1", REL_SLIDE_MASTER, "../slideMasters/slideMaster1.xml");
    pkg.set_rels_for("/ppt/slideLayouts/slideLayout1.xml", &layout_rels)?;

    // 4. Slides
    let mut pres_rels = Relationships::default();
    pres_rels.add("rId1", REL_SLIDE_MASTER, "slideMasters/slideMaster1.xml");

    for (idx, slide) in presentation.slides.iter().enumerate() {
        let slide_num = idx + 1;
        let slide_part = format!("/ppt/slides/slide{slide_num}.xml");
        let rel_id = format!("rId{}", slide_num + 1);

        pkg.put_text(&slide_part, slide_xml(slide))?;

        let mut slide_rels = Relationships::default();
        slide_rels.add("rId1", REL_SLIDE_LAYOUT, "../slideLayouts/slideLayout1.xml");
        pkg.set_rels_for(&slide_part, &slide_rels)?;

        let target_slide = format!("slides/slide{slide_num}.xml");
        pres_rels.add(&rel_id, REL_SLIDE, &target_slide);
    }
        pres_rels.add("rIdTheme", REL_THEME, "theme/theme1.xml");
    pres_rels.add("rIdTableStyles", REL_TABLE_STYLES, "tableStyles.xml");
    pres_rels.add("rIdPresProps", REL_PRES_PROPS, "presProps.xml");
    pkg.set_rels_for("/ppt/presentation.xml", &pres_rels)?;

    // 5. Package Root Relationships
    let mut root_rels = Relationships::default();
    root_rels.add("rId1", REL_OFFICE_DOCUMENT, "ppt/presentation.xml");
    root_rels.add("rId2", REL_CORE, "docProps/core.xml");
    pkg.set_root_rels(&root_rels)?;

    // 6. Content Types
    let mut ct: ContentTypes = pkg.content_types.clone();
    ct.add_override("/docProps/core.xml", CT_CORE)?;
    ct.add_override("/ppt/presentation.xml", CT_PRESENTATION)?;
    ct.add_override("/ppt/slideMasters/slideMaster1.xml", CT_SLIDE_MASTER)?;
    ct.add_override("/ppt/slideLayouts/slideLayout1.xml", CT_SLIDE_LAYOUT)?;
    ct.add_override("/ppt/theme/theme1.xml", CT_THEME)?;
    ct.add_override("/ppt/tableStyles.xml", CT_TABLE_STYLES)?;
    ct.add_override("/ppt/presProps.xml", CT_PRES_PROPS)?;

    for idx in 0..presentation.slides.len() {
        let slide_part = format!("/ppt/slides/slide{}.xml", idx + 1);
        ct.add_override(&slide_part, CT_SLIDE)?;
    }
    pkg.content_types = ct;

    for part in &pkg.parts {
        if part.name.ends_with(".xml") {
            let text=std::str::from_utf8(&part.data).map_err(|_|crate::SlideError::Invalid("non UTF-8 XML output".into()))?;
            roxmltree::Document::parse(text)?;
        }
    }
    Ok(pkg)
}

fn core_xml(pres: &Presentation) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
  <dc:title>{}</dc:title>
  <dc:creator>{}</dc:creator>
  <cp:lastModifiedBy>{}</cp:lastModifiedBy>
</cp:coreProperties>"#,
        escape(&pres.title),
        escape(&pres.author),
        escape(&pres.author)
    )
}

fn presentation_xml(pres: &Presentation) -> String {
    let mut s = String::new();
    s.push_str(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentation xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:sldMasterIdLst>
    <p:sldMasterId id="2147483648" r:id="rId1"/>
  </p:sldMasterIdLst>
  <p:sldIdLst>
"#);

    for (idx, _) in pres.slides.iter().enumerate() {
        let sld_id = 256 + idx as u32;
        let r_id = format!("rId{}", idx + 2);
        s.push_str(&format!(r#"    <p:sldId id="{sld_id}" r:id="{r_id}"/>"#));
        s.push('\n');
    }

    s.push_str(&format!(
        r#"  </p:sldIdLst>
  <p:sldSz cx="{}" cy="{}"/>
  <p:notesSz cx="6858000" cy="9144000"/>
</p:presentation>"#,
        pres.width_emu, pres.height_emu
    ));
    s
}

fn slide_master_xml() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldMaster xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:cSld>
    <p:spTree>
      <p:nvGrpSpPr>
        <p:cNvPr id="1" name=""/>
        <p:cNvGrpSpPr/>
        <p:nvPr/>
      </p:nvGrpSpPr>
      <p:grpSpPr/>
    </p:spTree>
  </p:cSld>
  <p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/>
  <p:sldLayoutIdLst>
    <p:sldLayoutId id="2147483649" r:id="rId1"/>
  </p:sldLayoutIdLst>
</p:sldMaster>"#.into()
}

fn slide_layout_xml() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldLayout xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" type="blank">
  <p:cSld>
    <p:spTree>
      <p:nvGrpSpPr>
        <p:cNvPr id="1" name=""/>
        <p:cNvGrpSpPr/>
        <p:nvPr/>
      </p:nvGrpSpPr>
      <p:grpSpPr/>
    </p:spTree>
  </p:cSld>
  <p:clrMapOvr>
    <a:masterClrMapping/>
  </p:clrMapOvr>
</p:sldLayout>"#.into()
}

fn slide_xml(slide: &Slide) -> String {
    let mut s = String::new();
    s.push_str(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:cSld>
    <p:spTree>
      <p:nvGrpSpPr>
        <p:cNvPr id="1" name=""/>
        <p:cNvGrpSpPr/>
        <p:nvPr/>
      </p:nvGrpSpPr>
      <p:grpSpPr/>
"#);

    for (idx, shape) in slide.shapes.iter().enumerate() {
        let shape_id = 2 + idx as u32;
        let tx_box_attr = if shape.kind == ShapeKind::TextBox { " txBox=\"1\"" } else { "" };
        let prst = match shape.kind {
            ShapeKind::TextBox | ShapeKind::Rectangle => "rect",
            ShapeKind::RoundedRectangle => "roundRect",
            ShapeKind::Ellipse => "ellipse",
        };
        let width = shape.width;
        let height = shape.height;
        let x = shape.x;
        let y = shape.y;

        s.push_str(&format!(
            r#"      <p:sp>
        <p:nvSpPr>
          <p:cNvPr id="{shape_id}" name="{}"/>
          <p:cNvSpPr{tx_box_attr}/>
          <p:nvPr/>
        </p:nvSpPr>
        <p:spPr>
          <a:xfrm>
            <a:off x="{x}" y="{y}"/>
            <a:ext cx="{width}" cy="{height}"/>
          </a:xfrm>
          <a:prstGeom prst="{prst}">
            <a:avLst/>
          </a:prstGeom>
"#,
            escape(&shape.name)
        ));

        if let Some(c) = &shape.fill_color {
            s.push_str(&format!(r#"          <a:solidFill><a:srgbClr val="{}"/></a:solidFill>"#, escape(c)));
        } else {
            s.push_str("          <a:noFill/>\n");
        }
        s.push_str("        </p:spPr>\n");

        if let Some(tb) = &shape.text_body {
            s.push_str("        <p:txBody>\n          <a:bodyPr rtlCol=\"0\" anchor=\"t\"/>\n          <a:lstStyle/>\n");
            if tb.paragraphs.is_empty() {
                s.push_str("          <a:p><a:endParaRPr lang=\"en-US\" dirty=\"0\"/></a:p>\n");
            } else {
                for p in &tb.paragraphs {
                    let al = match p.align {
                        Some(TextAlign::Center) => " algn=\"ctr\"",
                        Some(TextAlign::Right) => " algn=\"r\"",
                        _ => "",
                    };
                    s.push_str(&format!("          <a:p{al}>\n"));
                    for r in &p.runs {
                        s.push_str("            <a:r>\n              <a:rPr");
                        if let Some(sz) = r.size_hundredth_pt {
                            s.push_str(&format!(" sz=\"{sz}\""));
                        }
                        if r.bold {
                            s.push_str(" b=\"1\"");
                        }
                        if r.italic {
                            s.push_str(" i=\"1\"");
                        }
                        s.push_str(">\n");
                        if let Some(col) = &r.color {
                            s.push_str(&format!("                <a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill>\n", escape(col)));
                        }
                        s.push_str(&format!("              </a:rPr>\n              <a:t>{}</a:t>\n            </a:r>\n", escape(&r.text)));
                    }
                    s.push_str("            <a:endParaRPr lang=\"en-US\" dirty=\"0\"/>\n");
                    s.push_str("          </a:p>\n");
                }
            }
            s.push_str("        </p:txBody>\n");
        }
        s.push_str("      </p:sp>\n");
    }

    s.push_str(r#"    </p:spTree>
  </p:cSld>
  <p:clrMapOvr>
    <a:masterClrMapping/>
  </p:clrMapOvr>
</p:sld>"#);
    s
}

fn table_styles_xml() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<a:tblStyleLst xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" def="{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}"/>"#.into()
}

fn pres_props_xml() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentationPr xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"/>"#.into()
}

fn theme_xml() -> String {
    include_str!("../assets/theme1.xml").into()
}
