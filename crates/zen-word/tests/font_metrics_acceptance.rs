//! Bounded opt-in line-height API; no claim of document or baseline parity.
use zen_word::*;
use zen_word::layout::{FontLineHeight, FontLineHeightProvider};

struct Controlled;
impl FontLineHeightProvider for Controlled {
    fn lookup(&self, p: &RunProps) -> Option<FontLineHeight> {
        if p.bold==Some(true) || p.italic==Some(true) { return None; }
        let (units,source)=match p.font_ascii.as_deref() {
            Some("Times New Roman")=>(2355,"cycle10:times.ttf:hhea:fbb57cdb0079137adc0e478913ca134dfee02aa2ef443738ec5e839bf97a1f7f"),
            Some("SyntheticTall")=>(4096,"synthetic:2em"),
            Some("SyntheticTiny")=>(1,"synthetic:tiny"),
            _=>return None,
        };
        Some(FontLineHeight::new(2048,units,source.into()).unwrap())
    }
}
fn paragraph(hp:u32, font:&str)->Paragraph {
    Paragraph { props:Default::default(), runs:vec![Run { props:RunProps {
        font_ascii:Some(font.into()),size_half_points:Some(hp),..Default::default()
    },contents:vec![RunContent::Text("Ag".into())] }] }
}
fn height(p:&Paragraph)->u32 { LineServices::format_paragraph_with_metrics(p,10000,&Controlled).0[0].height_twips }

#[test]
fn controlled_sizes_and_spacing_match_frozen_native_spans() {
    // Six identical short lines, five steps, from cycle09 native/line_metrics.json.
    // This compares NEW measurement output to OLD native inputs, not new COM.
    for (hp,single_span,double_span) in [(20,57.5,115.0),(22,63.0,126.5),(24,69.0,138.0),(28,80.5,161.0),(32,92.0,184.0)] {
        for (auto,expected) in [(240,single_span),(480,double_span)] {
            let mut p=paragraph(hp,"Times New Roman");p.props.line_spacing=Some(LineSpacing::Auto(auto));
            let actual=f64::from(height(&p))*5.0/20.0;
            assert!((actual-expected).abs()<=1.0,"hp={hp},auto={auto}: {actual} vs {expected}");
        }
    }
}
#[test]
fn default_api_keeps_heuristic_height() {
    let p=paragraph(24,"Times New Roman");assert_eq!(LineServices::format_paragraph(&p,10000)[0].height_twips,288);
    assert_eq!(height(&p),276);
}
#[test]
fn missing_and_unsupported_variant_fallback_is_traced() {
    for mut p in [paragraph(24,"Missing"),paragraph(24,"Times New Roman")] {
        if p.runs[0].props.font_ascii.as_deref()==Some("Times New Roman") {p.runs[0].props.bold=Some(true);}
        let (lines,trace)=LineServices::format_paragraph_with_metrics(&p,10000,&Controlled);
        assert_eq!(lines[0].height_twips,288);assert_eq!(trace.len(),1);assert!(trace[0].source.is_none());
    }
}
#[test]
fn source_and_input_preserved() {
    let p=paragraph(24,"Times New Roman");let original=p.clone();
    let (lines,trace)=LineServices::format_paragraph_with_metrics(&p,10000,&Controlled);
    assert_eq!(p,original);assert_eq!(lines[0].runs[0].text,"Ag");
    assert!(trace[0].source.as_ref().unwrap().contains("fbb57cdb"));assert_eq!(trace[0].natural_height_twips,276);
}
#[test]
fn empty_runs_use_metrics_and_default_missing_font_is_traced() {
    let mut p=paragraph(24,"Times New Roman");p.runs[0].contents.clear();assert_eq!(height(&p),276);
    p.runs.clear();let (lines,trace)=LineServices::format_paragraph_with_metrics(&p,10000,&Controlled);
    assert_eq!(lines[0].height_twips,252);assert!(trace[0].source.is_none());
}
#[test]
fn mixed_run_uses_largest_actual_height() {
    let mut p=paragraph(24,"Times New Roman");p.runs.push(paragraph(20,"SyntheticTall").runs.remove(0));
    assert_eq!(height(&p),400);assert_eq!(LineServices::format_paragraph(&p,10000)[0].height_twips,288);
}
#[test]
fn spacing_policies_still_apply() {
    let mut p=paragraph(24,"Times New Roman");
    for (spacing,expected) in [(LineSpacing::Exact(360),360),(LineSpacing::AtLeast(300),300),(LineSpacing::AtLeast(200),276),(LineSpacing::Auto(480),552)] {
        p.props.line_spacing=Some(spacing);assert_eq!(height(&p),expected);
    }
}
#[test]
fn invalid_metrics_rejected_and_extreme_scaling_bounded() {
    for (em,units,source) in [(0,2355,"x"),(15,2355,"x"),(16385,2355,"x"),(2048,0,"x"),(2048,u32::MAX,"x"),(2048,2355," ")] {
        assert!(FontLineHeight::new(em,units,source.into()).is_err());
    }
    let m=FontLineHeight::new(16,196605,"synthetic".into()).unwrap();
    assert_eq!(m.scale_twips(u32::MAX),u32::MAX);assert_eq!(m.scale_twips(0),1);
}
#[test]
fn zero_width_and_small_height_safe() {
    let p=paragraph(24,"SyntheticTiny");let (l,t)=LineServices::format_paragraph_with_metrics(&p,0,&Controlled);assert!(l.is_empty()&&t.is_empty());
    let (l,_)=LineServices::format_paragraph_with_metrics(&p,10000,&Controlled);
    assert_eq!(l[0].height_twips,1);assert!(l[0].ascent_twips<=l[0].height_twips);assert_eq!(l[0].ascent_twips+l[0].descent_twips,1);
}
