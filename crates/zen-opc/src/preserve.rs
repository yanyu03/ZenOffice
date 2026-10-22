//! Surgical edits: retain every unmodified OPC part, including opaque markup.
//! This is deliberately narrower than reconstructing an imported document model.
use crate::{OpcError, Relationships, Result, ZipArchive, ZipWriter};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    ops::Range,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextLocation {
    pub part: String,
    pub range: Range<usize>,
    pub editable: bool,
}

/// XML 1.0 text/attributes. Unlike SpreadsheetML ST_Xstring, ordinary XML
/// text must not prefix literal `_xHHHH_` strings with `_x005F_`.
pub fn xml_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub fn validate_text(text: &str) -> Result<()> {
    if text
        .chars()
        .any(|c| c < '\u{20}' || c == '\u{FFFE}' || c == '\u{FFFF}')
    {
        return Err(OpcError::Xml(
            "text-only edit cannot contain controls, tabs or line breaks".into(),
        ));
    }
    Ok(())
}

pub fn text_element(xml: &str, range: Range<usize>, replacement: &str) -> Result<String> {
    validate_text(replacement)?;
    let raw = xml
        .get(range)
        .ok_or_else(|| OpcError::Xml("invalid source text range".into()))?;
    let end = raw
        .find('>')
        .ok_or_else(|| OpcError::Xml("text element start missing".into()))?;
    let opening = &raw[..end];
    let tag = opening
        .trim_start_matches('<')
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_end_matches('/');
    if tag.is_empty() {
        return Err(OpcError::Xml("text element name missing".into()));
    }
    // Preserve leading/trailing spaces without rebuilding the surrounding run.
    let mut opening = opening
        .trim_end_matches('/')
        .replace("xml:space=\"default\"", "xml:space=\"preserve\"")
        .replace("xml:space='default'", "xml:space='preserve'");
    if !opening.contains("xml:space=") {
        opening.push_str(" xml:space=\"preserve\"");
    }
    Ok(format!("{opening}>{}</{tag}>", xml_text(replacement)))
}

pub fn patch_xml(xml: &str, mut edits: Vec<(Range<usize>, String)>) -> Result<String> {
    edits.sort_by_key(|(range, _)| range.start);
    let mut end = 0;
    for (range, _) in &edits {
        if range.start < end
            || range.end > xml.len()
            || range.start > range.end
            || !xml.is_char_boundary(range.start)
            || !xml.is_char_boundary(range.end)
        {
            return Err(OpcError::Xml("overlapping or invalid XML edits".into()));
        }
        end = range.end;
    }
    let mut result = xml.to_string();
    for (range, text) in edits.into_iter().rev() {
        result.replace_range(range, &text);
    }
    roxmltree::Document::parse(&result).map_err(|e| OpcError::Xml(e.to_string()))?;
    Ok(result)
}

/// Text mutations reject unsupported dependencies. No mutation means original
/// bytes can be returned directly, without claiming unsupported editing ability.
pub fn validate_text_edit_package(
    bytes: &[u8],
    allowed_office_relationships: &[&str],
) -> Result<()> {
    validate_edit_package(bytes, allowed_office_relationships, None)
}

/// Explicit XLSX preservation policy only. The checked single custom XML edge
/// and payload can satisfy that specific rejection branch; all other gates run.
/// This does not infer ownership, waive formula/worksheet admission, or retry a
/// previously refused ordinary edit.
pub fn validate_xlsx_edit_package_with_policy(
    bytes: &[u8], allowed_office_relationships: &[&str],
    policy: &crate::opaque_xml::OpaqueXmlPolicy,
) -> Result<()> {
    policy.validate_candidate(bytes)?;
    validate_edit_package(bytes, allowed_office_relationships, Some(policy))
}

fn validate_edit_package(
    bytes: &[u8], allowed_office_relationships: &[&str],
    policy: Option<&crate::opaque_xml::OpaqueXmlPolicy>,
) -> Result<()> {
    let archive = ZipArchive::read(bytes)?;
    for entry in &archive.entries {
        let lower = entry.name.to_ascii_lowercase();
        if lower.contains("_xmlsignatures/")
            || lower.contains("vbaproject")
            || lower.contains("/activex/")
            || lower.contains("/embeddings/")
        {
            return Err(OpcError::Xml(format!(
                "editing signed/macro/active/embedded packages is unsupported: {}",
                entry.name
            )));
        }
        if lower.ends_with(".rels") {
            let text =
                std::str::from_utf8(&entry.data).map_err(|e| OpcError::Xml(e.to_string()))?;
            for rel in Relationships::parse(text)?.items {
                let office = rel.ty.strip_prefix(
                    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/",
                );
                let core = matches!(
                    rel.ty.as_str(),
                    "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties"
                        | "http://schemas.openxmlformats.org/package/2006/relationships/metadata/thumbnail"
                );
                if !core
                    && !(policy.is_some() && rel.ty == crate::opaque_xml::CUSTOM_XML_REL)
                    && !allowed_office_relationships.contains(&rel.ty.as_str())
                    && !office.is_some_and(|kind| allowed_office_relationships.contains(&kind))
                {
                    return Err(OpcError::Xml(format!(
                        "unsupported editing relationship: {} in {}",
                        rel.ty, entry.name
                    )));
                }
            }
        }
        if lower.ends_with("labelinfo.xml") {
            let text =
                std::str::from_utf8(&entry.data).map_err(|e| OpcError::Xml(e.to_string()))?;
            let doc = roxmltree::Document::parse(text).map_err(|e| OpcError::Xml(e.to_string()))?;
            if doc.descendants().any(|n| {
                n.is_element()
                    && n.tag_name().name() == "label"
                    && n.attribute("enabled") != Some("0")
            }) {
                return Err(OpcError::Xml(
                    "active classification labels require a policy-aware editor".into(),
                ));
            }
        }
        if lower.starts_with("customxml/") && lower.ends_with(".xml") {
            let text =
                std::str::from_utf8(&entry.data).map_err(|e| OpcError::Xml(e.to_string()))?;
            let doc = roxmltree::Document::parse(text).map_err(|e| OpcError::Xml(e.to_string()))?;
            let root = doc.root_element();
            let bibliography = "http://schemas.openxmlformats.org/officeDocument/2006/bibliography";
            let known_sources = root.has_tag_name((bibliography, "Sources"));
            let known_properties = root.has_tag_name((
                "http://schemas.openxmlformats.org/officeDocument/2006/customXml",
                "datastoreItem",
            )) && root
                .descendants()
                .filter(|n| n.is_element() && n.tag_name().name() == "schemaRef")
                .all(|n| {
                    n.attribute((
                        "http://schemas.openxmlformats.org/officeDocument/2006/customXml",
                        "uri",
                    )) == Some(bibliography)
                });
            if !known_sources && !known_properties
                && !policy.is_some_and(|p| p.declared_part() == format!("/{}", entry.name)) {
                return Err(OpcError::Xml("unknown custom XML dependency".into()));
            }
        }
        if lower.ends_with(".xml") {
            let text =
                std::str::from_utf8(&entry.data).map_err(|e| OpcError::Xml(e.to_string()))?;
            let doc = roxmltree::Document::parse(text).map_err(|e| OpcError::Xml(e.to_string()))?;
            if doc.descendants().any(|n| {
                n.is_element()
                    && matches!(
                        n.tag_name().name(),
                        "documentProtection" | "writeProtection" | "dataBinding"
                    )
            }) {
                return Err(OpcError::Xml(
                    "protected or data-bound document cannot be text-edited".into(),
                ));
            }
        }
    }
    Ok(())
}

/// Recompressing ZIP is permitted; all unmodified entry payloads stay byte-exact.
pub fn rewrite_parts(bytes: &[u8], replacements: &BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>> {
    rewrite_parts_with_removals(bytes, replacements, &BTreeSet::new())
}
pub fn rewrite_parts_with_removals(
    bytes: &[u8],
    replacements: &BTreeMap<String, Vec<u8>>,
    removals: &BTreeSet<String>,
) -> Result<Vec<u8>> {
    if replacements.is_empty() && removals.is_empty() {
        return Ok(bytes.to_vec());
    }
    let archive = ZipArchive::read(bytes)?;
    let mut seen = HashSet::new();
    let mut found = HashSet::new();
    let mut writer = ZipWriter::new();
    for entry in archive.entries {
        let name = crate::normalize_part_name(&entry.name)?;
        if !seen.insert(name.clone()) {
            return Err(OpcError::BadZip("duplicate normalized entry".into()));
        }
        if removals.contains(&name) {
            if replacements.contains_key(&name) {
                return Err(OpcError::BadPartName(
                    "cannot replace and remove the same part".into(),
                ));
            }
            continue;
        }
        if let Some(value) = replacements.get(&name) {
            writer.add(entry.name, value.clone());
            found.insert(name);
        } else {
            writer.add(entry.name, entry.data);
        }
    }
    if found.len() != replacements.len() {
        return Err(OpcError::BadPartName("replacement part missing".into()));
    }
    if !removals.iter().all(|p| seen.contains(p)) {
        return Err(OpcError::BadPartName("removal part missing".into()));
    }
    writer.finish()
}

/// A modified document must not retain a stale preview. Remove the optional
/// thumbnail, its root relationship and its content-type override together.
pub fn without_stale_thumbnail(
    bytes: &[u8],
    replacements: &mut BTreeMap<String, Vec<u8>>,
) -> Result<BTreeSet<String>> {
    let archive = ZipArchive::read(bytes)?;
    let mut removed = BTreeSet::new();
    let Some(root) = archive.get("_rels/.rels") else {
        return Ok(removed);
    };
    let xml = std::str::from_utf8(root).map_err(|e| OpcError::Xml(e.to_string()))?;
    let doc = roxmltree::Document::parse(xml).map_err(|e| OpcError::Xml(e.to_string()))?;
    let mut edits = Vec::new();
    for rel in doc.root_element().children().filter(|n| n.is_element() && n.attribute("Type")==Some("http://schemas.openxmlformats.org/package/2006/relationships/metadata/thumbnail")) {
        let target=crate::resolve_target("/",rel.attribute("Target").ok_or_else(||OpcError::Xml("thumbnail target missing".into()))?)?;
        if !target.starts_with("/docProps/") || ![".jpeg",".jpg",".png",".wmf",".emf"].iter().any(|s|target.to_ascii_lowercase().ends_with(s)) {
            return Err(OpcError::Xml("unsupported thumbnail part location/type".into()));
        }
        removed.insert(target); edits.push((rel.range(),String::new()));
    }
    if !edits.is_empty() {
        replacements.insert("/_rels/.rels".into(), patch_xml(xml, edits)?.into_bytes());
    }
    if !removed.is_empty() {
        let types = std::str::from_utf8(
            archive
                .get("[Content_Types].xml")
                .ok_or_else(|| OpcError::Xml("content types missing".into()))?,
        )
        .map_err(|e| OpcError::Xml(e.to_string()))?;
        let doc = roxmltree::Document::parse(types).map_err(|e| OpcError::Xml(e.to_string()))?;
        let edits: Vec<_> = doc
            .root_element()
            .children()
            .filter(|n| {
                n.is_element() && n.attribute("PartName").is_some_and(|p| removed.contains(p))
            })
            .map(|n| (n.range(), String::new()))
            .collect();
        if !edits.is_empty() {
            replacements.insert(
                "/[Content_Types].xml".into(),
                patch_xml(types, edits)?.into_bytes(),
            );
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn literal_escape_and_nonoverlapping_edits() {
        let xml = "<root><t>old</t><opaque attr='1'/></root>";
        let element = text_element(xml, 6..16, " _x0001_ & new ").unwrap();
        let result = patch_xml(xml, vec![(6..16, element)]).unwrap();
        assert!(result.contains(" _x0001_ &amp; new "));
        assert!(result.contains("<opaque attr='1'/>"));
        assert!(patch_xml(xml, vec![(6..16, "x".into()), (7..9, "y".into())]).is_err());
        assert!(validate_text("a\u{1}b").is_err());
    }
    #[test]
    fn untouched_parts_are_exact_and_missing_edits_fail() {
        let mut w = ZipWriter::new();
        w.add("a.xml", b"<a />".to_vec());
        w.add("opaque.bin", vec![0, 1, 255]);
        let bytes = w.finish().unwrap();
        assert_eq!(rewrite_parts(&bytes, &BTreeMap::new()).unwrap(), bytes);
        let edits = BTreeMap::from([("/a.xml".into(), b"<a>new</a>".to_vec())]);
        let out = ZipArchive::read(&rewrite_parts(&bytes, &edits).unwrap()).unwrap();
        assert_eq!(out.get("opaque.bin"), Some([0, 1, 255].as_slice()));
        assert!(rewrite_parts(&bytes, &BTreeMap::from([("/missing".into(), vec![])])).is_err());
    }
}
