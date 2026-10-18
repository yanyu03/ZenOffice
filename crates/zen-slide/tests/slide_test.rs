use zen_slide::*;

#[test]
fn test_create_slide_presentation_and_roundtrip() {
    let mut pres = Presentation::new("Zen Headless Slide Demo");
    pres.author = "Zen Contributor".into();

    // 幻灯片 1：封面页
    let mut slide1 = Slide::with_title("Zen PresentationML Engine");
    let mut sub_shape = Shape::textbox(
        EMU_PER_INCH,
        EMU_PER_INCH * 2,
        SLIDE_WIDTH_16_9 - 2 * EMU_PER_INCH,
        EMU_PER_INCH,
    );
    sub_shape.name = "Subtitle".into();
    sub_shape.add_paragraph(Paragraph::body("Built strictly upon ECMA-376 OOXML Open Standards."));
    slide1.add_shape(sub_shape);
    pres.add_slide(slide1);

    // 幻灯片 2：内容页
    let mut slide2 = Slide::with_title("Architecture & Features");
    let mut card = Shape::rectangle(
        EMU_PER_INCH,
        EMU_PER_INCH * 2,
        EMU_PER_INCH * 5,
        EMU_PER_INCH * 3,
        "417FF9",
    );
    card.name = "FeatureCard".into();
    card.add_paragraph(Paragraph::text("Native Rust / Headless / Pure Standards"));
    slide2.add_shape(card);
    pres.add_slide(slide2);

    assert_eq!(pres.slides.len(), 2);

    // 序列化为 PPTX 字节
    let pptx_bytes = write_pptx(&pres).expect("PPTX generation should succeed");
    assert!(pptx_bytes.starts_with(b"PK\x03\x04"), "Must be a valid ZIP archive");

    // 反序列化校验
    let read_pres = read_pptx_bytes(&pptx_bytes).expect("PPTX parse should succeed");
    assert_eq!(read_pres.title, "Zen Headless Slide Demo");
    assert_eq!(read_pres.author, "Zen Contributor");
    assert_eq!(read_pres.slides.len(), 2);
    assert_eq!(read_pres.width_emu, SLIDE_WIDTH_16_9);
    assert_eq!(read_pres.height_emu, SLIDE_HEIGHT_16_9);

    // 验证 Slide 1 文本内容
    let s1 = &read_pres.slides[0];
    assert!(s1.shapes.len() >= 2);
    let title_shape = &s1.shapes[0];
    let tb = title_shape.text_body.as_ref().expect("Title must have text body");
    assert_eq!(tb.paragraphs[0].runs[0].text, "Zen PresentationML Engine");
}

#[test]
fn test_slide_empty_and_custom_size() {
    let mut pres = Presentation::new("4:3 Presentation");
    pres.width_emu = SLIDE_WIDTH_4_3;
    pres.height_emu = SLIDE_HEIGHT_4_3;
    pres.add_slide(Slide::new());

    let pptx_bytes = write_pptx(&pres).expect("PPTX generation should succeed");
    let read_pres = read_pptx_bytes(&pptx_bytes).expect("PPTX parse should succeed");

    assert_eq!(read_pres.width_emu, SLIDE_WIDTH_4_3);
    assert_eq!(read_pres.height_emu, SLIDE_HEIGHT_4_3);
    assert_eq!(read_pres.slides.len(), 1);
}

#[test]
fn test_rich_shapes_roundtrip() {
    let mut pres = Presentation::new("Rich Shapes Test");
    let mut slide = Slide::with_title("Geometry Gallery");

    slide.add_shape(Shape::rounded_rectangle(100_000, 100_000, 500_000, 300_000, "10B981"));
    slide.add_shape(Shape::ellipse(700_000, 100_000, 400_000, 400_000, "6366F1"));
    slide.add_shape(Shape::card(
        100_000,
        500_000,
        1_000_000,
        600_000,
        "0F172A",
        "Executive Summary",
        "Deterministic pipeline with schema compliance.",
    ));
    pres.add_slide(slide);

    let bytes = write_pptx(&pres).expect("Should write PPTX");
    let parsed = read_pptx_bytes(&bytes).expect("Should read PPTX");

    assert_eq!(parsed.slides.len(), 1);
    let s = &parsed.slides[0];
    assert!(s.shapes.iter().any(|sh| sh.kind == ShapeKind::RoundedRectangle && sh.fill_color.as_deref() == Some("10B981")));
    assert!(s.shapes.iter().any(|sh| sh.kind == ShapeKind::Ellipse && sh.fill_color.as_deref() == Some("6366F1")));
    assert!(s.shapes.iter().any(|sh| sh.name == "Card" && sh.kind == ShapeKind::RoundedRectangle && sh.fill_color.as_deref() == Some("0F172A")));
}

#[test]
fn test_red_team_rt05_arbitrary_slide_names_and_malformed_xml_rejection() {
    // 1. 测试非 slide{N}.xml 命名与关系映射
    let mut pres = Presentation::new("Arbitrary Slide Names");
    let mut s1 = Slide::with_title("Slide Alpha");
    s1.add_shape(Shape::rectangle(0, 0, 100_000, 100_000, "FFFFFF"));
    pres.add_slide(s1);

    let mut pkg = build_package(&pres).expect("build package");
    // 人工篡改：将 slide1.xml 重命名为 slide_custom_name.xml 并更新关系
    let orig_slide = pkg.get("/ppt/slides/slide1.xml").expect("slide1").to_vec();
    // 移除原 slide1.xml 并写入新部件
    pkg.parts.retain(|p| p.name != "/ppt/slides/slide1.xml");
    pkg.put("/ppt/slides/slide_custom_name.xml", orig_slide).expect("put");

    let mut pres_rels = zen_opc::Relationships::default();
    pres_rels.add("rId1", write::REL_SLIDE_MASTER, "slideMasters/slideMaster1.xml");
    pres_rels.add("rId2", write::REL_SLIDE, "slides/slide_custom_name.xml");
    pkg.set_rels_for("/ppt/presentation.xml", &pres_rels).expect("set rels");

    let bytes = pkg.to_bytes().expect("to_bytes");
    let parsed = read_pptx_bytes(&bytes).expect("Should successfully parse custom named slide");
    assert_eq!(parsed.slides.len(), 1);

    // 2. 测试畸形 XML 拒绝（绝不应静默吞错）
    let mut bad_pkg = pkg.clone();
    bad_pkg.put_text("/ppt/slides/slide_custom_name.xml", "<p:sld><unclosed_tag>").expect("put bad xml");
    let bad_bytes = bad_pkg.to_bytes().expect("to_bytes");
    let err = read_pptx_bytes(&bad_bytes).expect_err("Malformed XML must return Err");
    match err {
        SlideError::Xml(_) => {}
        other => panic!("Expected SlideError::Xml, got: {other:?}"),
    }
}

#[test]
fn test_reject_non_slide_relationship_type() {
    let mut pres = Presentation::new("Rel Type Check");
    pres.add_slide(Slide::with_title("Slide"));
    let mut pkg = build_package(&pres).expect("build package");

    // 将 rId2 (slide1) 的关系类型篡改为非 slide 类型（例如 notesMaster 或 arbitrary）
    let mut bad_rels = zen_opc::Relationships::default();
    bad_rels.add("rId1", write::REL_SLIDE_MASTER, "slideMasters/slideMaster1.xml");
    bad_rels.add("rId2", "http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesMaster", "slides/slide1.xml");
    pkg.set_rels_for("/ppt/presentation.xml", &bad_rels).expect("set rels");

    let bad_bytes = pkg.to_bytes().expect("to_bytes");
    let err = read_pptx_bytes(&bad_bytes).expect_err("Must reject non-slide rel type in sldIdLst");
    match err {
        SlideError::Invalid(msg) => {
            assert!(msg.contains("关系类型不是 slide"), "Actual error msg: {msg}");
        }
        other => panic!("Expected SlideError::Invalid, got: {other:?}"),
    }
}

#[test]
fn test_reject_malformed_relationship_xml() {
    let mut pres = Presentation::new("Malformed Rels Test");
    pres.add_slide(Slide::with_title("Slide"));
    let mut pkg = build_package(&pres).expect("build package");

    // 篡改 presentation.xml.rels 为坏 XML
    pkg.put_text("/ppt/_rels/presentation.xml.rels", "<Relationships><Relationship id=").expect("put text");
    let bad_bytes = pkg.to_bytes().expect("to_bytes");
    let err = read_pptx_bytes(&bad_bytes).expect_err("Must reject malformed rels XML");
    match err {
        SlideError::Opc(zen_opc::OpcError::Xml(msg)) => {
            assert!(msg.contains("Relationships XML 语法错误") || msg.contains("解析失败"), "{msg}");
        }
        other => panic!("Expected SlideError::Opc(Xml), got: {other:?}"),
    }
}

