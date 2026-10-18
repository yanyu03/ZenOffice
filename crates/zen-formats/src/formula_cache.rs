//! Preserve engine value types in formula caches that the writer auto-classifies.
//! This only rewrites our own generated worksheet parts; it is not an importer.
use crate::FormatError;
use std::io::{Cursor, Read, Write};

pub(crate) fn needs_string_type(value: &str) -> bool {
    value.parse::<f64>().is_ok()
        || matches!(
            value,
            "TRUE"
                | "FALSE"
                | "#DIV/0!"
                | "#N/A"
                | "#NAME?"
                | "#NULL!"
                | "#NUM!"
                | "#REF!"
                | "#VALUE!"
                | "#GETTING_DATA"
        )
}

fn escaped(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub(crate) fn preserve_string_types(
    bytes: Vec<u8>,
    caches: &[(usize, String, String)],
) -> Result<Vec<u8>, FormatError> {
    if caches.is_empty() {
        return Ok(bytes);
    }
    let mut source = zip::ZipArchive::new(Cursor::new(bytes))?;
    let mut destination = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for index in 0..source.len() {
        let mut file = source.by_index(index)?;
        let patches: Vec<_> = caches
            .iter()
            .filter(|(sheet, _, _)| file.name() == format!("xl/worksheets/sheet{sheet}.xml"))
            .collect();
        if patches.is_empty() {
            destination.raw_copy_file(file)?;
            continue;
        }
        let mut xml = String::new();
        file.read_to_string(&mut xml)?;
        let doc = roxmltree::Document::parse(&xml)?;
        let mut replacements = Vec::new();
        for (_, coord, text) in patches {
            let cell = doc
                .descendants()
                .find(|n| n.has_tag_name("c") && n.attribute("r") == Some(coord.as_str()))
                .ok_or_else(|| FormatError::Invalid("找不到需要写入文本缓存的公式单元格".into()))?;
            let value = cell
                .children()
                .find(|n| n.has_tag_name("v"))
                .ok_or_else(|| FormatError::Invalid("公式缓存节点缺失".into()))?;
            let cell_range = cell.range();
            let mut fragment = xml[cell_range.clone()].to_owned();
            let value_range = value.range();
            fragment.replace_range(
                value_range.start - cell_range.start..value_range.end - cell_range.start,
                &format!("<v>{}</v>", escaped(text)),
            );
            fragment = fragment.replacen(" t=\"b\"", " t=\"str\"", 1).replacen(
                " t=\"e\"",
                " t=\"str\"",
                1,
            );
            if cell.attribute("t").is_none() {
                let end = fragment
                    .find('>')
                    .ok_or_else(|| FormatError::Invalid("公式单元格 XML 无效".into()))?;
                fragment.insert_str(end, " t=\"str\"");
            }
            replacements.push((cell_range, fragment));
        }
        replacements.sort_unstable_by_key(|(range, _)| std::cmp::Reverse(range.start));
        drop(doc);
        for (range, replacement) in replacements {
            xml.replace_range(range, &replacement);
        }
        destination.start_file(
            file.name(),
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated),
        )?;
        destination.write_all(xml.as_bytes())?;
    }
    Ok(destination.finish()?.into_inner())
}
