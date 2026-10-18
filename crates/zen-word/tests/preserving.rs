use std::path::Path;
use zen_word::*;
fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
#[test]
fn imported_target_text_edit_preserves_sections_headers_and_unknown_markup() {
    let bytes = std::fs::read(
        root().join("RCD/red-team-corpus/office-targets-2026-10/docx/zen_word_multiscenario.docx"),
    )
    .unwrap();
    let mut doc = read_docx(&bytes).unwrap().document;
    assert_eq!(write_docx(&doc).unwrap(), bytes);
    assert!(doc.replace_text_preserving("Review", "Report").unwrap() > 0);
    let out = write_docx(&doc).unwrap();
    let a = zen_opc::ZipArchive::read(&bytes).unwrap();
    let b = zen_opc::ZipArchive::read(&out).unwrap();
    assert_eq!(a.names(), b.names());
    for e in &a.entries {
        if e.name != "word/document.xml" {
            assert_eq!(b.get(&e.name), Some(e.data.as_slice()), "{}", e.name);
        }
    }
    let text = read_docx(&out).unwrap().text();
    assert!(text.contains("Report"));
    assert!(text.contains("_x0001_"));
    assert!(!text.contains("_x005F_x0001_"));
    let baseline = doc.clone();
    assert!(doc.replace_text_preserving("Report", "bad\u{1}").is_err());
    assert_eq!(doc, baseline);
    doc.section.orientation = if doc.section.orientation == Orientation::Portrait {
        Orientation::Landscape
    } else {
        Orientation::Portrait
    };
    assert!(write_docx(&doc).is_err());
}
#[test]
fn new_document_literal_text_and_illegal_xml_are_handled() {
    let mut doc = Document::default();
    doc.blocks.push(Block::paragraph("literal _x0001_ & 中文"));
    let bytes = write_docx(&doc).unwrap();
    assert!(
        read_docx(&bytes)
            .unwrap()
            .text()
            .contains("literal _x0001_ & 中文")
    );
    doc.blocks.push(Block::paragraph("invalid \u{1}"));
    assert!(write_docx(&doc).is_err());
}

#[test]
fn unknown_relationship_rejects_edit_without_changing_model() {
    let mut doc = Document::default();
    doc.blocks.push(Block::paragraph("old text"));
    let bytes = write_docx(&doc).unwrap();
    let mut pkg =
        zen_opc::Package::from_archive(&zen_opc::ZipArchive::read(&bytes).unwrap()).unwrap();
    let mut rels = pkg.root_rels().unwrap();
    rels.add("rIdUnknown", "urn:unknown-dependency", "word/document.xml");
    pkg.set_root_rels(&rels).unwrap();
    let mut imported = read_docx(&pkg.to_bytes().unwrap()).unwrap().document;
    let before = imported.clone();
    assert!(imported.replace_text_preserving("old", "new").is_err());
    assert_eq!(imported, before);
}
#[test]
fn academic_formula_footnote_media_parts_survive_real_text_edit() {
    let path =
        root().join("RCD/red-team-corpus/academic-word-2026-10/zen_academic_multiscenario.docx");
    let bytes = std::fs::read(path).unwrap();
    let mut doc = read_docx(&bytes).unwrap().document;
    assert_eq!(
        doc.replace_text_preserving("本文讨论", "本文分析").unwrap(),
        1
    );
    let out = write_docx(&doc).unwrap();
    let a = zen_opc::ZipArchive::read(&bytes).unwrap();
    let b = zen_opc::ZipArchive::read(&out).unwrap();
    for e in &a.entries {
        if e.name != "word/document.xml" {
            assert_eq!(b.get(&e.name), Some(e.data.as_slice()), "{}", e.name);
        }
    }
    let xml = std::str::from_utf8(b.get("word/document.xml").unwrap()).unwrap();
    assert!(xml.contains("oMath"));
    assert!(xml.contains("footnoteReference"));
    assert!(xml.contains("sectPr"));
    assert!(xml.contains("本文分析"));
}
