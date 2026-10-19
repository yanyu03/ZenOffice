//! Admission for numeric date-serial intent, not a calendar or display engine.
use super::*;
use super::edit::EditError;
use zen_engine::workbook::{CellAddress, TypedValue};

type Result<T> = std::result::Result<T, EditError>;
fn refuse(message: &str) -> EditError { EditError::Unsupported(message.into()) }
fn text(bytes: &[u8]) -> Result<&str> {
    if bytes.len() > 2 * 1024 * 1024 { return Err(refuse("date metadata exceeds bounded XML bytes")); }
    std::str::from_utf8(bytes).map_err(|_| refuse("date metadata must be UTF-8"))
}
fn parse(value: &str) -> Result<roxmltree::Document<'_>> {
    Ok(roxmltree::Document::parse_with_options(value, roxmltree::ParsingOptions {
        nodes_limit: 16_384, ..Default::default()
    })?)
}
fn counted(node: roxmltree::Node<'_, '_>, length: usize) -> Result<()> {
    if node.attributes().any(|a| a.namespace().is_some() || a.name() != "count")
        || node.attribute("count").is_some_and(|n| n.parse::<usize>().ok() != Some(length))
        || node.children().any(|n| n.is_text() && !n.text().unwrap_or("").trim().is_empty()) {
        return Err(refuse("ambiguous date style count or attributes"));
    }
    Ok(())
}

impl ReadWorkbook {
    /// Require explicit, matching date system and an existing, unambiguous date
    /// style. This read-only check also precedes a session's numeric no-op path.
    pub fn validate_date_serial(
        &self, sheet_name: &str, address: CellAddress, serial: f64, system: DateSystem,
    ) -> Result<()> {
        if system != self.workbook.date_system() {
            return Err(refuse("date serial system differs from workbook; implicit conversion refused"));
        }
        // End-exclusive limits include the final day's fraction. Serial 60 in
        // the 1900 system is compatibility data, never converted to a real date.
        let end = match system { DateSystem::Excel1900 => 2_958_466.0, DateSystem::Excel1904 => 2_957_004.0 };
        if !serial.is_finite() || !(0.0..end).contains(&serial) {
            return Err(refuse("date serial is outside the supported finite nonnegative range"));
        }
        let sheet = self.workbook.sheets().iter().find(|s| s.name().eq_ignore_ascii_case(sheet_name))
            .ok_or_else(|| refuse("date target sheet missing"))?;
        let cell = sheet.cell(address).ok_or_else(|| refuse("date target must be an existing cell"))?;
        if cell.formula.is_some() || matches!(&cell.value, Some(TypedValue::Error(_))) {
            return Err(refuse("date intent cannot replace a formula or error"));
        }
        let style = cell.style_index.ok_or_else(|| refuse("date target requires an explicit style index"))? as usize;
        let sheet_part = self.package.part(&self.sheet_parts[&sheet.id()]).ok_or_else(|| refuse("date sheet part missing"))?;
        let sheet_xml = text(sheet_part.bytes())?;
        let sheet_doc = parse(sheet_xml)?;
        if sheet_doc.descendants().any(|n| n.has_tag_name((NS, "conditionalFormatting")) || n.has_tag_name((NS, "tableParts"))) {
            return Err(refuse("conditional/table date format precedence is not implemented"));
        }
        const TYPE: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml";
        const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles";
        let parts: Vec<_> = self.package.parts().filter(|p| p.content_type().contains("styles")).collect();
        let links: Vec<_> = self.package.relationships().iter().filter(|r| r.relationship_type.ends_with("/styles")).collect();
        if parts.len() != 1 || links.len() != 1 { return Err(refuse("date style requires a unique styles part and owner")); }
        let (part, link) = (parts[0], links[0]);
        if part.content_type() != TYPE || link.relationship_type != REL || link.source.as_ref() != Some(&self.workbook_part)
            || !matches!(&link.target, RelationshipTarget::Internal { part: target, fragment: None } if target == part.name())
            || self.package.incoming(part.name()).count() != 1 {
            return Err(refuse("unsupported date style ownership or content type"));
        }
        let xml = text(part.bytes())?;
        let doc = parse(xml)?;
        let root = doc.root_element();
        if !root.has_tag_name((NS, "styleSheet")) || root.attributes().len() != 0
            || root.descendants().any(|n| n.is_element() && (n.tag_name().namespace() != Some(NS) || n.tag_name().name() == "extLst"))
            || root.children().any(|n| (n.is_text() && !n.text().unwrap_or("").trim().is_empty())
                || (n.is_element() && !["numFmts", "fonts", "fills", "borders", "cellStyleXfs", "cellXfs", "cellStyles", "tableStyles", "colors", "dxfs"].contains(&n.tag_name().name()))) {
            return Err(refuse("unsupported date style root or extension semantics"));
        }
        let groups: Vec<_> = root.children().filter(|n| n.has_tag_name((NS, "cellXfs"))).collect();
        if groups.len() != 1 { return Err(refuse("date style requires one cellXfs table")); }
        let xfs: Vec<_> = groups[0].children().filter(|n| n.is_element()).collect();
        counted(groups[0], xfs.len())?;
        if xfs.iter().any(|n| !n.has_tag_name((NS, "xf"))) { return Err(refuse("unknown date style record")); }
        let xf = xfs.get(style).ok_or_else(|| refuse("date style index outside cellXfs"))?;
        if !matches!(xf.attribute("applyNumberFormat"), Some("1" | "true"))
            || xf.attribute("xfId").is_some_and(|value| value != "0")
            || xf.attributes().any(|a| a.namespace().is_some() || !["numFmtId", "fontId", "fillId", "borderId", "xfId", "applyNumberFormat"].contains(&a.name()))
            || xf.children().any(|n| n.is_element() || (n.is_text() && !n.text().unwrap_or("").trim().is_empty())) {
            return Err(refuse("date format must be explicitly applied without unsupported inheritance or metadata"));
        }
        let id = xf.attribute("numFmtId").and_then(|n| n.parse::<u32>().ok()).ok_or_else(|| refuse("date format id missing"))?;
        let definitions: Vec<_> = root.children().filter(|n| n.has_tag_name((NS, "numFmts"))).collect();
        if definitions.len() > 1 { return Err(refuse("duplicate numFmts table")); }
        let mut formats = BTreeMap::new();
        if let Some(definitions) = definitions.first() {
            let entries: Vec<_> = definitions.children().filter(|n| n.is_element()).collect();
            counted(*definitions, entries.len())?;
            for format in entries {
                if !format.has_tag_name((NS, "numFmt"))
                    || format.attributes().any(|a| a.namespace().is_some() || !["numFmtId", "formatCode"].contains(&a.name()))
                    || format.children().any(|n| n.is_element() || (n.is_text() && !n.text().unwrap_or("").trim().is_empty())) {
                    return Err(refuse("unsupported custom date format record"));
                }
                let number = format.attribute("numFmtId").and_then(|n| n.parse::<u32>().ok()).ok_or_else(|| refuse("custom format id missing"))?;
                let code = format.attribute("formatCode").ok_or_else(|| refuse("custom format code missing"))?;
                if number < 164 || formats.insert(number, code).is_some() {
                    return Err(refuse("builtin override or duplicate custom number format id"));
                }
            }
        }
        if !matches!(id, 14 | 15 | 16 | 17 | 22)
            && !matches!(formats.get(&id).copied(), Some("yyyy-mm-dd" | "yyyy-mm-dd hh:mm:ss")) {
            return Err(refuse("date style format is outside the explicit supported subset"));
        }
        Ok(())
    }
}
