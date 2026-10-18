//! Small production entry point for the supported package-preserving operations.
use std::io::Read;
use std::{collections::BTreeMap, error::Error, path::PathBuf};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
fn main() {
    if let Err(error) = run() {
        eprintln!("Zen delivery rejected: {error}");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let command = args
        .next()
        .ok_or("commands: word-structure, word-text, word-paper-a4, slide-text, sheet-number")?;
    let mut flags = BTreeMap::new();
    while let Some(key) = args.next() {
        if ![
            "--input",
            "--output",
            "--find",
            "--replace",
            "--sheet",
            "--cell",
            "--value",
            "--tier",
        ]
        .contains(&key.as_str())
        {
            return Err(format!("unknown option {key}").into());
        }
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {key}"))?;
        if flags.insert(key.clone(), value).is_some() {
            return Err(format!("duplicate option {key}").into());
        }
    }
    let get = |name: &str| {
        flags
            .get(name)
            .map(String::as_str)
            .ok_or_else(|| format!("required option {name}"))
    };
    let input = PathBuf::from(get("--input")?);
    const MAX_INPUT_BYTES: u64 = 64 * 1024 * 1024;
    let file = std::fs::File::open(&input)?;
    if file.metadata()?.len() > MAX_INPUT_BYTES {
        return Err("input exceeds the 64 MiB delivery archive limit".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_INPUT_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_INPUT_BYTES {
        return Err("input grew beyond the delivery archive limit".into());
    }
    if command == "word-structure" {
        if flags.keys().any(|key| key != "--input") {
            return Err("word-structure accepts only --input and prints JSON".into());
        }
        if !input
            .extension()
            .and_then(|x| x.to_str())
            .is_some_and(|x| x.eq_ignore_ascii_case("docx"))
        {
            return Err("word-structure requires .docx input".into());
        }
        let outcome = zen_word::read_docx(&bytes)?;
        let sections: Vec<_> = outcome.document.sections().iter().map(|section| {
            let p = section.properties;
            serde_json::json!({
                "index": section.index, "block_start": section.blocks.start, "block_end_exclusive": section.blocks.end,
                "width_twips": p.page_width_twips, "height_twips": p.page_height_twips,
                "orientation": p.orientation.as_ooxml(), "start": p.start.as_ooxml(),
                "page_number_start": p.page_number_start, "page_number_format": p.page_number_format,
                "margins_twips": {"left": p.margin_left_twips, "right": p.margin_right_twips, "top": p.margin_top_twips, "bottom": p.margin_bottom_twips},
                "columns": p.column_count, "gutter_twips": p.gutter_twips,
                "header_distance_twips": p.header_distance_twips, "footer_distance_twips": p.footer_distance_twips
            })
        }).collect();
        let diagnostics: Vec<_> = outcome
            .diagnostics
            .iter()
            .map(|d| {
                serde_json::json!({
                    "part": d.part, "kind": format!("{:?}", d.kind), "detail": d.detail
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "result": "READ_ONLY_STRUCTURE", "scope": "semantic projection; not an Office pagination or compatibility verdict",
                "section_count": sections.len(), "sections": sections, "diagnostics": diagnostics
            }))?
        );
        return Ok(());
    }
    let output = PathBuf::from(get("--output")?);
    let expected_extension = match command.as_str() {
        "word-text" | "word-paper-a4" => "docx",
        "slide-text" => "pptx",
        "sheet-number" => "xlsx",
        _ => return Err("unknown command".into()),
    };
    if input
        .extension()
        .and_then(|s| s.to_str())
        .map(str::to_lowercase)
        .as_deref()
        != Some(expected_extension)
        || output
            .extension()
            .and_then(|s| s.to_str())
            .map(str::to_lowercase)
            .as_deref()
            != Some(expected_extension)
    {
        return Err(format!("command requires .{expected_extension} input and output").into());
    }
    let result = match command.as_str() {
        "word-paper-a4" => {
            if flags
                .keys()
                .any(|k| !["--input", "--output"].contains(&k.as_str()))
            {
                return Err("option is not applicable to word-paper-a4".into());
            }
            let doc = zen_word::read_docx(&bytes)?.document;
            doc.write_paper_a4_portrait()?
        }
        "word-text" => {
            if flags
                .keys()
                .any(|k| !["--input", "--output", "--find", "--replace"].contains(&k.as_str()))
            {
                return Err("option is not applicable to word-text".into());
            }
            let mut doc = zen_word::read_docx(&bytes)?.document;
            if doc.replace_text_preserving(get("--find")?, get("--replace")?)? == 0 {
                return Err("search text not found within an editable run".into());
            }
            zen_word::write_docx(&doc)?
        }
        "slide-text" => {
            if flags
                .keys()
                .any(|k| !["--input", "--output", "--find", "--replace"].contains(&k.as_str()))
            {
                return Err("option is not applicable to slide-text".into());
            }
            let mut pres = zen_slide::read_pptx_bytes(&bytes)?;
            if pres.replace_text_preserving(get("--find")?, get("--replace")?)? == 0 {
                return Err("search text not found within an editable run".into());
            }
            zen_slide::write_pptx(&pres)?
        }
        "sheet-number" => {
            if flags.keys().any(|k| {
                ![
                    "--input", "--output", "--sheet", "--cell", "--value", "--tier",
                ]
                .contains(&k.as_str())
            }) {
                return Err("option is not applicable to sheet-number".into());
            }
            let limits = match flags.get("--tier").map(String::as_str).unwrap_or("preview") {
                "preview" => zen_formats::spreadsheet::WorkbookReadLimits::default(),
                "office-standard" => {
                    zen_formats::spreadsheet::WorkbookReadLimits::office_standard()
                }
                _ => return Err("tier must be preview or office-standard".into()),
            };
            let a1 = get("--cell")?;
            if a1.len() > 10 || !a1.is_ascii() {
                return Err("invalid A1 cell address length".into());
            }
            let addr = zen_engine::calc::CellCoord::from_a1(a1).ok_or("invalid A1 cell address")?;
            if addr.row >= 1_048_576 || addr.col >= 16_384 {
                return Err("cell exceeds XLSX bounds".into());
            }
            let address = zen_engine::workbook::CellAddress::new(addr.row as u32, addr.col as u16)?;
            let value = get("--value")?.parse::<f64>()?;
            let read = zen_formats::spreadsheet::ReadWorkbook::read(&bytes, &limits)?;
            read.write_number_edit(get("--sheet")?, address, value)?
        }
        _ => unreachable!(),
    };
    // Export/validation completes before touching the destination. Locked or
    // failed replacements preserve the previous destination through save_atomic.
    zen_formats::save_atomic(&output, &result)?;
    println!(
        "{}",
        serde_json::json!({"operation":command,"output":output,"bytes":result.len(),"result":"SAVED_SUPPORTED_OPERATION"})
    );
    Ok(())
}
