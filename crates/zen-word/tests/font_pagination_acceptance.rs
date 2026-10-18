//! Controlled opt-in pagination; no native host acceptance claim.
use std::cell::Cell;
use zen_word::*;
use zen_word::layout::{FontLineHeight, FontLineHeightProvider, ParagraphFlow};
struct Controlled { calls: Cell<usize> }
impl Controlled { fn new()->Self {Self {calls:Cell::new(0)}} }
impl FontLineHeightProvider for Controlled {
    fn lookup(&self,p:&RunProps)->Option<FontLineHeight> {
        self.calls.set(self.calls.get()+1);
        if p.font_ascii.as_deref()!=Some("Times New Roman") || p.bold==Some(true) || p.italic==Some(true) {return None;}
        Some(FontLineHeight::new(2048,2355,"controlled:cycle10:tnr:hhea".into()).unwrap())
    }
}
fn doc(body:i32)->Document {
    let mut d=Document::default();d.section=SectionProps {page_width_twips:4000,page_height_twips:body+200,margin_left_twips:200,margin_right_twips:200,margin_top_twips:100,margin_bottom_twips:100,..Default::default()};
    d.styles.default_run=RunProps {font_ascii:Some("Times New Roman".into()),size_half_points:Some(24),..Default::default()};
    d.styles.default_para.line_spacing=Some(LineSpacing::Auto(240));d
}
fn p(text:&str,keep:bool)->Block {
    let mut p=Paragraph::text(text);p.props.keep_next=keep;Block::Paragraph(p)
}
fn labels(l:&LayoutDocument)->Vec<Vec<String>> {l.pages.iter().map(|p|p.placed_lines.iter().map(|(_,_,l)|l.runs.iter().map(|r|r.text.as_str()).collect()).collect()).collect()}
fn bounded(l:&LayoutDocument) {for p in &l.pages {for (_,y,line) in &p.placed_lines {assert!(*y>=p.margin_top_twips);assert!(i64::from(*y)+i64::from(line.height_twips)<=i64::from(p.height_twips-p.margin_bottom_twips));}}}
#[test]
fn hard_line_page_and_trailing_empty_use_same_metrics() {
    let d=doc(840);let mut p=Paragraph::text("A");
    p.runs[0].contents=vec![RunContent::Text("A".into()),RunContent::Break(BreakKind::Line),RunContent::Text("B".into()),RunContent::Break(BreakKind::Page),RunContent::Text("C".into()),RunContent::Break(BreakKind::Line)];
    let effective=StyleResolver::new(&d.styles).unwrap().resolve_paragraph(&p).unwrap().paragraph;
    let (flow,traces)=LineServices::format_paragraph_flow_with_metrics(&effective,3600,&Controlled::new());
    assert_eq!(flow.iter().filter(|f|matches!(f,ParagraphFlow::Break(BreakKind::Page))).count(),1);
    let lines:Vec<_>=flow.iter().filter_map(|f|if let ParagraphFlow::Lines(l)=f {Some(l)} else {None}).flatten().collect();
    assert_eq!(lines.len(),4);assert!(lines.iter().all(|l|l.height_twips==276));assert_eq!(traces.len(),4);
    assert!(traces.iter().all(|t|t.source.is_some()));
}
#[test]
fn body_and_keep_prediction_share_measurement_once() {
    let mut d=doc(840);d.blocks=vec![p("Fill",false),p("Title",true),p("Body",false)];let before=d.clone();let provider=Controlled::new();
    let (l,t)=PageTableServices::layout_document_with_metrics_checked(&d,&provider).unwrap();
    assert_eq!(labels(&l),vec![vec!["Fill","Title","Body"]]);assert_eq!(provider.calls.get(),3);assert_eq!(t.len(),3);bounded(&l);assert_eq!(d,before);
    assert_eq!(labels(&PageTableServices::layout_document_checked(&d).unwrap()),vec![vec!["Fill"],vec!["Title","Body"]]);
}
#[test]
fn hard_break_and_page_break_before_win_over_keep() {
    for hard in [true,false] {
        let mut d=doc(840);d.blocks=vec![p("Title",true),p("Body",false)];
        if hard {let Block::Paragraph(p)=&mut d.blocks[0] else {unreachable!()};p.runs[0].contents.push(RunContent::Break(BreakKind::Page));}
        else {let Block::Paragraph(p)=&mut d.blocks[1] else {unreachable!()};p.props.page_break_before=true;}
        let (l,_)=PageTableServices::layout_document_with_metrics_checked(&d,&Controlled::new()).unwrap();
        assert_eq!(labels(&l),vec![vec!["Title"],vec!["Body"]]);bounded(&l);
    }
}
#[test]
fn body_and_cell_lines_share_resolved_font_and_height() {
    let mut d=doc(2000);let mut cell=TableCell::default();let mut para=Paragraph::text("A");
    para.runs[0].contents.extend([RunContent::Break(BreakKind::Line),RunContent::Text("B".into())]);cell.blocks.push(Block::Paragraph(para));
    let mut table=Table::new(vec![3000]);table.rows.push(TableRow {cells:vec![cell]});d.blocks=vec![p("Body",false),Block::Table(table)];let original=d.clone();
    let (l,t)=PageTableServices::layout_document_with_metrics_checked(&d,&Controlled::new()).unwrap();
    assert_eq!(l.pages[0].placed_lines[0].2.height_twips,276);let c=&l.pages[0].placed_cells[0];assert_eq!(c.height_twips,752);assert_eq!(c.lines.len(),2);assert!(c.lines.iter().all(|(_,_,l)|l.height_twips==276));assert_eq!(t.len(),3);assert!(t.iter().all(|x|x.source.is_some()));assert_eq!(d,original);
}
#[test]
fn missing_body_and_cell_fonts_are_traced() {
    let mut d=doc(2000);d.styles.default_run.font_ascii=Some("Missing".into());let mut table=Table::new(vec![3000]);table.rows.push(TableRow::from_texts(["Cell"]));d.blocks=vec![p("Body",false),Block::Table(table)];
    let (l,t)=PageTableServices::layout_document_with_metrics_checked(&d,&Controlled::new()).unwrap();assert_eq!(t.len(),2);assert!(t.iter().all(|x|x.source.is_none()));assert_eq!(l.pages[0].placed_lines[0].2.height_twips,288);assert_eq!(l.pages[0].placed_cells[0].lines[0].2.height_twips,288);
}
#[test]
fn unsupported_cell_break_and_invalid_sections_still_reject_without_mutation() {
    let mut d=doc(2000);let mut table=Table::new(vec![3000]);let mut p=Paragraph::text("Cell");p.runs[0].contents.push(RunContent::Break(BreakKind::Page));table.rows.push(TableRow {cells:vec![TableCell {blocks:vec![Block::Paragraph(p)],..Default::default()}]});d.blocks.push(Block::Table(table));let original=d.clone();
    assert!(PageTableServices::layout_document_with_metrics_checked(&d,&Controlled::new()).is_err());assert_eq!(d,original);
    d.section.column_count=2;let provider=Controlled::new();assert!(PageTableServices::layout_document_with_metrics_checked(&d,&provider).is_err());assert_eq!(provider.calls.get(),0);
}
#[test]
fn oversized_row_remains_rejected() {
    let mut d=doc(400);let mut table=Table::new(vec![3000]);table.rows.push(TableRow::from_texts(["Tall"]));d.blocks.push(Block::Table(table));let original=d.clone();
    assert!(PageTableServices::layout_document_with_metrics_checked(&d,&Controlled::new()).is_err());assert_eq!(d,original);
}
#[test]
fn exact_spacing_keeps_original_pagination() {
    let mut d=doc(840);d.styles.default_para.line_spacing=Some(LineSpacing::Exact(250));d.blocks=vec![p("A",false),p("B",true),p("C",false)];
    let old=PageTableServices::layout_document_checked(&d).unwrap();let (new,t)=PageTableServices::layout_document_with_metrics_checked(&d,&Controlled::new()).unwrap();assert_eq!(labels(&new),labels(&old));assert!(new.pages.iter().flat_map(|p|&p.placed_lines).all(|(_,_,l)|l.height_twips==250));assert_eq!(t.len(),3);
}
