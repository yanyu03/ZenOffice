use zen_word::*;
use zen_word::write::{PART_DOCUMENT, PART_SETTINGS, REL_SETTINGS, W_NS};

fn package() -> zen_opc::Package {
    let mut doc = Document::default();
    doc.blocks.push(Block::paragraph("old text"));
    let bytes = write_docx(&doc).unwrap();
    zen_opc::Package::from_archive(&zen_opc::ZipArchive::read(&bytes).unwrap()).unwrap()
}
fn with_settings(inner: &str) -> Vec<u8> {
    let mut pkg = package();
    pkg.put_text(PART_SETTINGS, format!(r#"<w:settings xmlns:w="{W_NS}">{inner}</w:settings>"#)).unwrap();
    pkg.to_bytes().unwrap()
}
#[test]
fn new_defaults_and_explicit_boundaries_roundtrip() {
    assert_eq!(Document::default().settings.default_tab_stop_twips, 420);
    assert!(zen_word::write::settings_xml().contains(r#"w:val="420""#));
    for value in [0, 1, 420, 720, 32767] {
        let mut doc = Document::default();
        doc.settings.default_tab_stop_twips = value;
        let bytes = write_docx(&doc).unwrap();
        let imported = read_docx(&bytes).unwrap().document;
        assert_eq!(imported.settings, doc.settings);
        assert_eq!(write_docx(&imported).unwrap(), bytes);
    }
    for value in [32768, u32::MAX] {
        let mut doc = Document::default();
        doc.settings.default_tab_stop_twips = value;
        let before = doc.clone();
        assert!(matches!(write_docx(&doc), Err(WordError::UnsupportedEdit(_))));
        assert_eq!(doc, before);
    }
}
#[test]
fn omitted_setting_or_relationship_uses_import_default() {
    let bytes = with_settings("<w:zoom w:percent=\"100\"/>");
    let doc = read_docx(&bytes).unwrap().document;
    assert_eq!(doc.settings.default_tab_stop_twips, 720);
    assert_eq!(write_docx(&doc).unwrap(), bytes);
    let mut pkg = package();
    let mut rels = pkg.rels_for(PART_DOCUMENT).unwrap();
    rels.items.retain(|r| r.ty != REL_SETTINGS);
    pkg.set_rels_for(PART_DOCUMENT, &rels).unwrap();
    let bytes = pkg.to_bytes().unwrap();
    let doc = read_docx(&bytes).unwrap().document;
    // Orphaned 420-twip settings must not be read.
    assert_eq!(doc.settings.default_tab_stop_twips, 720);
    assert_eq!(write_docx(&doc).unwrap(), bytes);
}
#[test]
fn settings_follow_relocated_relationship_and_preserve_unknown_data() {
    let mut pkg = package();
    pkg.put_text("/word/config/tab-settings.xml", format!(r#"<w:settings xmlns:w="{W_NS}" xmlns:x="urn:unknown"><x:future/><w:defaultTabStop w:val="720"/></w:settings>"#)).unwrap();
    let mut rels = pkg.rels_for(PART_DOCUMENT).unwrap();
    rels.items.iter_mut().find(|r| r.ty == REL_SETTINGS).unwrap().target = "config/tab-settings.xml".into();
    pkg.set_rels_for(PART_DOCUMENT, &rels).unwrap();
    let bytes = pkg.to_bytes().unwrap();
    let mut doc = read_docx(&bytes).unwrap().document;
    assert_eq!(doc.settings.default_tab_stop_twips, 720);
    assert_eq!(write_docx(&doc).unwrap(), bytes);
    doc.replace_text_preserving("old", "new").unwrap();
    let out = write_docx(&doc).unwrap();
    let result = zen_opc::Package::from_archive(&zen_opc::ZipArchive::read(&out).unwrap()).unwrap();
    assert_eq!(result.get_text("/word/config/tab-settings.xml"), pkg.get_text("/word/config/tab-settings.xml"));
    let before = doc.clone();
    doc.settings.default_tab_stop_twips = 420;
    assert!(matches!(write_docx(&doc), Err(WordError::UnsupportedEdit(_))));
    assert_eq!(doc.blocks, before.blocks);
    assert_eq!(read_docx(&bytes).unwrap().document.settings.default_tab_stop_twips, 720);
}
#[test]
fn malformed_values_and_settings_roots_are_rejected() {
    for inner in [r#"<w:defaultTabStop/>"#, r#"<w:defaultTabStop val="720"/>"#,
        r#"<w:defaultTabStop w:val="-1"/>"#, r#"<w:defaultTabStop w:val="abc"/>"#,
        r#"<w:defaultTabStop w:val="1.5"/>"#,
        r#"<w:defaultTabStop w:val="420"/><w:defaultTabStop w:val="720"/>"#] {
        assert!(matches!(read_docx(&with_settings(inner)), Err(WordError::Malformed { .. })), "{inner}");
    }
    assert!(matches!(read_docx(&with_settings(r#"<w:defaultTabStop w:val="32768"/>"#)), Err(WordError::UnsupportedEdit(_))));
    for text in ["<broken", "<settings/>"] {
        let mut pkg = package();
        pkg.put_text(PART_SETTINGS, text).unwrap();
        assert!(read_docx(&pkg.to_bytes().unwrap()).is_err());
    }
}
#[test]
fn dangling_external_and_multiple_settings_relationships_are_rejected() {
    for kind in ["dangling", "external", "duplicate"] {
        let mut pkg = package();
        let mut rels = pkg.rels_for(PART_DOCUMENT).unwrap();
        let rel = rels.items.iter_mut().find(|r| r.ty == REL_SETTINGS).unwrap();
        match kind {
            "dangling" => rel.target = "missing.xml".into(),
            "external" => rel.mode = Some("External".into()),
            _ => { rels.add("rIdOtherSettings", REL_SETTINGS, "settings.xml"); },
        }
        pkg.set_rels_for(PART_DOCUMENT, &rels).unwrap();
        let bytes = pkg.to_bytes().unwrap();
        assert!(read_docx(&bytes).is_err(), "{kind}");
        assert_eq!(pkg.to_bytes().unwrap(), bytes);
    }
}
#[test]
#[ignore = "requires cycle23 frozen independent Word-oracle inputs"]
fn frozen_tab_inputs_read_distinct_intervals_without_rewriting() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = root.join("target/delivery_cycles/cycle_20261016_23/controlled");
    // These independent Word-oracle inputs are frozen by cycle23's manifest.
    let mut checked = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("docx") { continue; }
        let bytes = std::fs::read(&path).unwrap();
        let doc = read_docx(&bytes).unwrap().document;
        let expected = if path.file_stem().unwrap().to_string_lossy().contains("420") {420} else {720};
        assert_eq!(doc.settings.default_tab_stop_twips, expected, "{}", path.display());
        assert_eq!(write_docx(&doc).unwrap(), bytes);
        checked += 1;
    }
    assert_eq!(checked, 12);
}
