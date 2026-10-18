use zen_word::*;

fn document() -> Document {
    let mut doc = Document::default();
    doc.section = SectionProps {
        page_width_twips: 4000,
        page_height_twips: 2000,
        margin_top_twips: 500,
        margin_bottom_twips: 500,
        margin_left_twips: 200,
        margin_right_twips: 200,
        ..Default::default()
    };
    doc.styles.default_para.line_spacing = Some(LineSpacing::Exact(250));
    doc
}

fn paragraph(text: &str, keep: bool) -> Block {
    Block::Paragraph(
        Paragraph::with_props(ParagraphProps {
            keep_next: keep,
            ..Default::default()
        })
        .push_run(Run::text(text)),
    )
}

fn texts(layout: &LayoutDocument) -> Vec<Vec<String>> {
    layout
        .pages
        .iter()
        .map(|page| {
            page.placed_lines
                .iter()
                .map(|(_, _, line)| line.runs.iter().map(|run| run.text.as_str()).collect())
                .collect()
        })
        .collect()
}

#[test]
fn keep_requirement_includes_both_paragraphs_spacing() {
    let mut doc = document();
    doc.blocks = vec![
        paragraph("F1", false),
        paragraph("F2", false),
        paragraph("Title", true),
        paragraph("Body", false),
    ];
    let Block::Paragraph(title) = &mut doc.blocks[2] else {
        panic!()
    };
    title.props.space_after_twips = Some(100);
    let Block::Paragraph(body) = &mut doc.blocks[3] else {
        panic!()
    };
    body.props.space_before_twips = Some(100);
    let layout = PageTableServices::layout_document_checked(&doc).unwrap();
    assert_eq!(
        texts(&layout),
        vec![vec!["F1", "F2"], vec!["Title", "Body"]]
    );
    assert_eq!(layout.pages[1].placed_lines[1].1, 950);
}

#[test]
fn exact_fit_including_spacing_does_not_move_the_group() {
    let mut doc = document();
    doc.blocks = vec![
        paragraph("F1", false),
        paragraph("Title", true),
        paragraph("Body", false),
    ];
    let Block::Paragraph(title) = &mut doc.blocks[1] else {
        panic!()
    };
    title.props.space_before_twips = Some(100);
    title.props.space_after_twips = Some(50);
    let Block::Paragraph(body) = &mut doc.blocks[2] else {
        panic!()
    };
    body.props.space_before_twips = Some(100);
    let layout = PageTableServices::layout_document_checked(&doc).unwrap();
    assert_eq!(texts(&layout), vec![vec!["F1", "Title", "Body"]]);
    assert_eq!(layout.pages[0].placed_lines[2].1, 1250);
}

#[test]
fn inherited_keep_next_is_used_by_pagination() {
    let mut doc = document();
    doc.styles.named.push(StyleDef {
        style_id: "KeepStyle".into(),
        name: "Keep Style".into(),
        kind: StyleKind::Paragraph,
        based_on: None,
        run: Default::default(),
        para: ParagraphProps {
            keep_next: true,
            ..Default::default()
        },
    });
    doc.blocks = vec![
        paragraph("F1", false),
        paragraph("F2", false),
        paragraph("F3", false),
        paragraph("Title", false),
        paragraph("Body", false),
    ];
    let Block::Paragraph(title) = &mut doc.blocks[3] else {
        panic!()
    };
    title.props.style_id = Some("KeepStyle".into());
    let original = doc.clone();
    let layout = PageTableServices::layout_document_checked(&doc).unwrap();
    assert_eq!(
        texts(&layout),
        vec![vec!["F1", "F2", "F3"], vec!["Title", "Body"]]
    );
    assert_eq!(doc, original, "preview must not modify the source model");
}

#[test]
fn keep_next_does_not_cross_a_section_boundary() {
    let mut doc = document();
    doc.blocks = vec![
        paragraph("F1", false),
        paragraph("F2", false),
        paragraph("F3", false),
        paragraph("Title", true),
    ];
    doc.append_section(doc.section.clone()).unwrap();
    doc.blocks.push(paragraph("Body", false));
    let layout = PageTableServices::layout_document_checked(&doc).unwrap();
    assert_eq!(
        texts(&layout),
        vec![vec!["F1", "F2", "F3", "Title"], vec!["Body"]]
    );
    assert_eq!(layout.pages[1].section_index, 1);
}

#[test]
fn hard_break_inside_a_heading_takes_priority_with_a_diagnostic() {
    let mut doc = document();
    let mut title = Paragraph::with_props(ParagraphProps {
        keep_next: true,
        ..Default::default()
    });
    title.runs.push(Run {
        props: Default::default(),
        contents: vec![
            RunContent::Text("Title".into()),
            RunContent::Break(BreakKind::Page),
            RunContent::Text("Continued".into()),
        ],
    });
    doc.blocks = vec![
        paragraph("F1", false),
        paragraph("F2", false),
        paragraph("F3", false),
        Block::Paragraph(title),
        paragraph("Body", false),
    ];
    let layout = PageTableServices::layout_document_checked(&doc).unwrap();
    assert_eq!(
        texts(&layout),
        vec![vec!["F1", "F2", "F3", "Title"], vec!["Continued", "Body"]]
    );
    assert!(layout.diagnostics.iter().any(|s| s.contains("hard break")));
}

#[test]
fn next_paragraph_starting_with_a_hard_break_is_not_pulled_back() {
    let mut doc = document();
    let body = Paragraph {
        props: Default::default(),
        runs: vec![Run {
            props: Default::default(),
            contents: vec![
                RunContent::Break(BreakKind::Page),
                RunContent::Text("Body".into()),
            ],
        }],
    };
    doc.blocks = vec![
        paragraph("F1", false),
        paragraph("F2", false),
        paragraph("F3", false),
        paragraph("Title", true),
        Block::Paragraph(body),
    ];
    let layout = PageTableServices::layout_document_checked(&doc).unwrap();
    assert_eq!(
        texts(&layout),
        vec![vec!["F1", "F2", "F3", "Title"], vec!["Body"]]
    );
}

#[test]
fn table_boundary_reports_the_unsupported_keep_relationship() {
    let mut doc = document();
    let mut table = Table::new(vec![1500]);
    table.rows.push(TableRow::from_texts(["Cell"]));
    doc.blocks = vec![paragraph("Title", true), Block::Table(table)];
    let layout = PageTableServices::layout_document_checked(&doc).unwrap();
    assert!(
        layout
            .diagnostics
            .iter()
            .any(|s| s.contains("across a table"))
    );
    assert_eq!(layout.pages[0].placed_cells.len(), 1);
}

#[test]
fn long_oversized_chain_relaxes_once_without_losing_or_overflowing_lines() {
    let mut doc = document();
    for i in 0..5000 {
        doc.blocks.push(paragraph("H", i < 4999));
    }
    let layout = PageTableServices::layout_document_checked(&doc).unwrap();
    assert_eq!(layout.total_pages, 1250);
    assert_eq!(
        layout
            .pages
            .iter()
            .map(|page| page.placed_lines.len())
            .sum::<usize>(),
        5000
    );
    assert_eq!(
        layout
            .diagnostics
            .iter()
            .filter(|s| s.contains("group exceeds"))
            .count(),
        1
    );
    for page in &layout.pages {
        assert_eq!(page.placed_lines.len(), 4);
        for (_, y, line) in &page.placed_lines {
            assert!(*y >= 500 && *y + line.height_twips as i32 <= 1500);
        }
    }
}
