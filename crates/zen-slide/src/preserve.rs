//! Original masters/layouts/tables/media remain in the imported package.
use crate::{Result, SlideError, model::*};
use std::{collections::BTreeMap, sync::Arc};
use zen_opc::preserve::{self, TextLocation};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreservedPresentation {
    pub(crate) bytes: Arc<Vec<u8>>,
    pub(crate) baseline: Box<Presentation>,
    pub(crate) locations: Vec<TextLocation>,
}
fn slots(pres: &mut Presentation) -> Vec<&mut String> {
    pres.slides
        .iter_mut()
        .flat_map(|s| &mut s.shapes)
        .filter_map(|s| s.text_body.as_mut())
        .flat_map(|t| &mut t.paragraphs)
        .flat_map(|p| &mut p.runs)
        .map(|r| &mut r.text)
        .collect()
}
pub(crate) fn save(presentation: &Presentation, source: &PreservedPresentation) -> Result<Vec<u8>> {
    let mut candidate = presentation.clone();
    candidate.preservation = None;
    if candidate == *source.baseline {
        return Ok(source.bytes.as_ref().clone());
    }
    let mut baseline = source.baseline.as_ref().clone();
    let (after, before) = (slots(&mut candidate), slots(&mut baseline));
    if after.len() != before.len() || before.len() != source.locations.len() {
        return Err(SlideError::Invalid(
            "preserving save does not support changed text topology".into(),
        ));
    }
    let archive = zen_opc::ZipArchive::read(&source.bytes)?;
    let mut edits: BTreeMap<String, Vec<_>> = BTreeMap::new();
    for ((new, old), location) in after.into_iter().zip(before).zip(&source.locations) {
        if *new != *old {
            if !location.editable {
                return Err(SlideError::Invalid(
                    "unsupported source text mapping".into(),
                ));
            }
            let xml = std::str::from_utf8(
                archive
                    .get(location.part.trim_start_matches('/'))
                    .ok_or_else(|| SlideError::Invalid("text source part missing".into()))?,
            )
            .map_err(|_| SlideError::Invalid("source XML is not UTF-8".into()))?;
            let replacement = preserve::text_element(xml, location.range.clone(), new)?;
            edits
                .entry(location.part.clone())
                .or_default()
                .push((location.range.clone(), replacement));
            *new = old.clone();
        }
    }
    if candidate != *source.baseline {
        return Err(SlideError::Invalid("preserving save supports existing run text only; structural/style/metadata edits rejected".into()));
    }
    preserve::validate_text_edit_package(
        &source.bytes,
        &[
            "officeDocument",
            "extended-properties",
            "custom-properties",
            "slide",
            "slideMaster",
            "slideLayout",
            "theme",
            "tableStyles",
            "presProps",
            "viewProps",
            "notesSlide",
            "notesMaster",
            "image",
            "hyperlink",
        ],
    )?;
    let mut replacements = BTreeMap::new();
    for (part, changes) in edits {
        let xml = std::str::from_utf8(archive.get(part.trim_start_matches('/')).unwrap())
            .map_err(|_| SlideError::Invalid("source XML is not UTF-8".into()))?;
        replacements.insert(part, preserve::patch_xml(xml, changes)?.into_bytes());
    }
    let removals = preserve::without_stale_thumbnail(&source.bytes, &mut replacements)?;
    Ok(preserve::rewrite_parts_with_removals(
        &source.bytes,
        &replacements,
        &removals,
    )?)
}
impl Presentation {
    pub fn replace_text_preserving(&mut self, find: &str, replacement: &str) -> Result<usize> {
        if find.is_empty() {
            return Err(SlideError::Invalid("empty search".into()));
        }
        preserve::validate_text(replacement)?;
        let mut candidate = self.clone();
        let mut count = 0;
        for text in slots(&mut candidate) {
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
