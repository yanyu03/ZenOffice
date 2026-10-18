use zen_slide::*;
#[test]
fn new_invalid_xml_and_zero_size_shapes_are_rejected() {
    let mut pres = Presentation::new("invalid");
    pres.add_slide(Slide::with_title("bad\u{1}"));
    assert!(write_pptx(&pres).is_err());
    pres.slides[0].shapes[0].width = 0;
    assert!(write_pptx(&pres).is_err());
}
#[test]
fn native_placeholders_tables_and_connectors_survive_text_edit() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bytes = std::fs::read(root.join(
        "RCD/red-team-corpus/office-targets-2026-10/pptx/zen_presentation_multiscenario.pptx",
    ))
    .unwrap();
    let mut pres = read_pptx_bytes(&bytes).unwrap();
    assert_eq!(write_pptx(&pres).unwrap(), bytes);
    for shape in &pres.slides[0].shapes {
        assert!(
            shape.width > 1_000_000 && shape.height > 100_000,
            "placeholder transform {:?}",
            shape
        );
    }
    assert_eq!(pres.replace_text_preserving("Review", "Report").unwrap(), 1);
    let out = write_pptx(&pres).unwrap();
    let a = zen_opc::ZipArchive::read(&bytes).unwrap();
    let b = zen_opc::ZipArchive::read(&out).unwrap();
    for e in &a.entries {
        if ![
            "ppt/slides/slide1.xml",
            "_rels/.rels",
            "docProps/thumbnail.jpeg",
            "[Content_Types].xml",
        ]
        .contains(&e.name.as_str())
        {
            assert_eq!(b.get(&e.name), Some(e.data.as_slice()), "{}", e.name);
        }
    }
    assert!(b.get("docProps/thumbnail.jpeg").is_none());
    pres.slides[0].shapes[0].width += 1;
    assert!(write_pptx(&pres).is_err());
}
