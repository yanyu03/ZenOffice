use zen_word::*;

fn style(id: &str, parent: Option<&str>, caps: bool, kind: StyleKind) -> StyleDef {
    StyleDef { style_id:id.into(), name:id.into(), kind, based_on:parent.map(str::to_string),
        run:RunProps { caps:Some(caps), ..Default::default() }, para:Default::default() }
}
fn resolved(doc: &Document, p: &Paragraph) -> Paragraph {
    StyleResolver::new(&doc.styles).unwrap().resolve_paragraph(p).unwrap().paragraph
}

#[test]
fn caps_roundtrip_retains_none_on_off_and_literal_contents() {
    let mut doc=Document::default();
    for caps in [None,Some(true),Some(false)] {
        doc.blocks.push(Block::Paragraph(Paragraph::default().push_run(Run::styled(
            RunProps { caps, ..Default::default() },"Mixed case"))));
    }
    let bytes=write_docx(&doc).unwrap();
    let read=read_docx(&bytes).unwrap().document;
    assert_eq!(doc.blocks,read.blocks);
    assert!(extract_text(&read).contains("Mixed case"));
    assert!(!extract_text(&read).contains("MIXED CASE"));
    assert_eq!(write_docx(&read).unwrap(),bytes);
}

#[test]
fn paragraph_caps_uses_nearest_style_value_and_direct_false_cancels() {
    let mut doc=Document::default();
    doc.styles.named=vec![style("Base",None,true,StyleKind::Paragraph),
        style("Double",Some("Base"),true,StyleKind::Paragraph),
        style("OffStyle",Some("Base"),false,StyleKind::Paragraph)];
    for (name,expected) in [("Base",true),("Double",true),("OffStyle",false)] {
        let p=Paragraph::with_props(ParagraphProps::style(name)).push_run(Run::text("Mixed"));
        assert_eq!(resolved(&doc,&p).runs[0].props.caps,Some(expected));
    }
    let p=Paragraph::with_props(ParagraphProps::style("Base")).push_run(Run::styled(RunProps {caps:Some(false),..Default::default()},"Mixed"));
    assert_eq!(resolved(&doc,&p).runs[0].props.caps,Some(false));
}

#[test]
fn character_caps_toggles_paragraph_caps_and_direct_true_overrides() {
    let mut doc=Document::default();
    doc.styles.named=vec![style("Base",None,true,StyleKind::Paragraph),style("Char",None,true,StyleKind::Character)];
    let mut p=Paragraph::with_props(ParagraphProps::style("Base")).push_run(Run::styled(
        RunProps {style_id:Some("Char".into()),..Default::default()},"Mixed"));
    assert_eq!(resolved(&doc,&p).runs[0].props.caps,Some(false));
    p.runs[0].props.caps=Some(true);
    assert_eq!(resolved(&doc,&p).runs[0].props.caps,Some(true));
}

#[test]
fn layout_measures_displayed_capitals_without_mutating_the_model() {
    let p=Paragraph::default().push_run(Run::styled(RunProps {caps:Some(true),..Default::default()},"iii"));
    let before=p.clone();
    let lines=LineServices::format_paragraph(&p,10000);
    assert_eq!(lines[0].runs.iter().map(|r|r.text.as_str()).collect::<String>(),"III");
    assert_eq!(lines[0].width_twips,LineServices::measure_text_twips("III",21));
    assert!(lines[0].width_twips>LineServices::measure_text_twips("iii",21));
    assert_eq!(p,before);
}

#[test]
fn unicode_preview_expansion_preserves_raw_text_and_terminates() {
    let p=Paragraph::default().push_run(Run::styled(RunProps {caps:Some(true),..Default::default()},"aß中1!"));
    let lines=LineServices::format_paragraph(&p,300);
    assert_eq!(lines.iter().flat_map(|l|l.runs.iter()).map(|r|r.text.as_str()).collect::<String>(),"ASS中1!");
    assert_eq!(p.plain_text(),"aß中1!");
}

#[test]
#[ignore = "requires opt-in external SIU fixture; see docs/TESTING.md"]
fn siu_caps_resolves_and_previews_without_rewriting_the_imported_package() {
    let path=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../RCD/red-team-corpus/academic-word-2026-10/siu_thesis_template_original.docx");
    let original=std::fs::read(path).unwrap();
    let read=read_docx(&original).unwrap();
    assert!(!read.diagnostics.iter().any(|d|d.detail.contains("<caps>")));
    let before=read.document.clone();
    let mut checked=0;
    for block in &read.document.blocks {
        if let Block::Paragraph(p)=block && p.props.style_id.as_deref()==Some("Heading2") && p.plain_text().contains("chapter title") {
            let effective=resolved(&read.document,p);
            assert!(effective.runs.iter().all(|r|r.props.caps==Some(true)));
            checked+=1;
        }
    }
    assert_eq!(checked,3);
    let layout=PageTableServices::layout_document_checked(&read.document).unwrap();
    let displayed=layout.pages.iter().flat_map(|p|p.placed_lines.iter()).map(|(_,_,line)|line.runs.iter().map(|r|r.text.as_str()).collect::<String>()).collect::<Vec<_>>();
    assert_eq!(displayed.iter().filter(|s|s.as_str()=="CHAPTER TITLE").count(),3);
    assert_eq!(read.document,before);
    assert_eq!(write_docx(&read.document).unwrap(),original);
}

#[test]
fn imported_caps_format_edits_still_require_a_preserving_operation() {
    let original=write_docx(&Document {blocks:vec![Block::paragraph("Mixed")],..Default::default()}).unwrap();
    let mut read=read_docx(&original).unwrap().document;
    let Block::Paragraph(p)=&mut read.blocks[0] else {panic!()};
    p.runs[0].props.caps=Some(true);
    let before=read.clone();
    assert!(matches!(write_docx(&read),Err(WordError::UnsupportedEdit(_))));
    assert_eq!(read,before);
}

#[test]
fn controlled_caps_cases_resolve_and_export_for_optional_native_audit() {
    let mut doc=Document::default();
    doc.styles.named=vec![style("Base",None,true,StyleKind::Paragraph),
        style("Double",Some("Base"),true,StyleKind::Paragraph),
        style("OffStyle",Some("Base"),false,StyleKind::Paragraph),
        style("Char",None,true,StyleKind::Character)];
    let cases=[("Direct mixed",None,None,Some(true),true),
        ("Direct lower",Some("Base"),None,Some(false),false),
        ("Inherited lower",Some("Base"),None,None,true),
        ("Toggled lower",Some("Double"),None,None,true),
        ("Style false inherits",Some("OffStyle"),None,None,false),
        ("Character lower",None,Some("Char"),None,true),
        ("Combined lower",Some("Base"),Some("Char"),None,false)];
    let mut rows=String::from("index\traw\tdisplay\tcaps\n");
    for (i,(text,para_style,char_style,direct,expected)) in cases.into_iter().enumerate() {
        let p=Paragraph::with_props(ParagraphProps {style_id:para_style.map(str::to_string),..Default::default()}).push_run(Run::styled(
            RunProps {style_id:char_style.map(str::to_string),caps:direct,..Default::default()},text));
        let effective=resolved(&doc,&p);
        assert_eq!(effective.runs[0].props.caps.unwrap_or(false),expected);
        let display=effective.runs[0].props.display_text(text);
        assert_eq!(display,if expected {text.to_uppercase()} else {text.into()});
        rows.push_str(&format!("{}\t{text}\t{display}\t{expected}\n",i+1));
        doc.blocks.push(Block::Paragraph(p));
    }
    let bytes=write_docx(&doc).unwrap();
    let read=read_docx(&bytes).unwrap().document;
    assert_eq!(read.blocks,doc.blocks);
    if let Some(path)=std::env::var_os("ZEN_CAPS_AUDIT_DIR") {
        let out=std::path::PathBuf::from(path);std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join("caps_cases.docx"),bytes).unwrap();
        std::fs::write(out.join("caps_cases.tsv"),rows).unwrap();
    }
}
