//! Restricted, package-preserving input edits. The projected workbook
//! itself remains read-only; saving is an explicit checked operation.
use super::*;
use std::collections::BTreeMap;
use zen_engine::{
    recalc::{CalculationValue, WorkbookCalculator},
    workbook::{CellAddress, TypedValue},
};
use zen_opc::preserve::{patch_xml, rewrite_parts, validate_text_edit_package};

#[derive(Debug, thiserror::Error)]
pub enum EditError {
    #[error("Unsupported preserving workbook edit: {0}")]
    Unsupported(String),
    #[error(transparent)]
    Opc(#[from] zen_opc::OpcError),
    #[error(transparent)]
    Calc(#[from] zen_engine::recalc::RecalcError),
    #[error(transparent)]
    Xml(#[from] roxmltree::Error),
}
type EditResult<T> = std::result::Result<T, EditError>;
fn unsupported(message: &str) -> EditError {
    EditError::Unsupported(message.into())
}

fn utf8(bytes: &[u8]) -> EditResult<&str> {
    std::str::from_utf8(bytes).map_err(|_| unsupported("editable XML must be UTF-8"))
}
fn source_bytes(source: &ReadWorkbook) -> EditResult<Vec<u8>> {
    let mut zip = zen_opc::ZipWriter::new();
    zip.add(
        "[Content_Types].xml",
        source.package.content_types_bytes().to_vec(),
    );
    for part in source.package.parts() {
        zip.add(
            part.name().as_str().trim_start_matches('/'),
            part.bytes().to_vec(),
        );
    }
    for item in source.package.unmapped_items() {
        zip.add(&item.name, item.bytes.clone());
    }
    Ok(zip.finish()?)
}
fn qualified_tag(xml: &str, node: roxmltree::Node<'_, '_>) -> String {
    xml[node.range()]
        .trim_start_matches('<')
        .split([' ', '\t', '\n', '\r', '/', '>'])
        .next()
        .unwrap()
        .to_string()
}
fn value_edit(
    xml: &str,
    cell: roxmltree::Node<'_, '_>,
    value: f64,
) -> EditResult<(std::ops::Range<usize>, String)> {
    if !value.is_finite() {
        return Err(unsupported("non-finite numeric result"));
    }
    if cell.attribute("t").is_some_and(|t| t != "n") {
        return Err(unsupported(
            "numeric edit cannot change an existing cell's type",
        ));
    }
    let v = cell
        .children()
        .find(|n| n.has_tag_name((NS, "v")))
        .ok_or_else(|| unsupported("numeric cache/value element missing"))?;
    if v.children().any(|n| n.is_element()) {
        return Err(unsupported("complex numeric value"));
    }
    let tag = qualified_tag(xml, v);
    Ok((v.range(), format!("<{tag}>{value}</{tag}>")))
}
fn same_value(a: Option<&TypedValue>, b: Option<&TypedValue>) -> bool {
    match (a, b) {
        (Some(TypedValue::Number { value: a, .. }), Some(TypedValue::Number { value: b, .. })) => a == b,
        _ => a == b,
    }
}

fn string_xml(text: &str) -> EditResult<String> {
    let mut out = String::new();
    for (i, c) in text.char_indices() {
        if !matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}') {
            return Err(unsupported("text contains an unsupported XML character"));
        }
        // Shield a literal ST_Xstring escape from the reader's one-pass decoder.
        let tail = &text.as_bytes()[i..];
        if c == '_' && tail.get(1) == Some(&b'x') && tail.get(6) == Some(&b'_')
            && tail.get(2..6).is_some_and(|h| h.iter().all(u8::is_ascii_hexdigit)) {
            out.push_str("_x005F_");
            continue;
        }
        match c {
            '&' => out.push_str("&amp;"), '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"), '\r' => out.push_str("&#13;"),
            _ => out.push(c),
        }
    }
    Ok(out)
}

fn typed_edit(
    xml: &str, cell: roxmltree::Node<'_, '_>, value: Option<&TypedValue>, formula_cache: bool,
) -> EditResult<(std::ops::Range<usize>, String)> {
    if cell.children().any(|n| n.is_element() && !["f", "v", "is"].iter().any(|t| n.has_tag_name((NS, *t)))) {
        return Err(unsupported("unprojected cell children"));
    }
    let tag = qualified_tag(xml, cell);
    let prefix = tag.strip_suffix('c').unwrap();
    // Keep the source opening tag (including local namespace declarations).
    let start = cell.range().start;
    let mut quote = None;
    let open_end = xml[start..].char_indices().find_map(|(i, c)| {
        match (quote, c) {
            (None, '\'' | '"') => quote = Some(c),
            (Some(q), c) if q == c => quote = None,
            (None, '>') => return Some(start + i),
            _ => (),
        }
        None
    }).ok_or_else(|| unsupported("cell opening tag"))?;
    let mut opening = xml[start..open_end].to_string();
    if let Some(t) = cell.attribute_node("t") {
        // roxmltree 0.19 exposes attribute start positions, not full ranges.
        // Parsed XML guarantees a quoted value; find its matching source quote.
        let bytes = xml.as_bytes();
        let first = (t.position()..open_end).find(|i| matches!(bytes[*i], b'\'' | b'"'))
            .ok_or_else(|| unsupported("cell type attribute quote"))?;
        let last = (first + 1..open_end).find(|i| bytes[*i] == bytes[first])
            .ok_or_else(|| unsupported("cell type attribute end"))?;
        opening.replace_range(t.position() - start..last + 1 - start, "");
    }
    opening = opening.trim_end().trim_end_matches('/').trim_end().to_string();
    let (kind, body) = match value {
        None => ("", String::new()),
        Some(TypedValue::Number { value, .. }) if value.is_finite() => ("n", format!("<{prefix}v>{value}</{prefix}v>")),
        Some(TypedValue::Boolean(b)) => ("b", format!("<{prefix}v>{}</{prefix}v>", u8::from(*b))),
        Some(TypedValue::Text(t)) => {
            let t = string_xml(t)?;
            if formula_cache { ("str", format!("<{prefix}v>{t}</{prefix}v>")) }
            else { ("inlineStr", format!("<{prefix}is><{prefix}t xml:space=\"preserve\">{t}</{prefix}t></{prefix}is>")) }
        }
        _ => return Err(unsupported("unsupported typed value")),
    };
    if !kind.is_empty() { opening.push_str(&format!(" t=\"{kind}\"")); }
    opening.push('>');
    if formula_cache {
        let f = cell.children().find(|n| n.has_tag_name((NS, "f")))
            .ok_or_else(|| unsupported("formula cache without formula"))?;
        opening.push_str(&xml[f.range()]);
    }
    opening.push_str(&body);
    opening.push_str(&format!("</{tag}>"));
    Ok((cell.range(), opening))
}

fn address(text: &str) -> EditResult<CellAddress> {
    let unprefixed = text.strip_prefix('$').unwrap_or(text);
    let letters = unprefixed
        .bytes()
        .take_while(|c| c.is_ascii_alphabetic())
        .count();
    let rest = &unprefixed[letters..];
    let digits = rest.strip_prefix('$').unwrap_or(rest);
    if letters == 0
        || letters > 3
        || digits.is_empty()
        || digits.len() > 7
        || !digits.bytes().all(|c| c.is_ascii_digit())
    {
        return Err(unsupported("chart range must be a single A1 address"));
    }
    let coord = zen_engine::calc::CellCoord::from_a1(&text.replace('$', ""))
        .ok_or_else(|| unsupported("chart range address"))?;
    if coord.row >= 1_048_576 || coord.col >= 16_384 {
        return Err(unsupported("chart range bounds"));
    }
    CellAddress::new(coord.row as u32, coord.col as u16)
        .map_err(|_| unsupported("chart range bounds"))
}
fn chart_values(calc: &WorkbookCalculator, formula: &str) -> EditResult<Vec<f64>> {
    let (sheet, range) = formula
        .rsplit_once('!')
        .ok_or_else(|| unsupported("chart reference must name a sheet"))?;
    if sheet.contains(['[', ']']) {
        return Err(unsupported("external chart reference"));
    }
    let sheet = if sheet.starts_with('\'') && sheet.ends_with('\'') {
        sheet[1..sheet.len() - 1].replace("''", "'")
    } else {
        sheet.to_string()
    };
    let (a, b) = range.split_once(':').unwrap_or((range, range));
    let (a, b) = (address(a)?, address(b)?);
    if b.row() < a.row()
        || b.col() < a.col()
        || (b.row() - a.row() + 1) as u64 * (b.col() - a.col() + 1) as u64 > 100_000
    {
        return Err(unsupported("chart range quota or order"));
    }
    let mut values = Vec::new();
    for row in a.row()..=b.row() {
        for col in a.col()..=b.col() {
            let v = calc
                .value(&sheet, CellAddress::new(row, col).unwrap())
                .and_then(CalculationValue::as_number)
                .ok_or_else(|| {
                    unsupported("chart numeric reference contains empty/text/error result")
                })?;
            values.push(v);
        }
    }
    Ok(values)
}

impl ReadWorkbook {
    /// Change one *existing numeric input*, recalculate every normal formula,
    /// refresh supported native chart caches, and preserve all unrelated parts.
    /// No formula, sheet, style, string-table or relationship edits are exposed.
    pub fn write_number_edit(
        &self,
        sheet_name: &str,
        address: CellAddress,
        value: f64,
    ) -> EditResult<Vec<u8>> {
        if !value.is_finite() {
            return Err(unsupported("input number must be finite"));
        }
        let sheet = self
            .workbook
            .sheets()
            .iter()
            .find(|s| s.name().eq_ignore_ascii_case(sheet_name))
            .ok_or_else(|| unsupported("sheet not found"))?;
        let input = sheet
            .cell(address)
            .ok_or_else(|| unsupported("only existing numeric inputs can be edited"))?;
        if input.formula.is_some() || !matches!(input.value, Some(TypedValue::Number { .. })) {
            return Err(unsupported("target is not a numeric input"));
        }
        self.write_input_edit(sheet_name, address, Some(TypedValue::Number {
            value, lexical: value.to_string(),
        }), false)
    }

    /// Replace an existing non-formula, non-error input with blank, text, bool
    /// or a finite number. Formula removal, dates and direct errors are deferred.
    /// The caller must re-read the candidate under its original resource limits.
    pub fn write_value_edit(
        &self, sheet_name: &str, address: CellAddress, value: Option<TypedValue>,
    ) -> EditResult<Vec<u8>> {
        self.write_input_edit(sheet_name, address, value, true)
    }

    fn admitted_source(&self) -> EditResult<Vec<u8>> {
        if self.package.diagnostics().iter().any(|d| {
            matches!(
                d.code,
                crate::opc::DiagnosticCode::DanglingTarget
                    | crate::opc::DiagnosticCode::UnmappedZipItem
            )
        }) {
            return Err(unsupported("package has dangling or unmapped dependencies"));
        }
        let bytes = source_bytes(self)?;
        validate_text_edit_package(
            &bytes,
            &[
                "officeDocument",
                "extended-properties",
                "custom-properties",
                "worksheet",
                "styles",
                "theme",
                "sharedStrings",
                "calcChain",
                "drawing",
                "chart",
                "image",
                "table",
                "hyperlink",
            ],
        )?;
        // Opaque dependencies are preserved only where the operation's effects
        // are understood. Protection, external data, extension semantics and
        // calculated table columns are not safe under this numeric-edit subset.
        for part in self
            .package
            .parts()
            .filter(|p| p.name().as_str().ends_with(".xml"))
        {
            let doc = roxmltree::Document::parse(utf8(part.bytes())?)?;
            if part.content_type().contains("spreadsheetml.worksheet") {
                let supported = [
                    "sheetPr",
                    "dimension",
                    "sheetViews",
                    "sheetFormatPr",
                    "cols",
                    "sheetData",
                    "sheetCalcPr",
                    "autoFilter",
                    "mergeCells",
                    "phoneticPr",
                    "conditionalFormatting",
                    "dataValidations",
                    "hyperlinks",
                    "printOptions",
                    "pageMargins",
                    "pageSetup",
                    "headerFooter",
                    "rowBreaks",
                    "colBreaks",
                    "drawing",
                    "tableParts",
                ];
                if doc
                    .root_element()
                    .children()
                    .filter(|n| n.is_element())
                    .any(|n| {
                        n.tag_name().namespace() != Some(NS)
                            || !supported.contains(&n.tag_name().name())
                    })
                {
                    return Err(unsupported("unrecognized worksheet dependency"));
                }
                if doc
                    .descendants()
                    .filter(|n| n.has_tag_name((NS, "c")))
                    .any(|n| {
                        n.attributes().any(|a| {
                            a.namespace().is_some() || !["r", "s", "t"].contains(&a.name())
                        })
                    })
                {
                    return Err(unsupported("unrecognized cell metadata dependency"));
                }
            }
            if doc.descendants().any(|n| {
                n.is_element()
                    && matches!(
                        n.tag_name().name(),
                        "sheetProtection"
                            | "workbookProtection"
                            | "pivotTableDefinition"
                            | "pivotCacheDefinition"
                            | "externalData"
                            | "calculatedColumnFormula"
                            | "totalsRowFormula"
                    )
            }) {
                return Err(unsupported(&format!(
                    "unsupported protected/extended dependency in {}",
                    part.name().as_str()
                )));
            }
            for alternate in doc
                .descendants()
                .filter(|n| n.has_tag_name((xml::MC_NS, "AlternateContent")))
            {
                let appearance_only = part.content_type().contains("drawingml.chart")
                    && alternate.descendants().filter(|n| n.is_element()).all(|n| {
                        matches!(
                            (n.tag_name().namespace(), n.tag_name().name()),
                            (Some(xml::MC_NS), "AlternateContent" | "Choice" | "Fallback")
                                | (
                                    Some(
                                        "http://schemas.microsoft.com/office/drawing/2007/8/2/chart"
                                    ),
                                    "style"
                                )
                                | (
                                    Some("http://schemas.openxmlformats.org/drawingml/2006/chart"),
                                    "style"
                                )
                        )
                    });
                let saved_path_only=part.name()==&self.workbook_part && alternate.descendants().filter(|n|n.is_element()).all(|n| {
                    matches!((n.tag_name().namespace(),n.tag_name().name()),
                        (Some(xml::MC_NS),"AlternateContent" | "Choice" | "Fallback") |
                        (Some("http://schemas.microsoft.com/office/spreadsheetml/2010/11/ac"),"absPath"))
                });
                if !appearance_only && !saved_path_only {
                    return Err(unsupported("unknown alternate-content dependency"));
                }
            }
            if part.name() == &self.workbook_part {
                let supported = [
                    "fileVersion",
                    "fileSharing",
                    "workbookPr",
                    "workbookProtection",
                    "bookViews",
                    "sheets",
                    "definedNames",
                    "calcPr",
                    "extLst",
                ];
                if doc
                    .root_element()
                    .children()
                    .filter(|n| n.is_element())
                    .any(|n| {
                        !(n.tag_name().namespace() == Some(NS)
                            && supported.contains(&n.tag_name().name())
                            || n.has_tag_name((xml::MC_NS, "AlternateContent"))
                            || n.has_tag_name((
                                "http://schemas.microsoft.com/office/spreadsheetml/2014/revision",
                                "revisionPtr",
                            )))
                    })
                {
                    return Err(unsupported("unrecognized workbook dependency"));
                }
                for ext in doc.descendants().filter(|n| n.has_tag_name((NS, "ext"))) {
                    if !matches!(
                        ext.attribute("uri"),
                        Some(
                            "{140A7094-0E35-4892-8432-C4D2E57EDEB5}"
                                | "{B58B0392-4F1F-4190-BB64-5DF3571DCE5F}"
                                | "{D14903EA-33C4-47F7-8F05-3474C54BE107}"
                        )
                    ) {
                        return Err(unsupported("unknown workbook extension dependency"));
                    }
                    if ext.descendants().filter(|n|n.is_element() && *n!=ext).any(|n| !matches!((n.tag_name().namespace(),n.tag_name().name()),
                        (Some("http://schemas.microsoft.com/office/spreadsheetml/2010/11/main"),"workbookPr") |
                        (Some("http://schemas.microsoft.com/office/spreadsheetml/2018/calcfeatures"),"calcFeatures" | "feature") |
                        (Some("http://schemas.microsoft.com/office/spreadsheetml/2024/workbookCompatibilityVersion"),"version"))) {
                        return Err(unsupported("unrecognized workbook extension payload"));
                    }
                }
            }
            if part.content_type().contains("spreadsheetml.worksheet")
                && doc.descendants().any(|n| n.has_tag_name((NS, "extLst")))
            {
                return Err(unsupported(
                    "worksheet extensions cannot be safely maintained",
                ));
            }
        }
        Ok(bytes)
    }

    fn write_input_edit(
        &self, sheet_name: &str, address: CellAddress, value: Option<TypedValue>, typed: bool,
    ) -> EditResult<Vec<u8>> {
        if matches!(&value, Some(TypedValue::Number { value, .. }) if !value.is_finite()) {
            return Err(unsupported("input number must be finite"));
        }
        if matches!(&value, Some(TypedValue::Error(_))) {
            return Err(unsupported("direct error input is not supported"));
        }
        let sheet = self.workbook.sheets().iter()
            .find(|s| s.name().eq_ignore_ascii_case(sheet_name))
            .ok_or_else(|| unsupported("sheet not found"))?;
        let input = sheet.cell(address).ok_or_else(|| unsupported("existing explicit input required"))?;
        if input.formula.is_some() || matches!(&input.value, Some(TypedValue::Error(_))) {
            return Err(unsupported("formula/error input replacement is not supported"));
        }
        let bytes = self.admitted_source()?;
        let mut calc = WorkbookCalculator::new(&self.workbook)?;
        calc.set_input(sheet.name(), address, value.clone())?;
        calc.recalculate()?;
        let mut replacements = BTreeMap::new();
        for ws in self.workbook.sheets() {
            let part = &self.sheet_parts[&ws.id()];
            let original = self
                .package
                .part(part)
                .ok_or_else(|| unsupported("worksheet source missing"))?;
            let xml = utf8(original.bytes())?;
            let doc = roxmltree::Document::parse(xml)?;
            let mut edits = Vec::new();
            if typed && ws.id() == sheet.id() {
                for merge in doc.descendants().filter(|n| n.has_tag_name((NS, "mergeCell"))) {
                    let range = merge.attribute("ref").ok_or_else(|| unsupported("merge range missing"))?;
                    let (a, b) = range.split_once(':').unwrap_or((range, range));
                    let (a, b) = (self::address(a)?, self::address(b)?);
                    if a.row() > b.row() || a.col() > b.col() {
                        return Err(unsupported("merge range order"));
                    }
                    if (a.row()..=b.row()).contains(&address.row()) && (a.col()..=b.col()).contains(&address.col()) {
                        return Err(unsupported("merged input edits are not supported"));
                    }
                }
            }
            for cell in doc.descendants().filter(|n| n.has_tag_name((NS, "c"))) {
                let Some(a1) = cell.attribute("r") else {
                    return Err(unsupported(
                        "explicit cell addresses required for preserving edits",
                    ));
                };
                let coord = self::address(a1)?;
                let Some(model) = ws.cell(coord) else {
                    return Err(unsupported("unprojected worksheet cell"));
                };
                if ws.id() == sheet.id() && coord == address {
                    edits.push(if typed {
                        typed_edit(xml, cell, value.as_ref(), false)?
                    } else {
                        let Some(TypedValue::Number { value, .. }) = &value else { unreachable!() };
                        value_edit(xml, cell, *value)?
                    });
                } else if model.formula.is_some() {
                    if typed {
                        let calculated = calc.value(ws.name(), coord)
                            .ok_or_else(|| unsupported("formula result missing"))?;
                        let next = match calculated {
                            CalculationValue::Number(n) => Some(TypedValue::Number { value: *n, lexical: n.to_string() }),
                            CalculationValue::Text(t) => Some(TypedValue::Text(t.clone())),
                            CalculationValue::Boolean(b) => Some(TypedValue::Boolean(*b)),
                            CalculationValue::Error(e) if matches!(&model.value, Some(TypedValue::Error(old)) if old == e) => continue,
                            _ => return Err(unsupported("changed error or empty formula cache is not supported")),
                        };
                        if matches!(&model.value, Some(TypedValue::Error(_))) {
                            return Err(unsupported("error cache type transition is not supported"));
                        }
                        if !same_value(model.value.as_ref(), next.as_ref()) {
                            edits.push(typed_edit(xml, cell, next.as_ref(), true)?);
                        }
                    } else {
                    match calc.value(ws.name(), coord) {
                        Some(CalculationValue::Number(n)) => {
                            if !matches!(&model.value,Some(TypedValue::Number{value:old,..}) if old==n)
                            {
                                edits.push(value_edit(xml, cell, *n)?);
                            }
                        }
                        Some(CalculationValue::Text(t)) if matches!(&model.value,Some(TypedValue::Text(old)) if old==t) => {
                            ()
                        }
                        Some(CalculationValue::Boolean(b)) if matches!(&model.value,Some(TypedValue::Boolean(old)) if old==b) => {
                            ()
                        }
                        Some(CalculationValue::Error(e)) if matches!(&model.value,Some(TypedValue::Error(old)) if old==e) => {
                            // Recalculated and unchanged: preserve the original error
                            // cell bytes, just as for unchanged text/boolean caches.
                            // A changed error or type transition still fails closed.
                            ()
                        }
                        _ => {
                            return Err(unsupported(
                                "changed nonnumeric/error formula caches are not supported in this save subset",
                            ));
                        }
                    }
                    }
                }
            }
            if !edits.is_empty() {
                replacements.insert(part.as_str().into(), patch_xml(xml, edits)?.into_bytes());
            }
        }
        const C: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
        for part in self
            .package
            .parts()
            .filter(|p| p.content_type().ends_with("drawingml.chart+xml"))
        {
            let xml = utf8(part.bytes())?;
            let doc = roxmltree::Document::parse(xml)?;
            if typed && doc.descendants().any(|n| {
                n.has_tag_name((C, "strRef")) || n.has_tag_name((C, "multiLvlStrRef"))
            }) {
                return Err(unsupported("chart string-reference caches are not maintained by typed edits"));
            }
            let mut edits = Vec::new();
            for reference in doc.descendants().filter(|n| n.has_tag_name((C, "numRef"))) {
                let formula = reference
                    .children()
                    .find(|n| n.has_tag_name((C, "f")))
                    .and_then(|n| n.text())
                    .ok_or_else(|| unsupported("chart reference formula missing"))?;
                let cache = reference
                    .children()
                    .find(|n| n.has_tag_name((C, "numCache")))
                    .ok_or_else(|| unsupported("chart cache missing"))?;
                let values = chart_values(&calc, formula)?;
                let tag = qualified_tag(xml, cache);
                let prefix = tag.strip_suffix("numCache").unwrap();
                let mut text = format!("<{tag}>");
                if let Some(format) = cache.children().find(|n| n.has_tag_name((C, "formatCode"))) {
                    text.push_str(&xml[format.range()]);
                }
                text.push_str(&format!("<{prefix}ptCount val=\"{}\"/>", values.len()));
                for (i, v) in values.iter().enumerate() {
                    text.push_str(&format!(
                        "<{prefix}pt idx=\"{i}\"><{prefix}v>{v}</{prefix}v></{prefix}pt>"
                    ));
                }
                text.push_str(&format!("</{tag}>"));
                edits.push((cache.range(), text));
            }
            if !edits.is_empty() {
                replacements.insert(
                    part.name().as_str().into(),
                    patch_xml(xml, edits)?.into_bytes(),
                );
            }
        }
        // Formula definitions and their dependency structure have not changed:
        // calcChain remains valid. Every formula cache was recalculated above.
        Ok(rewrite_parts(&bytes, &replacements)?)
    }
}
