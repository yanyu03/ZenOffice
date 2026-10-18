#![cfg(windows)]
use std::{fs, io::Write, process::{Command, Stdio}};
use zen_engine::workbook::CellAddress;
use zen_formats::spreadsheet::{ReadWorkbook, WorkbookReadLimits};

fn fixture() -> &'static [u8] {
    include_bytes!("../../../RCD/red-team-corpus/office-targets-2026-10/xlsx/zen_spreadsheet_multiscenario.xlsx")
}
fn run(source: &std::path::Path, commands: &str) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_zen-session")).arg(source)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(commands.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
}
#[test]
fn one_process_edits_undoes_redoes_saves_and_reopens() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source with spaces.xlsx");
    let output = dir.path().join("saved with spaces.xlsx");
    fs::write(&source, fixture()).unwrap();
    let result = run(&source, &format!("set\tTransactions\t3\t3\t42\nset\tTransactions\t3\t3\t43\nundo\nget\tTransactions\t3\t3\nredo\nsave\t{}\nundo\nredo\nreopen\t{}\nget\tTransactions\t3\t3\nquit\n", output.display(), output.display()));
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let text = String::from_utf8(result.stdout).unwrap();
    assert!(text.contains("VALUE Some(\"42\")"));
    assert!(text.contains("VALUE Some(\"43\")"));
    assert!(text.contains("OK save revision=4 dirty=false undo=true redo=false"));
    assert!(text.contains("OK undo revision=5 dirty=true"));
    assert!(text.contains("OK redo revision=6 dirty=false"));
    assert!(text.contains("OK reopen revision=0 dirty=false undo=false redo=false"));
    assert_eq!(fs::read(&source).unwrap(), fixture());
    let saved = fs::read(output).unwrap();
    let reopened = ReadWorkbook::read(&saved, &WorkbookReadLimits::default()).unwrap();
    let cell = reopened.workbook().sheets().iter().find(|s| s.name()=="Transactions").unwrap()
        .cell(CellAddress::new(3,3).unwrap()).unwrap();
    assert_eq!(cell.value.as_ref().unwrap().text(), "43");
}
#[test]
fn rejected_commands_preserve_work_and_errors_are_nonzero() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.xlsx");
    let output = dir.path().join("saved.xlsx");
    let old = dir.path().join("existing.xlsx");
    fs::write(&source, fixture()).unwrap(); fs::write(&old, b"old output").unwrap();
    let result = run(&source, &format!("set\tTransactions\t3\t3\t42\nreopen\t{}\nset\tTransactions\t3\t3\tNaN\nset\tTransactions\t0\t3\t9\nsave\t{}\nstatus\nget\tTransactions\t3\t3\nsave\t{}\nreopen\t{}\nstatus\nquit\n", source.display(), old.display(), output.display(), dir.path().join("missing.xlsx").display()));
    assert_eq!(result.status.code(), Some(2));
    let text = String::from_utf8(result.stdout).unwrap();
    assert!(text.contains("OK status revision=1 dirty=true undo=true redo=false"));
    assert!(text.contains("VALUE Some(\"42\")"));
    assert!(text.contains("OK status revision=1 dirty=false undo=true redo=false"));
    assert_eq!(fs::read(old).unwrap(), b"old output");
    assert_eq!(fs::read(source).unwrap(), fixture());
    assert!(output.exists());
}
#[test]
fn dirty_eof_and_oversized_command_fail_without_writing_source() {
    let dir = tempfile::tempdir().unwrap(); let source = dir.path().join("source.xlsx");
    fs::write(&source, fixture()).unwrap();
    assert_eq!(run(&source, "set\tTransactions\t3\t3\t42\n").status.code(), Some(2));
    let oversized = run(&source, &"x".repeat(16 * 1024 + 1));
    assert_eq!(oversized.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&oversized.stderr).contains("exceeds 16 KiB"));
    assert_eq!(fs::read(source).unwrap(), fixture());
}
