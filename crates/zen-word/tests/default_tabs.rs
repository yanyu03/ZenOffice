use zen_word::*;
use zen_word::layout::LineServices;

fn tab_doc(interval: u32, prefix: &str, count: usize) -> Document {
    let mut d = Document::default();
    d.settings.default_tab_stop_twips = interval;
    let mut p = Paragraph::text(prefix);
    p.runs[0].props.size_half_points = Some(24);
    p.runs.push(Run { props: p.runs[0].props.clone(), contents: vec![RunContent::Tab; count] });
    p.runs.push(Run::styled(p.runs[0].props.clone(), "END"));
    d.blocks.push(Block::Paragraph(p));
    d
}
fn marker(d: &Document) -> (usize, i32) {
    let before = d.clone();
    let l = PageTableServices::layout_document_checked(d).unwrap();
    assert_eq!(*d, before);
    l.pages[0].placed_lines.iter().enumerate().find_map(|(i, (_, _, line))| {
        line.runs.iter().find(|r| r.text == "END").map(|r| (i, r.x_twips))
    }).unwrap()
}
#[test]
fn leading_and_consecutive_tabs_use_document_grid() {
    for interval in [420, 720] {
        for prefix in ["", "A", "i"] {
            for count in 1..=3 {
                let d = tab_doc(interval, prefix, count);
                assert_eq!(marker(&d), (0, interval as i32 * count as i32));
            }
        }
    }
}
#[test]
fn tab_at_existing_stop_moves_to_next_stop_and_standalone_uses_420() {
    // Seven narrow 12pt letters = 7*60 = 420 twips in current width estimator.
    let d = tab_doc(420, "iiiiiii", 1);
    assert_eq!(marker(&d), (0, 840));
    let Block::Paragraph(p) = &d.blocks[0] else { unreachable!() };
    let lines = LineServices::format_paragraph(p, 5000);
    assert_eq!(lines[0].runs.last().unwrap().x_twips, 840);
}
#[test]
fn narrow_area_always_progresses_and_zero_is_rejected_without_mutation() {
    let mut d = tab_doc(720, "", 4);
    d.section.margin_left_twips = 1440;
    d.section.margin_right_twips = 1440;
    d.section.page_width_twips = 3080; // 200 twips: narrower than the tab, wider than a glyph.
    let before = d.clone();
    let l = PageTableServices::layout_document_checked(&d).unwrap();
    assert!(!l.pages.is_empty());
    assert_eq!(d, before);
    d.section.page_width_twips = 2980; // 100 twips cannot fit one estimated glyph.
    let before = d.clone();
    assert!(matches!(PageTableServices::layout_document_checked(&d), Err(WordError::UnsupportedEdit(_))));
    assert_eq!(d, before);
    d.settings.default_tab_stop_twips = 0;
    let before = d.clone();
    assert!(matches!(PageTableServices::layout_document_checked(&d), Err(WordError::UnsupportedEdit(_))));
    assert_eq!(d, before);
    // A legal zero setting remains serializable and byte-preserving on import.
    let bytes = write_docx(&d).unwrap();
    assert_eq!(write_docx(&read_docx(&bytes).unwrap().document).unwrap(), bytes);
}
#[test]
fn body_and_cell_share_document_interval() {
    let mut d = tab_doc(720, "A", 2);
    let Block::Paragraph(p) = &d.blocks[0] else { unreachable!() };
    let p = p.clone();
    let mut t = Table::default();
    t.grid_twips = vec![4000];
    t.rows.push(TableRow { cells: vec![TableCell { blocks: vec![Block::Paragraph(p)], ..Default::default() }], ..Default::default() });
    d.blocks.push(Block::Table(t));
    let l = PageTableServices::layout_document_checked(&d).unwrap();
    let cells = &l.pages[0].placed_cells;
    assert_eq!(cells.len(), 1);
    // Inspect placed line runs: both stories must have END at 1440 from their text origin.
    assert_eq!(marker(&d), (0, 1440));
    assert!(cells[0].lines.iter().any(|(_, _, line)| line.runs.iter().any(|r| r.text == "END" && r.x_twips == 1440)));
}

#[test]
fn exact_right_edge_tab_carries_following_text_but_not_the_previous_tab() {
    for (prefix, count, width) in [("A", 5, 3600), ("A", 6, 3600), ("CHAPTER", 11, 8640)] {
        let mut d = tab_doc(720, prefix, count);
        d.section.margin_left_twips = 1440;
        d.section.margin_right_twips = 1440;
        d.section.page_width_twips = width + 2880;
        assert_eq!(marker(&d), (1, 720));
        let before = d.clone();
        let l = PageTableServices::layout_document_checked(&d).unwrap();
        assert_eq!(l.pages[0].placed_lines.len(), 2);
        assert_eq!(d, before);
        if count == 6 {
            let tabs = l.pages[0].placed_lines[0].2.runs.iter().filter(|r| r.text == "\t").count();
            assert_eq!(tabs, 5); // preceding edge tab must remain on line one.
        }
    }
}

#[test]
fn explicit_settings_paragraph_measurement_agrees_with_document_layout() {
    struct MissingFont;
    impl zen_word::layout::FontLineHeightProvider for MissingFont {
        fn lookup(&self, _: &RunProps) -> Option<zen_word::layout::FontLineHeight> { None }
    }
    let mut d = tab_doc(720, "A", 5);
    d.section.margin_left_twips = 1440;
    d.section.margin_right_twips = 1440;
    d.section.page_width_twips = 6480;
    let Block::Paragraph(p) = &d.blocks[0] else { unreachable!() };
    let (flow, _) = LineServices::format_paragraph_flow_with_settings_and_metrics(p, 3600, &d.settings, &MissingFont).unwrap();
    let local: Vec<_> = flow.into_iter().flat_map(|f| match f { zen_word::layout::ParagraphFlow::Lines(l) => l, _ => Vec::new() }).collect();
    let (layout, _) = PageTableServices::layout_document_with_metrics_checked(&d, &MissingFont).unwrap();
    let placed = &layout.pages[0].placed_lines;
    assert_eq!(local.len(), placed.len());
    for (a, (_, _, b)) in local.iter().zip(placed) {
        assert_eq!(a.height_twips, b.height_twips);
        assert_eq!(a.runs.iter().map(|r| (&r.text, r.x_twips)).collect::<Vec<_>>(), b.runs.iter().map(|r| (&r.text, r.x_twips)).collect::<Vec<_>>());
    }
    for interval in [0, 32768, u32::MAX] {
        let settings = DocumentSettings { default_tab_stop_twips: interval };
        assert!(matches!(LineServices::format_paragraph_flow_with_settings_and_metrics(p, 3600, &settings, &MissingFont), Err(WordError::UnsupportedEdit(_))));
    }
}
