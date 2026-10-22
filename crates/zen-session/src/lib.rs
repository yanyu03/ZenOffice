//! Preserving numeric workbook transactions with bounded source checks.
//!
//! Only archive bytes are retained for history. Undo/redo re-read the selected
//! immutable frame with the original limits before changing any session state.
use std::sync::Arc;
mod persistence;
pub use persistence::SaveError;
use zen_engine::workbook::{CellAddress, DateSystem, TypedValue, Workbook};
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

    /// Explicit reviewed owned-input policy; never enabled by discovery/env.
    /// The original source hash is required here, even for saved output bytes.
    pub fn from_bytes_with_opaque_xml_policy(
        bytes: &[u8], limits: &WorkbookReadLimits, budget: HistoryBudget,
        policy: Arc<zen_opc::opaque_xml::OpaqueXmlPolicy>,
    ) -> Result<Self, SessionError> {
        let mut result = Self::from_bytes(bytes, limits, budget)?;
        result.projection = ReadWorkbook::read_with_opaque_xml_policy(bytes, limits, policy)?;
        Ok(result)
    }
    pub fn opaque_xml_policy_identity(&self) -> Option<[u8; 32]> {
        self.projection.opaque_xml_policy_identity()
    }
    pub fn retained_policy_descriptor_bytes(&self) -> usize {
        self.projection.retained_policy_descriptor_bytes()
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

    /// Change an existing input through the same bounded immutable-frame transaction.
    /// None is blank; empty text and false remain distinct. Formulas/dates/errors
    /// are not accepted as edit intents by this first typed-input slice.
    pub fn set_value(
        &mut self, sheet: &str, address: CellAddress, value: Option<TypedValue>,
    ) -> Result<EditOutcome, SessionError> {
        if matches!(&value, Some(TypedValue::Number { value, .. }) if !value.is_finite()) {
            return Err(SessionError::NonFinite);
        }
        if let Some(TypedValue::Text(text)) = &value {
            if text.len() > self.limits.max_text_bytes {
                return Err(SessionError::Read(ReadError::Limit("text bytes")));
            }
        }
        if !matches!(&value, Some(TypedValue::Error(_))) {
            if let Some(input) = self.workbook().sheets().iter()
                .find(|s| s.name().eq_ignore_ascii_case(sheet)).and_then(|s| s.cell(address)) {
                let equal = match (&input.value, &value) {
                    (Some(TypedValue::Number { value: a, .. }), Some(TypedValue::Number { value: b, .. })) => a == b,
                    _ => input.value == value,
                };
                if input.formula.is_none() && equal { return Ok(EditOutcome::Unchanged); }
            }
        }
        let candidate = self.projection.write_value_edit(sheet, address, value)?;
        self.commit_candidate(candidate)
    }

    /// Set an explicit numeric date serial in an already applied supported date
    /// style. System/style admission also applies to no-ops. No conversion or
    /// rendering is implied; serial 60 in 1900 stays compatibility serial data.
    pub fn set_date_serial(
        &mut self, sheet: &str, address: CellAddress, serial: f64, system: DateSystem,
    ) -> Result<EditOutcome, SessionError> {
        self.projection.validate_date_serial(sheet, address, serial, system)?;
        if self.workbook().sheets().iter().find(|s| s.name().eq_ignore_ascii_case(sheet))
            .and_then(|s| s.cell(address)).is_some_and(|c| {
                matches!(&c.value, Some(TypedValue::Number { value, .. }) if *value == serial)
            }) {
            return Ok(EditOutcome::Unchanged);
        }
        let candidate = self.projection.write_date_serial_edit(sheet, address, serial, system)?;
        self.commit_candidate(candidate)
    }

    /// Replace/create a normal formula in an existing cell. A supported optional
    /// calcChain is invalidated; unknown chain/error/empty results are rejected.
    pub fn set_formula(
        &mut self, sheet: &str, address: CellAddress, formula: &str,
    ) -> Result<EditOutcome, SessionError> {
        if formula.len() > self.limits.max_text_bytes {
            return Err(SessionError::Read(ReadError::Limit("formula text bytes")));
        }
        let normalized = formula.strip_prefix('=').unwrap_or(formula);
        if !normalized.trim().is_empty() && !normalized.trim_start().starts_with('=') {
            if let Some(input) = self.workbook().sheets().iter()
                .find(|s| s.name().eq_ignore_ascii_case(sheet)).and_then(|s| s.cell(address)) {
                if input.formula.as_ref().is_some_and(|f| f.kind == zen_engine::workbook::FormulaKind::Normal && f.text == normalized) {
                    return Ok(EditOutcome::Unchanged);
                }
            }
        }
        let candidate = self.projection.write_formula_edit(sheet, address, formula)?;
        self.commit_candidate(candidate)
    }

    /// Clear a normal formula and its cache to blank, preserving cell/style.
    /// An existing blank is a byte-preserving no-op; nonblank inputs are not
    /// formula targets. Chain admission/metadata/error/cache rules still apply.
    pub fn clear_formula(
        &mut self, sheet: &str, address: CellAddress,
    ) -> Result<EditOutcome, SessionError> {
        if let Some(input) = self.workbook().sheets().iter()
            .find(|s| s.name().eq_ignore_ascii_case(sheet)).and_then(|s| s.cell(address)) {
            if input.formula.is_none() && input.value.is_none() {
                return Ok(EditOutcome::Unchanged);
            }
        }
        let candidate = self.projection.write_clear_formula_edit(sheet, address)?;
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
        let projection = self.projection.reread_candidate(&bytes, &self.limits)?;
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
        let projection = self.projection.reread_candidate(&self.frames[cursor], &self.limits)?;
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

    fn opaque_session() -> WorkbookSession {
        use zen_opc::opaque_xml::{OpaqueXmlDeclaration, OpaqueXmlPolicy, sha256};
        let bytes=include_bytes!("../../../RCD/engine-acceptance/eng-005-business-v2/budget.xlsx");
        let zip=zen_opc::ZipArchive::read(bytes).unwrap();
        let payload=zip.get("customXml/zen-probe.xml").unwrap();
        let declaration=OpaqueXmlDeclaration{
            source_sha256:sha256(bytes),part_name:"/customXml/zen-probe.xml".into(),
            payload_sha256:sha256(payload),payload_bytes:payload.len(),content_type:"application/xml".into(),
            relationship_id:"rIdProbe".into(),relationship_target:"../customXml/zen-probe.xml".into(),
            controlled_owned_source:true,semantically_independent:true,review_basis:"Owned sealed budget; private candidate/history fault specimen".into(),
        };
        let policy=Arc::new(OpaqueXmlPolicy::bind_initial(bytes,&declaration).unwrap());
        WorkbookSession::from_bytes_with_opaque_xml_policy(bytes,&WorkbookReadLimits::default(),HistoryBudget::default(),policy).unwrap()
    }
    #[test]
    fn opaque_policy_candidate_and_restore_failures_are_atomic() {
        let mut s=opaque_session();
        let initial=s.bytes().to_vec();let identity=s.opaque_xml_policy_identity();
        s.set_value("Budget",CellAddress::new(11,1).unwrap(),Some(TypedValue::Text("Reviewed".into()))).unwrap();
        let mut p=zen_opc::Package::from_bytes(&initial).unwrap();
        p.put_text("/customXml/zen-probe.xml","<probe>owned fault mutation</probe>").unwrap();
        let bad=p.to_bytes().unwrap();
        let snapshot=|s:&WorkbookSession|(s.bytes().to_vec(),s.cursor,s.revision,s.history_frames(),s.retained_archive_bytes(),s.is_dirty(),s.can_undo(),s.can_redo(),s.opaque_xml_policy_identity());
        let before=snapshot(&s);
        assert!(matches!(s.commit_candidate(bad.clone()),Err(SessionError::Read(_))));
        assert_eq!(snapshot(&s),before);
        // Private fault injection only: a retained frame is made invalid. No
        // public frame/patch/import API exposes this mutation.
        s.frames[0]=Arc::from(bad);
        let before=snapshot(&s);
        assert!(matches!(s.undo(),Err(SessionError::Read(_))));
        assert_eq!(snapshot(&s),before);
        assert_eq!(s.opaque_xml_policy_identity(),identity);
        s.frames[0]=Arc::from(initial);
        s.undo().unwrap();assert!(!s.is_dirty());
        assert_eq!(s.opaque_xml_policy_identity(),identity);
    }

}
