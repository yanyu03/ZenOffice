use zen_word::*;

fn style(id: &str, kind: StyleKind, parent: Option<&str>, run: RunProps, para: ParagraphProps) -> StyleDef {
    StyleDef { style_id: id.into(), name: id.into(), kind, based_on: parent.map(str::to_string), run, para }
}
fn effective(doc: &Document, p: &Paragraph) -> Paragraph {
    StyleResolver::new(&doc.styles).unwrap().resolve_paragraph(p).unwrap().paragraph
}

#[test]
fn inherited_properties_and_direct_overrides_have_distinct_precedence() {
    let mut doc = Document::default();
    doc.styles.default_run = RunProps { size_half_points: Some(20), font_ascii: Some("Arial".into()), ..Default::default() };
    doc.styles.named.push(style("Body", StyleKind::Paragraph, Some("Normal"), RunProps { size_half_points: Some(24), font_east_asia: Some("宋体".into()), ..Default::default() }, ParagraphProps { space_after_twips: Some(120), ..Default::default() }));
    doc.styles.named.push(style("Emphasis", StyleKind::Character, None, RunProps { italic: Some(true), size_half_points: Some(28), ..Default::default() }, Default::default()));
    let mut p = Paragraph::with_props(ParagraphProps::style("Body"));
    p.runs.push(Run::styled(RunProps { style_id: Some("Emphasis".into()), size_half_points: Some(30), ..Default::default() }, "正文"));
    let before = doc.clone();
    let r = effective(&doc, &p);
    assert_eq!(r.runs[0].props.size_half_points, Some(30));
    assert_eq!(r.runs[0].props.font_ascii.as_deref(), Some("Arial"));
    assert_eq!(r.runs[0].props.font_east_asia.as_deref(), Some("宋体"));
    assert_eq!(r.runs[0].props.italic, Some(true));
    assert_eq!(r.props.space_after_twips, Some(120));
    assert_eq!(doc, before, "resolution must not modify the document");
}

#[test]
fn style_toggle_and_direct_false_are_not_the_same_operation() {
    let mut doc = Document::default();
    doc.styles.named.push(style("Parent", StyleKind::Paragraph, None, RunProps { bold: Some(true), ..Default::default() }, Default::default()));
    doc.styles.named.push(style("Child", StyleKind::Paragraph, Some("Parent"), RunProps { bold: Some(true), ..Default::default() }, Default::default()));
    let p = Paragraph::with_props(ParagraphProps::style("Child")).push_run(Run::text("x"));
    assert_eq!(effective(&doc, &p).runs[0].props.bold, Some(false));
    let p = Paragraph::with_props(ParagraphProps::style("Parent")).push_run(Run::styled(RunProps { bold: Some(false), ..Default::default() }, "x"));
    assert_eq!(effective(&doc, &p).runs[0].props.bold, Some(false));
}

#[test]
fn explicit_paragraph_off_and_auto_color_cancel_inherited_values() {
    let mut doc = Document::default();
    doc.styles.named.push(style("Base", StyleKind::Paragraph, None, RunProps { color: Some("FF0000".into()), ..Default::default() }, ParagraphProps { keep_next: true, page_break_before: true, ..Default::default() }));
    let p = Paragraph::with_props(ParagraphProps { style_id: Some("Base".into()), keep_next_off: true, page_break_before_off: true, ..Default::default() }).push_run(Run::styled(RunProps { color_auto: true, ..Default::default() }, "x"));
    let r = effective(&doc, &p);
    assert!(!r.props.keep_next && !r.props.page_break_before);
    assert!(r.runs[0].props.color_auto && r.runs[0].props.color.is_none());
    doc.blocks.push(Block::Paragraph(p));
    let read = read_docx(&write_docx(&doc).unwrap()).unwrap().document;
    let Block::Paragraph(p) = &read.blocks[0] else { panic!() };
    let r = effective(&read, p);
    assert!(!r.props.keep_next && !r.props.page_break_before);
    assert!(r.runs[0].props.color_auto);
}

#[test]
fn external_builtin_override_and_character_reference_survive_serialization() {
    let mut doc = Document::default();
    let mut heading = template_style_defs().into_iter().find(|s| s.style_id == "Heading1").unwrap();
    heading.run.size_half_points = Some(40);
    doc.styles.builtin_overrides.push(heading);
    doc.styles.named.push(style("Accent", StyleKind::Character, None, RunProps { font_ascii: Some("Consolas".into()), ..Default::default() }, Default::default()));
    doc.blocks.push(Block::Paragraph(Paragraph::with_props(ParagraphProps::style("Heading1")).push_run(Run::styled(RunProps { style_id: Some("Accent".into()), ..Default::default() }, "Title"))));
    let bytes = write_docx(&doc).unwrap();
    let read = read_docx(&bytes).unwrap().document;
    let Block::Paragraph(p) = &read.blocks[0] else { panic!() };
    let r = effective(&read, p);
    assert_eq!(r.runs[0].props.size_half_points, Some(40));
    assert_eq!(r.runs[0].props.font_ascii.as_deref(), Some("Consolas"));
    assert_eq!(write_docx(&read).unwrap(), bytes);
}

#[test]
fn explicit_default_style_is_used_without_a_paragraph_reference() {
    let mut doc = Document::default();
    doc.styles.default_paragraph_style_id = Some("Body".into());
    doc.styles.named.push(style("Body", StyleKind::Paragraph, None, RunProps { size_half_points: Some(26), ..Default::default() }, Default::default()));
    doc.blocks.push(Block::paragraph("x"));
    let read = read_docx(&write_docx(&doc).unwrap()).unwrap().document;
    let Block::Paragraph(p) = &read.blocks[0] else { panic!() };
    assert_eq!(effective(&read, p).runs[0].props.size_half_points, Some(26));
}

#[test]
fn missing_imported_style_is_not_replaced_with_a_template_heading() {
    let mut doc = Document::default();
    doc.styles.imported_style_ids = Some(vec![]);
    let resolver = StyleResolver::new(&doc.styles).unwrap();
    let p = Paragraph::with_props(ParagraphProps::style("Heading1")).push_run(Run::text("x"));
    let r = resolver.resolve_paragraph(&p).unwrap();
    assert_eq!(r.paragraph.runs[0].props.size_half_points, None);
    assert!(!r.diagnostics.is_empty());
}

#[test]
fn cycles_depth_and_duplicate_ids_are_rejected() {
    let mut doc = Document::default();
    doc.styles.named.push(style("A", StyleKind::Paragraph, Some("B"), Default::default(), Default::default()));
    doc.styles.named.push(style("B", StyleKind::Paragraph, Some("A"), Default::default(), Default::default()));
    assert!(StyleResolver::new(&doc.styles).unwrap().resolve_paragraph(&Paragraph::with_props(ParagraphProps::style("A"))).is_err());
    doc.styles.named.clear();
    for i in 0..257 {
        doc.styles.named.push(style(&format!("S{i}"), StyleKind::Paragraph, (i<256).then(|| format!("S{}",i+1)).as_deref(), Default::default(), Default::default()));
    }
    assert!(StyleResolver::new(&doc.styles).unwrap().resolve_paragraph(&Paragraph::with_props(ParagraphProps::style("S0"))).is_err());
    doc.styles.imported_style_ids = Some(vec!["Duplicate".into(),"Duplicate".into()]);
    assert!(StyleResolver::new(&doc.styles).is_err());
}

#[test]
fn wrong_parent_kind_is_diagnosed_without_applying_its_properties() {
    let mut doc = Document::default();
    doc.styles.named.push(style("Character", StyleKind::Character, None, RunProps { size_half_points: Some(80), ..Default::default() }, Default::default()));
    doc.styles.named.push(style("Body", StyleKind::Paragraph, Some("Character"), RunProps { italic: Some(true), ..Default::default() }, Default::default()));
    let r = StyleResolver::new(&doc.styles).unwrap().resolve_paragraph(&Paragraph::with_props(ParagraphProps::style("Body")).push_run(Run::text("x"))).unwrap();
    assert_eq!(r.paragraph.runs[0].props.size_half_points, None);
    assert_eq!(r.paragraph.runs[0].props.italic, Some(true));
    assert!(r.diagnostics.iter().any(|s| s.contains("wrong kind")));
}

#[test]
fn empty_paragraph_uses_inherited_font_and_exact_line_height() {
    let mut doc = Document::default();
    doc.styles.default_run.size_half_points = Some(16);
    doc.styles.default_para.line_spacing = Some(LineSpacing::Exact(400));
    doc.blocks.push(Block::paragraph(""));
    let layout = PageTableServices::layout_document_checked(&doc).unwrap();
    assert_eq!(layout.pages[0].placed_lines.len(), 1);
    assert_eq!(layout.pages[0].placed_lines[0].2.height_twips, 400);
}

#[test]
fn new_sections_roundtrip_and_layout_use_individual_geometry_and_numbering() {
    let mut doc = Document::default();
    doc.section.page_number_start = Some(7);
    doc.blocks.push(Block::paragraph("Portrait"));
    doc.append_section(SectionProps { page_width_twips: A4_HEIGHT_TWIPS, page_height_twips: A4_WIDTH_TWIPS, orientation: Orientation::Landscape, page_number_start: Some(1), ..Default::default() }).unwrap();
    doc.blocks.push(Block::paragraph("Landscape"));
    doc.append_section(SectionProps::default()).unwrap();
    doc.blocks.push(Block::paragraph("Portrait again"));
    let bytes = write_docx(&doc).unwrap();
    let read = read_docx(&bytes).unwrap().document;
    assert_eq!(read.sections().len(),3);
    let layout = PageTableServices::layout_document_checked(&read).unwrap();
    assert_eq!(layout.total_pages,3);
    assert_eq!(layout.pages.iter().map(|p|p.width_twips).collect::<Vec<_>>(),[A4_WIDTH_TWIPS,A4_HEIGHT_TWIPS,A4_WIDTH_TWIPS]);
    assert_eq!(layout.pages.iter().map(|p|p.printed_page_number).collect::<Vec<_>>(),[Some(7),Some(1),Some(2)]);
    assert_eq!(write_docx(&read).unwrap(),bytes);
}

#[test]
fn split_moves_section_end_and_backspace_cannot_delete_boundary() {
    let mut doc = Document::default();
    doc.blocks.push(Block::paragraph("AB"));
    doc.append_section(Default::default()).unwrap();
    doc.blocks.push(Block::paragraph("C"));
    doc.split_paragraph(Caret::new(0,0,1));
    assert!(doc.paragraph_at(0).unwrap().props.section_end.is_none());
    assert!(doc.paragraph_at(1).unwrap().props.section_end.is_some());
    let before = doc.clone();
    assert!(!doc.delete_backward(Caret::new(2,0,0)).changed);
    assert_eq!(doc,before);
}

#[test]
fn hard_page_break_and_unsupported_section_start_are_observable() {
    let mut doc = Document::default();
    doc.blocks.push(Block::Paragraph(Paragraph { props: Default::default(), runs: vec![Run { props: Default::default(), contents: vec![RunContent::Text("A".into()),RunContent::Break(BreakKind::Page),RunContent::Text("B".into())] }] }));
    let layout = PageTableServices::layout_document_checked(&doc).unwrap();
    assert_eq!(layout.total_pages,2);
    doc.append_section(SectionProps { start: SectionStart::Continuous, ..Default::default() }).unwrap();
    assert!(PageTableServices::layout_document_checked(&doc).is_err());
}

#[test]
fn styled_table_cells_use_effective_run_size() {
    let mut doc = Document::default();
    doc.styles.default_run.size_half_points = Some(36);
    doc.blocks.push(Block::Table(Table::new(vec![3000]).push_row(TableRow::from_texts(["cell"]))));
    let layout = PageTableServices::layout_document_checked(&doc).unwrap();
    let run = &layout.pages[0].placed_cells[0].lines[0].2.runs[0];
    assert_eq!(run.props.size_half_points,Some(36));
}

#[test]
fn imported_structure_insert_and_zero_font_fail_without_mutating_source() {
    let mut doc = Document::default();doc.blocks.push(Block::paragraph("x"));
    let bytes = write_docx(&doc).unwrap();
    let mut imported = read_docx(&bytes).unwrap().document;
    let before = imported.clone();
    assert!(imported.append_section(Default::default()).is_err());
    assert_eq!(imported,before);
    assert_eq!(write_docx(&imported).unwrap(),bytes);
    doc.styles.default_run.size_half_points=Some(0);
    assert!(PageTableServices::layout_document_checked(&doc).is_err());
}

#[test]
#[ignore = "requires opt-in external SIU fixture; see docs/TESTING.md"]
fn pinned_documents_export_effective_formats_for_native_audit() {
    use std::{fs,path::Path};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = std::env::var_os("ZEN_WORD_AUDIT_DIR").map(std::path::PathBuf::from)
        .unwrap_or_else(|| root.join("target/word_style_audit/current"));
    fs::create_dir_all(&output).unwrap();
    let targets = [
        ("office", "RCD/red-team-corpus/office-targets-2026-10/docx/zen_word_multiscenario.docx"),
        ("academic", "RCD/red-team-corpus/academic-word-2026-10/zen_academic_multiscenario.docx"),
        ("siu", "RCD/red-team-corpus/academic-word-2026-10/siu_thesis_template_original.docx"),
    ];
    let hex = |s: &str| s.as_bytes().iter().map(|b|format!("{b:02x}")).collect::<String>();
    let boolean = |v: Option<bool>| v.map(|b|if b {"1"} else {"0"}).unwrap_or("-");
    let mut rows = String::from("target\tpath\tparagraph_utf8_hex\trun_utf8_hex\tsize_half_points\tbold\titalic\n");
    let mut count = 0;
    for (label,path) in targets {
        let bytes = fs::read(root.join(path)).unwrap();
        let doc = read_docx(&bytes).unwrap().document;
        assert_eq!(write_docx(&doc).unwrap(),bytes);
        let resolver = StyleResolver::new(&doc.styles).unwrap();
        let mut selected = 0;
        for block in &doc.blocks {
            let Block::Paragraph(p) = block else { continue };
            let text = p.plain_text();
            if text.trim().is_empty() || text.chars().any(|c|c.is_control()) { continue; }
            let resolved = resolver.resolve_paragraph(p).unwrap();
            let Some(run) = resolved.paragraph.runs.iter().find(|r| !r.plain_text().is_empty()) else { continue };
            let Some(size) = run.props.size_half_points else { continue };
            rows.push_str(&format!("{label}\t{path}\t{}\t{}\t{size}\t{}\t{}\n",hex(&text),hex(&run.plain_text()),boolean(run.props.bold),boolean(run.props.italic)));
            count += 1; selected += 1;
            if selected == 20 { break; }
        }
        assert!(selected>0,"no resolvable paragraph candidates in {label}");
    }
    assert!(count>=15);
    fs::write(output.join("effective_format_candidates.tsv"),rows).unwrap();
}

#[test]
fn default_numbering_and_unknown_styles_cannot_replace_default_paragraph_style() {
    let mut doc = Document::default();
    doc.styles.default_run.size_half_points=Some(22);
    let mut normal=template_style_defs().into_iter().find(|s|s.style_id=="Normal").unwrap();
    normal.run.size_half_points=Some(24);
    doc.styles.builtin_overrides.push(normal);
    doc.blocks.push(Block::paragraph("Body"));
    let bytes=write_docx(&doc).unwrap();
    let mut package=zen_opc::Package::from_archive(&zen_opc::ZipArchive::read(&bytes).unwrap()).unwrap();
    let xml=package.get_text("/word/styles.xml").unwrap().replace("</w:styles>",r#"<w:style w:type="numbering" w:default="1" w:styleId="NoList"><w:name w:val="No List"/></w:style><w:style w:type="futureType" w:default="1" w:styleId="Future"><w:name w:val="Future"/></w:style></w:styles>"#);
    package.put_text("/word/styles.xml",xml).unwrap();
    let bytes=package.to_bytes().unwrap();
    let outcome=read_docx(&bytes).unwrap();
    assert_eq!(outcome.document.styles.default_paragraph_style_id.as_deref(),Some("Normal"));
    assert!(outcome.document.styles.named.iter().any(|s|s.style_id=="NoList" && s.kind==StyleKind::Numbering));
    let Block::Paragraph(p)=&outcome.document.blocks[0] else {panic!()};
    assert_eq!(effective(&outcome.document,p).runs[0].props.size_half_points,Some(24));
    assert_eq!(write_docx(&outcome.document).unwrap(),bytes);
    assert!(outcome.diagnostics.iter().any(|d|d.detail.contains("unknown type")));
}
