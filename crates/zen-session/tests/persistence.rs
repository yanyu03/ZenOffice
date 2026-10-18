#![cfg(windows)]
use std::{fs, fs::OpenOptions, os::windows::fs::OpenOptionsExt, path::Path};
use tempfile::TempDir;
use zen_engine::workbook::CellAddress;
use zen_formats::spreadsheet::{ReadWorkbook, WorkbookReadLimits};
use zen_session::{HistoryBudget, SaveError, SessionError, WorkbookSession};

fn fixture() -> &'static [u8] {
    include_bytes!("../../../RCD/red-team-corpus/office-targets-2026-10/xlsx/zen_spreadsheet_multiscenario.xlsx")
}
fn prepare() -> (TempDir, std::path::PathBuf, WorkbookSession) {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.xlsx");
    fs::write(&source, fixture()).unwrap();
    let session = WorkbookSession::open(&source, &WorkbookReadLimits::default(), HistoryBudget::default()).unwrap();
    (directory, source, session)
}
fn edit(session: &mut WorkbookSession, number: f64) {
    session.set_number("Transactions", CellAddress::new(3, 3).unwrap(), number).unwrap();
}
fn state(s: &WorkbookSession) -> (Vec<u8>, u64, usize, usize, bool, bool, bool) {
    (s.bytes().to_vec(), s.revision(), s.history_frames(), s.retained_archive_bytes(),
     s.is_dirty(), s.can_undo(), s.can_redo())
}

#[test]
fn create_new_save_reopens_and_preserves_source_revision_and_history() {
    let (directory, source, mut s) = prepare();
    edit(&mut s,42.0); edit(&mut s,43.0);
    let before = state(&s);
    let output = directory.path().join("new.xlsx");
    s.save_as_new(&output).unwrap();
    assert!(!s.is_dirty()); assert_eq!(s.revision(),before.1);
    assert_eq!(s.history_frames(),before.2); assert!(s.can_undo());
    assert_eq!(fs::read(&output).unwrap(),s.bytes());
    let reopened = ReadWorkbook::read(&fs::read(&output).unwrap(),&WorkbookReadLimits::default()).unwrap();
    let changed = reopened.workbook().sheets().iter().find(|w|w.name()=="Transactions").unwrap()
        .cell(CellAddress::new(3,3).unwrap()).unwrap();
    assert_eq!(changed.value.as_ref().unwrap().text(),"43");
    s.undo().unwrap(); assert!(s.is_dirty());
    s.redo().unwrap(); assert!(!s.is_dirty());
    assert_eq!(fs::read(source).unwrap(),fixture());
}

#[test]
fn existing_destination_same_path_and_hardlink_alias_are_never_overwritten() {
    let (directory, source, mut s) = prepare();
    edit(&mut s,42.0); s.save_as_new(&directory.path().join("saved.xlsx")).unwrap();
    edit(&mut s,43.0); edit(&mut s,44.0); s.undo().unwrap();
    let existing = directory.path().join("old.xlsx"); fs::write(&existing,b"old output").unwrap();
    let alias = directory.path().join("source-alias.xlsx"); fs::hard_link(&source,&alias).unwrap();
    for target in [&source,&alias,&existing] {
        let before=state(&s); assert!(s.save_as_new(target).is_err()); assert_eq!(state(&s),before);
    }
    assert_eq!(fs::read(existing).unwrap(),b"old output");
    assert_eq!(fs::read(source).unwrap(),fixture());
    assert!(s.is_dirty()); assert!(s.can_redo());
}

#[test]
fn changed_source_refuses_without_moving_save_point_or_creating_target() {
    let (directory, source, mut s) = prepare();
    edit(&mut s,42.0); s.save_as_new(&directory.path().join("saved.xlsx")).unwrap(); edit(&mut s,43.0);
    fs::write(&source,b"external replacement").unwrap();
    let before=state(&s); let output=directory.path().join("refused.xlsx");
    assert!(matches!(s.save_as_new(&output),Err(SaveError::SourceChanged)));
    assert_eq!(state(&s),before); assert!(!output.exists());
    assert_eq!(fs::read(source).unwrap(),b"external replacement");
}

#[test]
fn missing_parent_and_unsupported_path_spellings_do_not_mutate_state() {
    let (directory, source, mut s) = prepare(); edit(&mut s,42.0);
    for target in [directory.path().join("absent/new.xlsx"),directory.path().join("new.xlsx:stream"),
                   directory.path().join("new.xlsx."),directory.path().join("NUL.xlsx"),
                   directory.path().join("../outside.xlsx")] {
        let before=state(&s); assert!(s.save_as_new(&target).is_err()); assert_eq!(state(&s),before);
    }
    assert_eq!(fs::read(source).unwrap(),fixture());
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(),1);
}

#[test]
fn exclusive_source_and_destination_locks_preserve_work_and_old_output() {
    let (directory, source, mut s) = prepare(); edit(&mut s,42.0);
    let locked_source=OpenOptions::new().read(true).write(true).share_mode(0).open(&source).unwrap();
    let before=state(&s); let output=directory.path().join("locked-new.xlsx");
    assert!(s.save_as_new(&output).is_err()); assert_eq!(state(&s),before); assert!(!output.exists());
    drop(locked_source);
    fs::write(&output,b"locked old output").unwrap();
    let locked_output=OpenOptions::new().read(true).write(true).share_mode(0).open(&output).unwrap();
    assert!(s.save_as_new(&output).is_err()); assert_eq!(state(&s),before);
    drop(locked_output); assert_eq!(fs::read(output).unwrap(),b"locked old output");
}

#[test]
fn opening_is_bounded_and_memory_only_session_cannot_claim_file_source_checks() {
    let (directory, source, _) = prepare();
    let mut limits=WorkbookReadLimits::default(); limits.package.max_archive_bytes=1;
    assert!(matches!(WorkbookSession::open(&source,&limits,HistoryBudget::default()),Err(SaveError::SourceLimit)));
    let mut s=WorkbookSession::from_bytes(fixture(),&WorkbookReadLimits::default(),HistoryBudget::default()).unwrap();
    assert!(matches!(s.save_as_new(&directory.path().join("memory.xlsx")),Err(SaveError::NoFileSource)));
    assert!(WorkbookSession::open(Path::new("missing.xlsx"),&WorkbookReadLimits::default(),HistoryBudget::default()).is_err());
}

#[test]
fn detached_saved_bytes_are_charged_before_branch_commit() {
    let (directory, source, _) = prepare();
    let read=ReadWorkbook::read(fixture(),&WorkbookReadLimits::default()).unwrap();
    let saved=read.write_number_edit("Transactions",CellAddress::new(3,3).unwrap(),42.0).unwrap();
    let other=read.write_number_edit("Transactions",CellAddress::new(3,3).unwrap(),43.0).unwrap();
    let budget=HistoryBudget {max_frames:3,max_retained_archive_bytes:fixture().len()+saved.len()+other.len()-1};
    let mut s=WorkbookSession::open(&source,&WorkbookReadLimits::default(),budget).unwrap();
    edit(&mut s,42.0); s.save_as_new(&directory.path().join("saved.xlsx")).unwrap(); s.undo().unwrap();
    let before=state(&s);
    assert!(matches!(s.set_number("Transactions",CellAddress::new(3,3).unwrap(),43.0),Err(SessionError::HistoryBudget)));
    assert_eq!(state(&s),before); assert!(s.can_redo()); assert!(s.is_dirty());
}

#[test]
fn saving_a_new_branch_releases_old_detached_save_point_bytes() {
    let (directory, _, mut s) = prepare();
    edit(&mut s,42.0); let saved_len=s.bytes().len();
    s.save_as_new(&directory.path().join("first.xlsx")).unwrap(); s.undo().unwrap(); edit(&mut s,43.0);
    assert_eq!(s.history_frames(),2); assert!(!s.can_redo());
    assert_eq!(s.retained_archive_bytes(),fixture().len()+s.bytes().len()+saved_len);
    s.save_as_new(&directory.path().join("second.xlsx")).unwrap();
    assert_eq!(s.retained_archive_bytes(),fixture().len()+s.bytes().len()); assert!(!s.is_dirty());
    s.undo().unwrap(); assert!(s.is_dirty()); s.redo().unwrap(); assert!(!s.is_dirty());
}
