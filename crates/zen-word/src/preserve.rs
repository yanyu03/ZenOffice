//! Imported documents keep their original package. Only mapped text changes
//! are patched; unsupported model changes must never fall back to a lossy save.
use crate::{Result, WordError, model::*};
use std::{collections::BTreeMap, sync::Arc};
use zen_opc::preserve::{self, TextLocation};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreservedDocument {
    pub(crate) bytes: Arc<Vec<u8>>,
    pub(crate) baseline: Box<Document>,
    pub(crate) locations: Vec<Option<TextLocation>>,
}

fn text_slots<'a>(blocks: &'a mut [Block], slots: &mut Vec<&'a mut String>) {
    for block in blocks {
        match block {
            Block::Paragraph(p) => {
                for run in &mut p.runs {
                    for content in &mut run.contents {
                        if let RunContent::Text(t) = content {
                            slots.push(t);
                        }
                    }
                }
            }
            Block::Table(t) => {
                for row in &mut t.rows {
                    for cell in &mut row.cells {
                        text_slots(&mut cell.blocks, slots);
                    }
                }
            }
        }
    }
}

pub(crate) fn save(document: &Document, source: &PreservedDocument) -> Result<Vec<u8>> {
    let mut candidate = document.clone();
    candidate.preservation = None;
    if &candidate == source.baseline.as_ref() {
        return Ok(source.bytes.as_ref().clone());
    }
    let mut baseline = source.baseline.as_ref().clone();
    let (mut after, mut before) = (Vec::new(), Vec::new());
    text_slots(&mut candidate.blocks, &mut after);
    text_slots(&mut baseline.blocks, &mut before);
    if after.len() != before.len() || before.len() != source.locations.len() {
        return Err(WordError::UnsupportedEdit(
            "imported document text topology changed".into(),
        ));
    }
    let archive = zen_opc::ZipArchive::read(&source.bytes)?;
    let mut patches: BTreeMap<String, Vec<_>> = BTreeMap::new();
    for ((new, old), location) in after.iter_mut().zip(before.iter()).zip(&source.locations) {
        if **new != **old {
            let location = location.as_ref().filter(|l| l.editable).ok_or_else(|| {
                WordError::UnsupportedEdit(
                    "field, locked, tracked or non-text content cannot be patched".into(),
                )
            })?;
            let xml = std::str::from_utf8(
                archive
                    .get(location.part.trim_start_matches('/'))
                    .ok_or_else(|| WordError::MissingPart(location.part.clone()))?,
            )
            .map_err(|_| WordError::UnsupportedEdit("source XML is not UTF-8".into()))?;
            let replacement = preserve::text_element(xml, location.range.clone(), new)?;
            patches
                .entry(location.part.clone())
                .or_default()
                .push((location.range.clone(), replacement));
            **new = (*old).clone();
        }
    }
    drop(after);
    drop(before);
    if candidate != *source.baseline {
        return Err(WordError::UnsupportedEdit("imported document structural, style or metadata edits require a supported preserving operation".into()));
    }
    validate_edit_package(&source.bytes)?;
    let mut replacements = BTreeMap::new();
    for (part, edits) in patches {
        let xml = std::str::from_utf8(archive.get(part.trim_start_matches('/')).unwrap())
            .map_err(|_| WordError::UnsupportedEdit("source XML is not UTF-8".into()))?;
        replacements.insert(part, preserve::patch_xml(xml, edits)?.into_bytes());
    }
    let removals = preserve::without_stale_thumbnail(&source.bytes, &mut replacements)?;
    Ok(preserve::rewrite_parts_with_removals(
        &source.bytes,
        &replacements,
        &removals,
    )?)
}

pub(crate) fn validate_edit_package(bytes: &[u8]) -> Result<()> {
    preserve::validate_text_edit_package(
        bytes,
        &[
            "officeDocument",
            "extended-properties",
            "custom-properties",
            "styles",
            "stylesWithEffects",
            "settings",
            "webSettings",
            "fontTable",
            "theme",
            "numbering",
            "header",
            "footer",
            "image",
            "hyperlink",
            "footnotes",
            "endnotes",
            "comments",
            "attachedTemplate",
            "customXml",
            "customXmlProps",
            "http://schemas.microsoft.com/office/2020/02/relationships/classificationlabels",
        ],
    )?;
    Ok(())
}

impl Document {
    /// Replace within existing text runs, retaining run formatting. Cross-run
    /// replacements and paragraph insertion need a separate structural API.
    pub fn replace_text_preserving(&mut self, find: &str, replacement: &str) -> Result<usize> {
        if find.is_empty() {
            return Err(WordError::UnsupportedEdit("empty search".into()));
        }
        preserve::validate_text(replacement)?;
        let mut candidate = self.clone();
        let mut slots = Vec::new();
        text_slots(&mut candidate.blocks, &mut slots);
        let mut count = 0;
        for text in slots {
            count += text.matches(find).count();
            *text = text.replace(find, replacement);
        }
        if let Some(source) = &candidate.preservation {
            save(&candidate, source)?;
        }
        *self = candidate;
        Ok(count)
    }
}
