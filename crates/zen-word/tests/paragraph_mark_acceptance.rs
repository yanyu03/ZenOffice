use zen_word::*;
fn resolved(doc:&Document,p:&Paragraph)->Paragraph {
    StyleResolver::new(&doc.styles).unwrap().resolve_paragraph(p).unwrap().paragraph
}
fn empty(size:Option<u32>)->Paragraph {
    Paragraph::with_props(ParagraphProps {mark_run:RunProps {size_half_points:size,..Default::default()},..Default::default()})
}
fn style(id:&str,kind:StyleKind,parent:Option<&str>,size:u32)->StyleDef {
    StyleDef {style_id:id.into(),name:id.into(),kind,based_on:parent.map(str::to_string),
        run:RunProps {size_half_points:Some(size),..Default::default()},para:Default::default()}
}

#[test]
fn mark_properties_roundtrip_and_precede_section_properties() {
    let mut doc=Document::default();
    let mut p=empty(Some(20));p.props.mark_run.font_ascii=Some("Consolas".into());
    p.props.mark_run.bold=Some(false);p.props.section_end=Some(doc.section.clone());
    doc.blocks=vec![Block::Paragraph(p),Block::paragraph("Body")];
    let bytes=write_docx(&doc).unwrap();let read=read_docx(&bytes).unwrap();
    assert_eq!(doc.blocks,read.document.blocks);
    assert_eq!(write_docx(&read.document).unwrap(),bytes);
    let xml=read.package.get_text("/word/document.xml").unwrap();
    let x=roxmltree::Document::parse(&xml).unwrap();
    let props=x.descendants().find(|n|n.has_tag_name((write::W_NS,"pPr"))).unwrap();
    assert_eq!(props.children().filter(|n|n.is_element()).map(|n|n.tag_name().name()).collect::<Vec<_>>(),vec!["rPr","sectPr"]);
    assert!(write::referenced_fonts(&doc).contains(&"Consolas".to_string()));
}

#[test]
fn empty_mark_size_overrides_inherited_font_and_changes_line_box() {
    let mut doc=Document::default();doc.styles.default_run.size_half_points=Some(24);
    let p=empty(Some(20));doc.blocks.push(Block::Paragraph(p.clone()));
    assert_eq!(resolved(&doc,&p).runs[0].props.size_half_points,Some(20));
    let layout=PageTableServices::layout_document_checked(&doc).unwrap();
    assert_eq!(layout.pages[0].placed_lines[0].2.height_twips,240);
    let mut inherited=doc.clone();inherited.blocks=vec![Block::Paragraph(empty(None))];
    assert_eq!(PageTableServices::layout_document_checked(&inherited).unwrap().pages[0].placed_lines[0].2.height_twips,288);
}

#[test]
fn paragraph_and_character_style_inheritance_precedes_direct_mark_properties() {
    let mut doc=Document::default();
    doc.styles.named=vec![style("Body",StyleKind::Paragraph,Some("Normal"),28),style("Mark",StyleKind::Character,None,26)];
    let mut p=empty(None);p.props.style_id=Some("Body".into());
    assert_eq!(resolved(&doc,&p).runs[0].props.size_half_points,Some(28));
    p.props.mark_run.style_id=Some("Mark".into());
    assert_eq!(resolved(&doc,&p).runs[0].props.size_half_points,Some(26));
    p.props.mark_run.size_half_points=Some(20);
    assert_eq!(resolved(&doc,&p).runs[0].props.size_half_points,Some(20));
}

#[test]
fn mark_format_does_not_leak_into_visible_text() {
    let mut doc=Document::default();doc.styles.default_run.size_half_points=Some(22);
    let p=empty(Some(80)).push_run(Run::text("Body"));
    let effective=resolved(&doc,&p);
    assert_eq!(effective.runs[0].props.size_half_points,Some(22));
    assert_eq!(effective.props.mark_run.size_half_points,Some(80));
    assert_eq!(effective.plain_text(),"Body");
}

#[test]
fn empty_placeholder_run_uses_mark_and_keeps_the_source_unchanged() {
    let doc=Document::default();
    let p=empty(Some(20)).push_run(Run::text(""));let before=p.clone();
    assert_eq!(resolved(&doc,&p).runs[0].props.size_half_points,Some(20));
    assert_eq!(p,before);
}

#[test]
fn zero_mark_font_is_rejected_by_preview_without_modifying_source() {
    let mut doc=Document::default();doc.blocks.push(Block::Paragraph(empty(Some(0))));
    let before=doc.clone();assert!(PageTableServices::layout_document_checked(&doc).is_err());assert_eq!(doc,before);
}

#[test]
#[ignore = "requires opt-in external SIU fixture; see docs/TESTING.md"]
fn siu_twenty_explicit_ten_point_blank_marks_resolve_and_preserve_original_bytes() {
    let path=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../RCD/red-team-corpus/academic-word-2026-10/siu_thesis_template_original.docx");
    let bytes=std::fs::read(path).unwrap();let read=read_docx(&bytes).unwrap();
    let before=read.document.clone();let mut count=0;
    for block in &read.document.blocks {
        if let Block::Paragraph(p)=block && p.runs.is_empty() && p.props.mark_run.size_half_points==Some(20) {
            assert_eq!(resolved(&read.document,p).runs[0].props.size_half_points,Some(20));count+=1;
        }
    }
    assert_eq!(count,20);
    PageTableServices::layout_document_checked(&read.document).unwrap();
    assert_eq!(read.document,before);assert_eq!(write_docx(&read.document).unwrap(),bytes);
}

#[test]
fn editing_an_imported_mark_format_is_still_refused() {
    let original=write_docx(&Document {blocks:vec![Block::Paragraph(empty(None))],..Default::default()}).unwrap();
    let mut doc=read_docx(&original).unwrap().document;
    let Block::Paragraph(p)=&mut doc.blocks[0] else {panic!()};p.props.mark_run.size_half_points=Some(20);
    let before=doc.clone();assert!(matches!(write_docx(&doc),Err(WordError::UnsupportedEdit(_))));assert_eq!(doc,before);
}

#[test]
fn controlled_empty_mark_cases_resolve_and_export_for_native_audit() {
    let mut doc=Document::default();doc.styles.default_run.size_half_points=Some(24);
    doc.styles.named=vec![style("Body",StyleKind::Paragraph,Some("Normal"),28),style("Mark",StyleKind::Character,None,26)];
    let mut cases=vec![empty(Some(20)),empty(None),empty(None),empty(None),empty(Some(20))];
    cases[2].props.style_id=Some("Body".into());
    cases[3].props.mark_run.style_id=Some("Mark".into());cases[4].props.mark_run.style_id=Some("Mark".into());
    let mut rows=String::from("index\tfont_hp\n");
    for (i,(p,expected)) in cases.into_iter().zip([20,24,28,26,20]).enumerate() {
        assert_eq!(resolved(&doc,&p).runs[0].props.size_half_points,Some(expected));
        rows.push_str(&format!("{}\t{expected}\n",i+1));doc.blocks.push(Block::Paragraph(p));
    }
    let bytes=write_docx(&doc).unwrap();let read=read_docx(&bytes).unwrap().document;assert_eq!(doc.blocks,read.blocks);
    if let Some(path)=std::env::var_os("ZEN_MARK_AUDIT_DIR") {
        let out=std::path::PathBuf::from(path);std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join("mark_cases.docx"),bytes).unwrap();std::fs::write(out.join("mark_cases.tsv"),rows).unwrap();
    }
}
