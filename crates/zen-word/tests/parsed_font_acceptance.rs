use zen_word::*;
use zen_word::layout::{FontLineHeightPolicy,FontLineHeightProvider,ParsedFontLineHeight};
fn u16at(b:&mut [u8],o:usize,v:u16){b[o..o+2].copy_from_slice(&v.to_be_bytes());}
fn u32at(b:&mut [u8],o:usize,v:u32){b[o..o+4].copy_from_slice(&v.to_be_bytes());}
fn font(flags:u16)->Vec<u8> {
    let mut head=vec![0;54];u32at(&mut head,0,0x10000);u32at(&mut head,12,0x5f0f3cf5);u16at(&mut head,18,2048);
    let mut hhea=vec![0;36];u32at(&mut hhea,0,0x10000);u16at(&mut hhea,4,1825);u16at(&mut hhea,6,(-443i16) as u16);u16at(&mut hhea,8,87);u16at(&mut hhea,34,1);
    let mut maxp=vec![0;6];u32at(&mut maxp,0,0x5000);u16at(&mut maxp,4,1);
    let mut os2=vec![0;78];u16at(&mut os2,62,flags);
    let namebytes:Vec<_>="Example Family".encode_utf16().flat_map(u16::to_be_bytes).collect();
    let mut name=vec![0;18];u16at(&mut name,2,1);u16at(&mut name,4,18);u16at(&mut name,6,3);u16at(&mut name,8,1);u16at(&mut name,10,0x409);u16at(&mut name,12,1);u16at(&mut name,14,namebytes.len() as u16);name.extend(namebytes);
    let tables=[(*b"head",head),(*b"hhea",hhea),(*b"maxp",maxp),(*b"OS/2",os2),(*b"name",name)];
    let mut out=vec![0;12+tables.len()*16];u32at(&mut out,0,0x10000);u16at(&mut out,4,tables.len() as u16);
    for (i,(tag,b)) in tables.iter().enumerate(){let off=12+i*16;out[off..off+4].copy_from_slice(tag);let start=out.len();u32at(&mut out,off+8,start as u32);u32at(&mut out,off+12,b.len() as u32);out.extend(b);}
    out
}
fn load(b:&[u8])->std::result::Result<ParsedFontLineHeight,&'static str>{ParsedFontLineHeight::from_bytes(b,0,"Example Family",FontLineHeightPolicy::Hhea,"synthetic")}
fn props()->RunProps {RunProps {font_ascii:Some("Example Family".into()),size_half_points:Some(24),..Default::default()}}
#[test]fn parses_real_tables_not_a_family_constant(){let f=load(&font(64)).unwrap();assert_eq!(f.lookup(&props()).unwrap().scale_twips(24),276);}
#[test]fn rejects_family_mismatch(){assert!(ParsedFontLineHeight::from_bytes(&font(64),0,"Wrong",FontLineHeightPolicy::Hhea,"s").is_err());}
#[test]fn rejects_empty_identity(){assert!(ParsedFontLineHeight::from_bytes(&font(64),0,"Example Family",FontLineHeightPolicy::Hhea,"").is_err());}
#[test]fn rejects_standalone_index(){assert!(ParsedFontLineHeight::from_bytes(&font(64),1,"Example Family",FontLineHeightPolicy::Hhea,"s").is_err());}
#[test]fn truncated_inputs_do_not_panic(){let b=font(64);for n in 0..12 {assert!(load(&b[..n]).is_err());}assert!(load(b"not a font").is_err());}
#[test]fn missing_variant_metadata_rejected(){let mut b=font(64);b[12+3*16..16+3*16].copy_from_slice(b"JUNK");assert!(load(&b).is_err());}
#[test]fn variable_and_negative_gap_rejected(){let mut b=font(64);b[12+4*16..16+4*16].copy_from_slice(b"fvar");assert!(load(&b).is_err());let mut b=font(64);let off=u32::from_be_bytes(b[12+16+8..12+16+12].try_into().unwrap()) as usize;u16at(&mut b,off+8,(-1i16) as u16);assert!(load(&b).is_err());}
#[test]fn exact_variant_match(){for (flags,bold,italic) in [(64,false,false),(32,true,false),(1,false,true),(33,true,true)] {let f=load(&font(flags)).unwrap();let mut p=props();p.bold=Some(bold);p.italic=Some(italic);assert!(f.lookup(&p).is_some());p.bold=Some(!bold);assert!(f.lookup(&p).is_none());}}
#[test]fn unknown_family_and_conflicting_east_asia_fallback(){let f=load(&font(64)).unwrap();let mut p=props();p.font_ascii=Some("unknown".into());assert!(f.lookup(&p).is_none());p=props();p.font_east_asia=Some("Other".into());assert!(f.lookup(&p).is_none());}
#[test]fn parsed_provider_runs_through_checked_pagination(){let f=load(&font(64)).unwrap();let mut d=Document::default();d.styles.default_run=props();d.blocks.push(Block::paragraph("Ag"));let original=d.clone();let (l,t)=PageTableServices::layout_document_with_metrics_checked(&d,&f).unwrap();assert_eq!(l.pages[0].placed_lines[0].2.height_twips,276);assert!(t[0].source.as_ref().unwrap().contains("synthetic:face=0:hhea"));assert_eq!(d,original);}

#[test]fn mixed_slots_ascii_and_empty_opt_in_keep_strict_lookup() {
    let f=load(&font(64)).unwrap();let mut p=props();p.font_east_asia=Some("Other".into());let original=p.clone();
    assert!(f.lookup(&p).is_none());
    for text in ["","Ag 09"," ","\t"] {assert_eq!(f.lookup_for_text(&p,text).unwrap().scale_twips(24),276);}
    assert_eq!(p,original);p.bold=Some(true);assert!(f.lookup_for_text(&p,"Ag").is_none());
}
#[test]fn mixed_slots_unknown_scripts_and_controls_stay_conservative() {
    let f=load(&font(64)).unwrap();let mut p=props();p.font_east_asia=Some("Other".into());
    for text in ["中文","é","Ж","🙂","Ag中","\0","\n"] {assert!(f.lookup_for_text(&p,text).is_none(),"{text:?}");}
}
#[test]fn contextual_height_reaches_empty_and_visible_paragraphs() {
    let f=load(&font(64)).unwrap();let mut d=Document::default();d.styles.default_run=props();d.styles.default_run.font_east_asia=Some("Other".into());d.blocks=vec![Block::paragraph("Ag"),Block::Paragraph(Paragraph::default()),Block::paragraph("中文")];let original=d.clone();
    let (l,t)=PageTableServices::layout_document_with_metrics_checked(&d,&f).unwrap();let h:Vec<_>=l.pages[0].placed_lines.iter().map(|(_,_,l)|l.height_twips).collect();assert_eq!(h,vec![276,276,288]);assert!(t[0].source.is_some()&&t[1].source.is_some());assert!(t.iter().skip(2).all(|x|x.source.is_none()));assert_eq!(d,original);
}

#[test]
#[ignore="Requires explicit local font directory and identity manifest; run separately"]
fn installed_times_variants() {
    let root=std::path::PathBuf::from(std::env::var("ZEN_AUDIT_FONT_DIR").expect("explicit font directory"));
    let identities=std::env::var("ZEN_AUDIT_FONT_IDENTITIES").expect("externally verified SHA256 identities");
    for (file,bold,italic) in [("times.ttf",false,false),("timesbd.ttf",true,false),("timesi.ttf",false,true),("timesbi.ttf",true,true)] {
        let identity=identities.lines().find(|l|l.starts_with(&format!("{file}="))).expect("font identity");
        let bytes=std::fs::read(root.join(file)).unwrap();let original=bytes.clone();
        let f=ParsedFontLineHeight::from_bytes(&bytes,0,"Times New Roman",FontLineHeightPolicy::Hhea,identity).unwrap();
        let mut p=props();p.font_ascii=Some("Times New Roman".into());p.bold=Some(bold);p.italic=Some(italic);let metric=f.lookup(&p).unwrap();assert!(metric.scale_twips(24)>0);p.italic=Some(!italic);assert!(f.lookup(&p).is_none());assert_eq!(bytes,original);
        println!("{identity}\tbold={bold}\titalic={italic}\t12pt_height_twips={}",metric.scale_twips(24));
    }
}
