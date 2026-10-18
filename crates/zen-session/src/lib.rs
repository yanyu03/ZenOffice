//! Preserving numeric workbook transactions with bounded source checks.
//!
//! Only archive bytes are retained for history. Undo/redo re-read the selected
//! immutable frame with the original limits before changing any session state.
use std::sync::Arc;
mod persistence;
pub use persistence::SaveError;
use zen_engine::workbook::{CellAddress, TypedValue, Workbook};
use zen_formats::spreadsheet::{
    ReadDiagnostic, ReadError, ReadWorkbook, WorkbookReadLimits, edit::EditError,
};

/// Limits retained encoded archives, not exact process heap usage. The current
/// projection and a transient candidate are independently bounded by read limits.
#[derive(Debug, Clone, Copy)]
pub struct HistoryBudget {
    pub max_frames: usize,
    pub max_retained_archive_bytes: usize,
}

impl Default for HistoryBudget {
    fn default() -> Self {
        Self { max_frames: 64, max_retained_archive_bytes: 256 * 1024 * 1024 }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error(transparent)]
    Read(#[from] ReadError),
    #[error(transparent)]
    Edit(#[from] EditError),
    #[error("history budget exceeded; existing history was retained")]
    HistoryBudget,
    #[error("input number must be finite")]
    NonFinite,
    #[error("revision exhausted")]
    RevisionExhausted,
    #[error("history allocation failed: {0}")]
    Allocation(#[from] std::collections::TryReserveError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditOutcome {
    Changed,
    Unchanged,
}

#[derive(Debug)]
pub struct WorkbookSession {
    frames: Vec<Arc<[u8]>>,
    cursor: usize,
    projection: ReadWorkbook,
    limits: WorkbookReadLimits,
    budget: HistoryBudget,
    revision: u64,
    source: Option<std::path::PathBuf>,
    save_point: Arc<[u8]>,
}

impl WorkbookSession {
    pub fn from_bytes(
        bytes: &[u8], limits: &WorkbookReadLimits, budget: HistoryBudget,
    ) -> Result<Self, SessionError> {
        if budget.max_frames == 0 || bytes.len() > budget.max_retained_archive_bytes {
            return Err(SessionError::HistoryBudget);
        }
        let projection = ReadWorkbook::read(bytes, limits)?;
        let initial: Arc<[u8]> = Arc::from(bytes);
        Ok(Self { frames: vec![Arc::clone(&initial)], cursor: 0, projection,
                  limits: limits.clone(), budget, revision: 0, source: None, save_point: initial })
    }

    pub fn workbook(&self) -> &Workbook { self.projection.workbook() }
    pub fn read_workbook(&self) -> &ReadWorkbook { &self.projection }
    pub fn diagnostics(&self) -> &[ReadDiagnostic] { self.projection.diagnostics() }
    pub fn bytes(&self) -> &[u8] { &self.frames[self.cursor] }
    pub fn revision(&self) -> u64 { self.revision }
    pub fn can_undo(&self) -> bool { self.cursor > 0 }
    pub fn can_redo(&self) -> bool { self.cursor + 1 < self.frames.len() }
    /// Byte equality with the most recent successful save (initial input at open).
    pub fn is_dirty(&self) -> bool { self.bytes() != self.save_point.as_ref() }
    pub fn history_frames(&self) -> usize { self.frames.len() }
    pub fn retained_archive_bytes(&self) -> usize {
        // Every addition was checked against usize and the configured budget.
        let history: usize = self.frames.iter().map(|f| f.len()).sum();
        history + if self.frames.iter().any(|f| Arc::ptr_eq(f, &self.save_point)) {
            0
        } else { self.save_point.len() }
    }

    pub fn set_number(
        &mut self, sheet: &str, address: CellAddress, value: f64,
    ) -> Result<EditOutcome, SessionError> {
        if !value.is_finite() { return Err(SessionError::NonFinite); }
        // A numeric no-op preserves lexical spelling, ZIP bytes and redo. It
        // does not grant admission to modify a protected/unsupported package.
        let input = self.workbook().sheets().iter()
            .find(|s| s.name().eq_ignore_ascii_case(sheet)).and_then(|s| s.cell(address));
        if let Some(cell) = input {
            if cell.formula.is_none() {
                if let Some(TypedValue::Number { value: old, .. }) = &cell.value {
                    if *old == value { return Ok(EditOutcome::Unchanged); }
                }
            }
        }
        let candidate = self.projection.write_number_edit(sheet, address, value)?;
        self.commit_candidate(candidate)
    }

    fn commit_candidate(&mut self, bytes: Vec<u8>) -> Result<EditOutcome, SessionError> {
        if bytes == self.bytes() { return Ok(EditOutcome::Unchanged); }
        let prefix = &self.frames[..=self.cursor];
        let reuses_saved = bytes.as_slice() == self.save_point.as_ref();
        let detached_saved = !reuses_saved && !prefix.iter().any(|f| Arc::ptr_eq(f, &self.save_point));
        let retained = prefix.iter().try_fold(bytes.len(), |n, f| n.checked_add(f.len()))
            .and_then(|n| n.checked_add(if detached_saved { self.save_point.len() } else { 0 }));
        if prefix.len() >= self.budget.max_frames
            || retained.is_none_or(|n| n > self.budget.max_retained_archive_bytes) {
            return Err(SessionError::HistoryBudget);
        }
        let projection = ReadWorkbook::read(&bytes, &self.limits)?;
        let revision = self.revision.checked_add(1).ok_or(SessionError::RevisionExhausted)?;
        let frame: Arc<[u8]> = if reuses_saved { Arc::clone(&self.save_point) } else { Arc::from(bytes) };
        self.frames.try_reserve(1)?;
        // All fallible preparation precedes the transaction commit.
        self.frames.truncate(self.cursor + 1);
        self.frames.push(frame);
        self.cursor += 1;
        self.projection = projection;
        self.revision = revision;
        Ok(EditOutcome::Changed)
    }

    pub fn undo(&mut self) -> Result<EditOutcome, SessionError> {
        if !self.can_undo() { return Ok(EditOutcome::Unchanged); }
        self.restore(self.cursor - 1)
    }

    pub fn redo(&mut self) -> Result<EditOutcome, SessionError> {
        if !self.can_redo() { return Ok(EditOutcome::Unchanged); }
        self.restore(self.cursor + 1)
    }

    fn restore(&mut self, cursor: usize) -> Result<EditOutcome, SessionError> {
        let projection = ReadWorkbook::read(&self.frames[cursor], &self.limits)?;
        let revision = self.revision.checked_add(1).ok_or(SessionError::RevisionExhausted)?;
        self.projection = projection;
        self.cursor = cursor;
        self.revision = revision;
        Ok(EditOutcome::Changed)
    }
}

#[cfg(test)]
mod fault_tests {
    use super::*;
    fn sample() -> &'static [u8] {
        include_bytes!("../../../RCD/red-team-corpus/office-targets-2026-10/xlsx/zen_spreadsheet_multiscenario.xlsx")
    }
    fn session() -> WorkbookSession {
        WorkbookSession::from_bytes(sample(), &WorkbookReadLimits::default(), HistoryBudget::default()).unwrap()
    }
    #[test]
    fn invalid_candidate_parse_preserves_every_state_component() {
        let mut s = session();
        s.set_number("Transactions", CellAddress::new(3, 3).unwrap(), 42.0).unwrap();
        s.undo().unwrap();
        let before = (s.bytes().to_vec(), s.cursor, s.revision, s.history_frames(),
                      s.retained_archive_bytes(), s.is_dirty(), s.can_undo(), s.can_redo());
        assert!(matches!(s.commit_candidate(b"not a zip".to_vec()), Err(SessionError::Read(_))));
        assert_eq!(before, (s.bytes().to_vec(), s.cursor, s.revision, s.history_frames(),
                           s.retained_archive_bytes(), s.is_dirty(), s.can_undo(), s.can_redo()));
        assert_eq!(s.redo().unwrap(), EditOutcome::Changed);
    }
    #[test]
    fn revision_exhaustion_is_atomic_for_edit_and_restore() {
        let mut s = session();
        s.set_number("Transactions", CellAddress::new(3, 3).unwrap(), 42.0).unwrap();
        s.revision = u64::MAX;
        let bytes = s.bytes().to_vec();
        assert!(matches!(s.undo(), Err(SessionError::RevisionExhausted)));
        assert!(matches!(s.set_number("Transactions", CellAddress::new(3, 3).unwrap(), 43.0), Err(SessionError::RevisionExhausted)));
        assert_eq!(s.bytes(), bytes);
        assert_eq!(s.history_frames(), 2);
        assert_eq!(s.revision(), u64::MAX);
    }
}
