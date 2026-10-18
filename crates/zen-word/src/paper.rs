//! Explicit paper delivery profile. This operation changes layout, whereas an
//! ordinary imported-document save must preserve the author's section geometry.
use crate::{Document, Result, WordError};
use roxmltree::Node;
use std::{collections::BTreeMap, ops::Range};
use zen_opc::preserve;

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const WP: &str = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
const A4_W: i64 = 11906;
const A4_H: i64 = 16838;
type Patch = (Range<usize>, String);

fn reject(message: &str) -> WordError {
    WordError::UnsupportedEdit(format!("A4 paper profile: {message}"))
}
fn child<'a, 'i>(node: Node<'a, 'i>, name: &str) -> Result<Node<'a, 'i>> {
    let mut matches = node.children().filter(|n| n.has_tag_name((W, name)));
    let first = matches
        .next()
        .ok_or_else(|| reject(&format!("missing {name}")))?;
    if matches.next().is_some() {
        return Err(reject(&format!("duplicate {name}")));
    }
    Ok(first)
}
fn integer(node: Node<'_, '_>, attr: &str) -> Result<i64> {
    node.attribute((W, attr))
        .ok_or_else(|| reject(&format!("missing {attr}")))?
        .parse()
        .map_err(|_| reject("invalid numeric geometry"))
}
// Namespace-qualified replacements affect only the explicit geometry nodes;
// opaque objects, text, fields and relationship parts are never reserialized.
fn element(name: &str, attrs: &[(&str, i64)]) -> String {
    let mut tag = format!("<z:{name} xmlns:z=\"{W}\"");
    for (key, value) in attrs {
        tag.push_str(&format!(" z:{key}=\"{value}\""));
    }
    tag.push_str("/>");
    tag
}
fn width_element(name: &str, width: i64) -> String {
    format!("<z:{name} xmlns:z=\"{W}\" z:w=\"{width}\" z:type=\"dxa\"/>")
}
fn widths_fit_table(table: Node<'_, '_>, available: i64, edits: &mut Vec<Patch>) -> Result<()> {
    let grid = child(table, "tblGrid")?;
    let cols: Vec<_> = grid
        .children()
        .filter(|n| n.has_tag_name((W, "gridCol")))
        .collect();
    let widths: Vec<_> = cols
        .iter()
        .map(|n| integer(*n, "w"))
        .collect::<Result<_>>()?;
    if widths.is_empty() || widths.iter().any(|w| *w <= 0 || *w > 31680) {
        return Err(reject("invalid table grid"));
    }
    let total: i64 = widths.iter().sum();
    let props = child(table, "tblPr")?;
    let table_width = child(props, "tblW")?;
    let rows: Vec<_> = table
        .children()
        .filter(|n| n.has_tag_name((W, "tr")))
        .collect();
    let preferred = match table_width.attribute((W, "type")) {
        Some("dxa") => integer(table_width, "w")?,
        Some("auto") => 0,
        _ => return Err(reject("table width must be auto or dxa")),
    };
    let mut row_widths = Vec::new();
    for row in &rows {
        let cells: Vec<_> = row
            .children()
            .filter(|n| n.has_tag_name((W, "tc")))
            .collect();
        let mut row_total = 0;
        for cell in &cells {
            let tcw = child(child(*cell, "tcPr")?, "tcW")?;
            if tcw.attribute((W, "type")) != Some("dxa") {
                return Err(reject("table cell width must be dxa"));
            }
            let value = integer(tcw, "w")?;
            if !(1..=31680).contains(&value) {
                return Err(reject("invalid cell width"));
            }
            row_total += value;
        }
        row_widths.push(row_total);
    }
    if total <= available && preferred <= available && row_widths.iter().all(|w| *w <= available) {
        return Ok(());
    }
    // Resizing merged, nested or positioned tables needs a separate algorithm.
    if table.descendants().any(|n| {
        n.is_element()
            && (n.has_tag_name((W, "tbl")) && n != table
                || n.tag_name().namespace() == Some(W)
                    && matches!(
                        n.tag_name().name(),
                        "gridSpan"
                            | "vMerge"
                            | "hMerge"
                            | "gridBefore"
                            | "gridAfter"
                            | "tblpPr"
                            | "tblCellSpacing"
                            | "drawing"
                            | "pict"
                            | "noWrap"
                            | "tcFitText"
                    ))
    }) {
        return Err(reject(
            "wide table contains merged, nested, floating or fixed content",
        ));
    }
    if table
        .descendants()
        .any(|n| n.has_tag_name((W, "trHeight")) && n.attribute((W, "hRule")) == Some("exact"))
    {
        return Err(reject("fixed row height could clip wrapped content"));
    }
    if props
        .children()
        .any(|n| n.has_tag_name((W, "tblInd")) && integer(n, "w").ok() != Some(0))
    {
        return Err(reject("indented wide table"));
    }
    let layout = child(props, "tblLayout")?;
    if layout.attribute((W, "type")) != Some("fixed") {
        return Err(reject("wide table must have a fixed grid"));
    }
    // Reserve at least half an inch per column so identifiers do not get
    // needlessly broken into individual characters after proportional fitting.
    const MIN_COLUMN: i64 = 720;
    if available < cols.len() as i64 * MIN_COLUMN {
        return Err(reject(
            "too many columns for readable portrait output; split the table",
        ));
    }
    let mut resized = vec![0; cols.len()];
    let mut pending: Vec<_> = (0..cols.len()).collect();
    let mut remaining = available;
    while !pending.is_empty() {
        let weight: i64 = pending.iter().map(|i| widths[*i]).sum();
        let narrow: Vec<_> = pending
            .iter()
            .copied()
            .filter(|i| remaining * widths[*i] / weight < MIN_COLUMN)
            .collect();
        if narrow.is_empty() {
            let budget = remaining;
            for (position, i) in pending.iter().enumerate() {
                let value = if position + 1 == pending.len() {
                    remaining
                } else {
                    budget * widths[*i] / weight
                };
                resized[*i] = value;
                remaining -= value;
            }
            break;
        }
        for i in &narrow {
            resized[*i] = MIN_COLUMN;
            remaining -= MIN_COLUMN;
        }
        pending.retain(|i| !narrow.contains(i));
    }
    for row in rows {
        let cells: Vec<_> = row
            .children()
            .filter(|n| n.has_tag_name((W, "tc")))
            .collect();
        if cells.len() != cols.len() {
            return Err(reject("non-rectangular wide table"));
        }
        for (cell, width) in cells.iter().zip(&resized) {
            let tcw = child(child(*cell, "tcPr")?, "tcW")?;
            edits.push((tcw.range(), width_element("tcW", *width)));
        }
    }
    for (col, width) in cols.iter().zip(&resized) {
        edits.push((col.range(), element("gridCol", &[("w", *width)])));
    }
    edits.push((table_width.range(), width_element("tblW", available)));
    Ok(())
}

impl Document {
    /// Deliver a uniform A4 portrait paper while retaining section breaks,
    /// margins, headers/footers and page-number restarts. Fit simple wide tables
    /// by changing widths and wrapping text; font sizes are never reduced.
    /// Unsupported layouts are rejected before producing output. The source
    /// model and source package remain unchanged.
    pub fn write_paper_a4_portrait(&self) -> Result<Vec<u8>> {
        let bytes = crate::write_docx(self)?;
        crate::preserve::validate_edit_package(&bytes)?;
        let archive = zen_opc::ZipArchive::read(&bytes)?;
        let raw = archive
            .get("word/document.xml")
            .ok_or_else(|| reject("missing main part"))?;
        let xml = std::str::from_utf8(raw).map_err(|_| reject("main part is not UTF-8"))?;
        let parsed = roxmltree::Document::parse(xml).map_err(|e| WordError::BadXml {
            part: "word/document.xml".into(),
            source: e,
        })?;
        let body = child(parsed.root_element(), "body")?;
        let sections: Vec<_> = body
            .descendants()
            .filter(|n| n.has_tag_name((W, "sectPr")))
            .collect();
        if sections.is_empty() {
            return Err(reject("missing explicit section geometry"));
        }
        // No hidden alternative or geometry-dependent content may be guessed.
        for entry in archive
            .entries
            .iter()
            .filter(|e| e.name.starts_with("word/") && e.name.ends_with(".xml"))
        {
            let text = std::str::from_utf8(&entry.data).map_err(|_| reject("part is not UTF-8"))?;
            let tree = roxmltree::Document::parse(text).map_err(|_| reject("invalid XML part"))?;
            if tree.descendants().any(|n| {
                n.is_element()
                    && (n.has_tag_name((WP, "anchor"))
                        || n.tag_name().namespace() == Some(W)
                            && matches!(
                                n.tag_name().name(),
                                "sectPrChange"
                                    | "tblPrChange"
                                    | "tcPrChange"
                                    | "ins"
                                    | "del"
                                    | "moveFrom"
                                    | "moveTo"
                                    | "pict"
                                    | "txbxContent"
                                    | "altChunk"
                            )
                        || n.tag_name().name() == "AlternateContent")
            }) {
                return Err(reject(
                    "tracked, alternative, floating or legacy content requires manual layout",
                ));
            }
        }
        let mut edits = Vec::new();
        let mut minimum_width = A4_W;
        for section in &sections {
            let size = child(*section, "pgSz")?;
            if size.attributes().any(|a| {
                a.namespace() != Some(W) || !["w", "h", "orient", "code"].contains(&a.name())
            }) {
                return Err(reject("unknown paper-size attributes"));
            }
            let margins = child(*section, "pgMar")?;
            let left = integer(margins, "left")?;
            let right = integer(margins, "right")?;
            let gutter = margins
                .attribute((W, "gutter"))
                .unwrap_or("0")
                .parse::<i64>()
                .map_err(|_| reject("invalid gutter"))?;
            let top = integer(margins, "top")?;
            let bottom = integer(margins, "bottom")?;
            if left < 0
                || right < 0
                || gutter < 0
                || top < 0
                || bottom < 0
                || top + bottom > A4_H - 1440
            {
                return Err(reject("margins do not fit portrait A4"));
            }
            let available = A4_W - left - right - gutter;
            if available < 2880 {
                return Err(reject("less than two inches of body width"));
            }
            minimum_width = minimum_width.min(available);
            if section.descendants().any(|n| {
                n.has_tag_name((W, "cols"))
                    && (n.attribute((W, "num")).unwrap_or("1") != "1"
                        || n.children().any(|c| c.is_element()))
            }) {
                return Err(reject("multi-column section"));
            }
            if integer(size, "w")? != A4_W
                || integer(size, "h")? != A4_H
                || size.attribute((W, "orient")) == Some("landscape")
            {
                edits.push((size.range(), format!("<z:pgSz xmlns:z=\"{W}\" z:w=\"{A4_W}\" z:h=\"{A4_H}\" z:orient=\"portrait\"/>")));
            }
        }
        // Conservative common width also covers retained headers and footnotes.
        for entry in archive
            .entries
            .iter()
            .filter(|e| e.name.starts_with("word/") && e.name.ends_with(".xml"))
        {
            let text = std::str::from_utf8(&entry.data).map_err(|_| reject("part is not UTF-8"))?;
            let tree = roxmltree::Document::parse(text).map_err(|_| reject("invalid XML part"))?;
            for extent in tree
                .descendants()
                .filter(|n| n.has_tag_name((WP, "extent")))
            {
                let cx = extent
                    .attribute("cx")
                    .unwrap_or("")
                    .parse::<i64>()
                    .map_err(|_| reject("invalid drawing width"))?;
                if cx <= 0 || cx > minimum_width * 635 {
                    return Err(reject("inline drawing exceeds portrait text width"));
                }
            }
            if entry.name != "word/document.xml"
                && tree.descendants().any(|n| n.has_tag_name((W, "tbl")))
            {
                return Err(reject(
                    "tables outside the main story require separate fitting",
                ));
            }
        }
        for table in body.children().filter(|n| n.has_tag_name((W, "tbl"))) {
            widths_fit_table(table, minimum_width, &mut edits)?;
        }
        if body
            .descendants()
            .filter(|n| n.has_tag_name((W, "tbl")))
            .count()
            != body
                .children()
                .filter(|n| n.has_tag_name((W, "tbl")))
                .count()
        {
            return Err(reject("nested or wrapped table"));
        }
        if edits.is_empty() {
            return Ok(bytes);
        }
        let changed = preserve::patch_xml(xml, edits)?;
        roxmltree::Document::parse(&changed).map_err(|_| reject("invalid patched XML"))?;
        let mut replacements =
            BTreeMap::from([("/word/document.xml".into(), changed.into_bytes())]);
        let removals = preserve::without_stale_thumbnail(&bytes, &mut replacements)?;
        Ok(preserve::rewrite_parts_with_removals(
            &bytes,
            &replacements,
            &removals,
        )?)
    }
}
