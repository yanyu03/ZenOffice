//! Bounded tab-delimited session driver; all editing/persistence is delegated.
use std::{io::{self, BufRead, Read, Write}, path::Path, process::ExitCode};
use zen_engine::workbook::CellAddress;
use zen_formats::spreadsheet::WorkbookReadLimits;
use zen_session::{HistoryBudget, WorkbookSession};

const MAX_LINE: u64 = 16 * 1024;

fn address(row: &str, column: &str) -> Result<CellAddress, String> {
    let row = row.parse::<u32>().map_err(|_| "invalid row")?.checked_sub(1).ok_or("row starts at 1")?;
    let column = column.parse::<u16>().map_err(|_| "invalid column")?.checked_sub(1).ok_or("column starts at 1")?;
    CellAddress::new(row, column)
        .map_err(|e| e.to_string())
}

fn command(session: &mut WorkbookSession, fields: &[&str]) -> Result<bool, String> {
    match fields {
        ["status"] => (),
        ["set", sheet, row, column, value] => {
            let number = value.parse::<f64>().map_err(|_| "invalid number")?;
            session.set_number(sheet, address(row, column)?, number).map_err(|e| e.to_string())?;
        }
        ["get", sheet, row, column] => {
            let address = address(row, column)?;
            let sheet = session.workbook().sheets().iter().find(|s| s.name().eq_ignore_ascii_case(sheet))
                .ok_or("unknown sheet")?;
            let value = sheet.cell(address).and_then(|c| c.value.as_ref()).map(|v| v.text());
            println!("VALUE {value:?}");
        }
        ["undo"] => { session.undo().map_err(|e| e.to_string())?; }
        ["redo"] => { session.redo().map_err(|e| e.to_string())?; }
        ["save", path] => { session.save_as_new(Path::new(path)).map_err(|e| e.to_string())?; }
        ["reopen", path] => {
            if session.is_dirty() { return Err("save before reopening; current work is dirty".into()); }
            let next = WorkbookSession::open(Path::new(path), &WorkbookReadLimits::default(), HistoryBudget::default())
                .map_err(|e| e.to_string())?;
            *session = next;
        }
        ["quit"] => {
            if session.is_dirty() { return Err("save before quitting; current work is dirty".into()); }
            return Ok(true);
        }
        _ => return Err("invalid command or field count; fields must be tab separated".into()),
    }
    Ok(false)
}

fn run() -> Result<bool, String> {
    let mut args = std::env::args_os().skip(1);
    let source = args.next().ok_or("usage: zen-session <source.xlsx> (commands on stdin)")?;
    if args.next().is_some() { return Err("expected one source path".into()); }
    let mut session = WorkbookSession::open(Path::new(&source), &WorkbookReadLimits::default(), HistoryBudget::default())
        .map_err(|e| e.to_string())?;
    let mut input = io::stdin().lock();
    let mut failed = false;
    loop {
        let mut bytes = Vec::new();
        Read::by_ref(&mut input).take(MAX_LINE + 1).read_until(b'\n', &mut bytes).map_err(|e| e.to_string())?;
        if bytes.is_empty() { return Ok(failed || session.is_dirty()); }
        if bytes.len() as u64 > MAX_LINE { return Err("command exceeds 16 KiB; session stopped".into()); }
        let line = std::str::from_utf8(&bytes).map_err(|_| "commands must be UTF-8")?;
        let fields: Vec<_> = line.trim_end_matches(['\r', '\n']).split('\t').collect();
        match command(&mut session, &fields) {
            Ok(quit) => {
                println!("OK {} revision={} dirty={} undo={} redo={}", fields[0], session.revision(),
                    session.is_dirty(), session.can_undo(), session.can_redo());
                io::stdout().flush().map_err(|e| e.to_string())?;
                if quit { return Ok(failed); }
            }
            Err(error) => { eprintln!("ERR {error}"); failed = true; }
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(false) => ExitCode::SUCCESS,
        Ok(true) => { eprintln!("ERR command refused or unsaved work at EOF"); ExitCode::from(2) }
        Err(error) => { eprintln!("ERR {error}"); ExitCode::from(2) }
    }
}
