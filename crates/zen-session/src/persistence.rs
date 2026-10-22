//! First saving tier: trusted local paths, Windows, create-new only.
//! It never overwrites an existing target or claims namespace compare-and-swap.
use crate::{HistoryBudget, SessionError, WorkbookSession};
use std::{fs::{self, File, OpenOptions}, io::{Read, Write, Seek, SeekFrom}, path::{Component, Path, PathBuf}, sync::Arc};
use zen_formats::spreadsheet::{WorkbookReadLimits, WorkbookFrameContext};

// Private I/O boundary: tests inject actual partial temporary writes or errors
// at sync/publication. No environment flag or public bypass changes admission.
trait SaveIo {
    fn write(&mut self, temporary: &mut tempfile::NamedTempFile, bytes: &[u8]) -> std::io::Result<()>;
    fn sync(&mut self, temporary: &tempfile::NamedTempFile) -> std::io::Result<()>;
    fn publish(&mut self, temporary: tempfile::NamedTempFile, target: &Path) -> std::io::Result<()>;
}

struct FileSaveIo;
impl SaveIo for FileSaveIo {
    fn write(&mut self, temporary: &mut tempfile::NamedTempFile, bytes: &[u8]) -> std::io::Result<()> {
        temporary.write_all(bytes)
    }
    fn sync(&mut self, temporary: &tempfile::NamedTempFile) -> std::io::Result<()> {
        temporary.as_file().sync_all()
    }
    fn publish(&mut self, temporary: tempfile::NamedTempFile, target: &Path) -> std::io::Result<()> {
        temporary.persist_noclobber(target).map(|_| ()).map_err(|e| e.error)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Session(#[from] SessionError),
    #[error("temporary output bytes differ from the checked current frame")]
    TemporaryChanged,
    #[error("source archive changed since this session was opened")]
    SourceChanged,
    #[error("source archive exceeds configured read limit")]
    SourceLimit,
    #[error("save requires a file-backed session opened with WorkbookSession::open")]
    NoFileSource,
    #[error("destination already exists; replacement and same-path saves are unsupported")]
    DestinationExists,
    #[error("path is outside the supported local, non-reparse path subset")]
    UnsupportedPath,
    #[error("create-new persistence is currently admitted only on Windows")]
    UnsupportedPlatform,
}

fn reparse(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)] {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))] { metadata.file_type().is_symlink() }
}

fn check_chain(path: &Path) -> Result<(), SaveError> {
    let mut partial = PathBuf::new();
    for component in path.components() {
        partial.push(component.as_os_str());
        if matches!(component, Component::Prefix(_)) { continue; }
        if reparse(&fs::symlink_metadata(&partial)?) { return Err(SaveError::UnsupportedPath); }
    }
    Ok(())
}

fn regular_name(name: &std::ffi::OsStr) -> bool {
    let Some(text) = name.to_str() else { return false; };
    if text.is_empty() || text.ends_with(['.', ' ']) || text.contains([':', '\0']) { return false; }
    let stem = text.split('.').next().unwrap().to_ascii_uppercase();
    !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        && !(stem.len() == 4 && (stem.starts_with("COM") || stem.starts_with("LPT"))
             && matches!(stem.as_bytes()[3], b'1'..=b'9'))
}

fn local_path(path: &Path) -> Result<PathBuf, SaveError> {
    if path.as_os_str().is_empty() { return Err(SaveError::UnsupportedPath); }
    for component in path.components() {
        match component {
            Component::ParentDir => return Err(SaveError::UnsupportedPath),
            Component::Normal(name) if !regular_name(name) => return Err(SaveError::UnsupportedPath),
            #[cfg(windows)]
            Component::Prefix(prefix) => {
                if !matches!(prefix.kind(), std::path::Prefix::Disk(_)) || !path.is_absolute() {
                    return Err(SaveError::UnsupportedPath);
                }
            }
            _ => ()
        }
    }
    Ok(if path.is_absolute() { path.to_owned() } else { std::env::current_dir()?.join(path) })
}

/// Read under a bounded limit; on Windows deny concurrent writers/deleters for
/// this handle's lifetime. Parent-namespace mutation is not made transactional.
fn read_source(path: &Path, limit: u64) -> Result<(File, Vec<u8>), SaveError> {
    check_chain(path)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)] {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1).custom_flags(0x00200000); // READ sharing, OPEN_REPARSE_POINT
    }
    let mut file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || reparse(&metadata) { return Err(SaveError::UnsupportedPath); }
    if metadata.len() > limit { return Err(SaveError::SourceLimit); }
    let mut bytes = Vec::new();
    Read::by_ref(&mut file).take(limit.saturating_add(1)).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit { return Err(SaveError::SourceLimit); }
    Ok((file, bytes))
}

/// Issued only after successful create-new publication. The bound path, exact
/// frame digest, original policy and original limits/budget cannot be replaced.
/// In-memory only; no Deserialize/new/import API or model/archives retained.
#[derive(Debug)]
pub struct SavedWorkbookReceipt {
    target: PathBuf,
    context: WorkbookFrameContext,
    budget: HistoryBudget,
}
impl SavedWorkbookReceipt {
    pub fn frame_sha256(&self) -> [u8; 32] { self.context.frame_sha256() }
    pub fn policy_identity(&self) -> Option<[u8; 32]> { self.context.policy_identity() }
    pub fn retained_policy_descriptor_bytes(&self) -> usize {
        self.context.retained_policy_descriptor_bytes()
    }
    /// Reopen only the successfully published filename. A moved, changed,
    /// locked or reparse path refuses; no alternate search or policy reseal.
    /// New history starts at revision0/clean with the saved frame as file source.
    pub fn reopen(&self) -> Result<WorkbookSession, SaveError> {
        let limits = self.context.limits();
        let (_guard, bytes) = read_source(&self.target, limits.package.max_archive_bytes)?;
        let projection = self.context.read(&bytes).map_err(SessionError::from)?;
        // The ordinary constructor applies the original history budget. The
        // validated context attaches the SAME immutable policy without calling
        // initial-source admission or changing its source SHA.
        let mut session = WorkbookSession::from_bytes(&bytes, limits, self.budget)?;
        session.projection = projection;
        session.source = Some(self.target.clone());
        Ok(session)
    }
}

fn destination_path(target: &Path) -> Result<PathBuf, SaveError> {
    let target = local_path(target)?;
    let filename = target.file_name().ok_or(SaveError::UnsupportedPath)?;
    let parent = target.parent().ok_or(SaveError::UnsupportedPath)?;
    check_chain(parent)?;
    Ok(fs::canonicalize(parent)?.join(filename))
}

impl WorkbookSession {
    pub fn open(path: &Path, limits: &WorkbookReadLimits, budget: HistoryBudget) -> Result<Self, SaveError> {
        let path = local_path(path)?;
        let (_guard, bytes) = read_source(&path, limits.package.max_archive_bytes)?;
        let path = fs::canonicalize(path)?;
        let mut session = Self::from_bytes(&bytes, limits, budget)?;
        session.source = Some(path);
        Ok(session)
    }

    /// File-backed original controlled source. Saved output is not adopted as
    /// an initial source; editable saved reopening requires a SavedWorkbookReceipt.
    pub fn open_with_opaque_xml_policy(
        path: &Path, limits: &WorkbookReadLimits, budget: HistoryBudget,
        policy: Arc<zen_opc::opaque_xml::OpaqueXmlPolicy>,
    ) -> Result<Self, SaveError> {
        let path = local_path(path)?;
        let (_guard, bytes) = read_source(&path, limits.package.max_archive_bytes)?;
        let path = fs::canonicalize(path)?;
        let mut session = Self::from_bytes_with_opaque_xml_policy(&bytes, limits, budget, policy)?;
        session.source = Some(path);
        Ok(session)
    }

    /// Publish a complete current candidate only to a new filename. The source
    /// remains the original input; Save As does not silently retarget its identity.
    pub fn save_as_new(&mut self, target: &Path) -> Result<(), SaveError> {
        self.save_as_new_with(target, &mut FileSaveIo)
    }

    /// Prepare the immutable frame context before I/O; expose it as a saved
    /// receipt only after the same safe publication path succeeds.
    pub fn save_as_new_with_receipt(&mut self, target: &Path) -> Result<SavedWorkbookReceipt, SaveError> {
        self.save_as_new_with_receipt_with(target, &mut FileSaveIo)
    }
    fn save_as_new_with_receipt_with(
        &mut self, target: &Path, io: &mut impl SaveIo,
    ) -> Result<SavedWorkbookReceipt, SaveError> {
        let published_target = destination_path(target)?;
        let context = self.projection.prepare_frame_reopen(self.bytes(), &self.limits)
            .map_err(SessionError::from)?;
        let receipt = SavedWorkbookReceipt { target: published_target, context, budget: self.budget };
        self.save_as_new_impl(target, io, true)?;
        Ok(receipt)
    }

    fn save_as_new_with(&mut self, target: &Path, io: &mut impl SaveIo) -> Result<(), SaveError> {
        self.save_as_new_impl(target, io, false)
    }
    fn save_as_new_impl(
        &mut self, target: &Path, io: &mut impl SaveIo, verify_temporary: bool,
    ) -> Result<(), SaveError> {
        if !cfg!(windows) { return Err(SaveError::UnsupportedPlatform); }
        let source = self.source.as_ref().ok_or(SaveError::NoFileSource)?;
        let target = destination_path(target)?;
        let parent = target.parent().ok_or(SaveError::UnsupportedPath)?;
        match fs::symlink_metadata(&target) {
            Ok(_) => return Err(SaveError::DestinationExists),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
        let (_source_guard, bytes) = read_source(source, self.limits.package.max_archive_bytes)?;
        if bytes.as_slice() != self.frames[0].as_ref() { return Err(SaveError::SourceChanged); }
        self.projection.recheck_preservation(self.bytes()).map_err(SessionError::from)?;
        let saved = Arc::clone(&self.frames[self.cursor]);
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        io.write(&mut temporary, &saved)?;
        io.sync(&temporary)?;
        if verify_temporary || self.opaque_xml_policy_identity().is_some() {
            // Verify the actual owned temporary handle before publication.
            // No namespace CAS or post-publication rollback is claimed.
            temporary.as_file_mut().seek(SeekFrom::Start(0))?;
            let mut actual = Vec::new();
            Read::by_ref(temporary.as_file_mut()).take((saved.len() as u64).saturating_add(1))
                .read_to_end(&mut actual)?;
            if actual.as_slice() != saved.as_ref() { return Err(SaveError::TemporaryChanged); }
            self.projection.recheck_preservation(&actual).map_err(SessionError::from)?;
        }
        // Noclobber is the publication condition, not a check followed by overwrite.
        // If another process creates target first, its bytes are left intact.
        io.publish(temporary, &target)?;
        self.save_point = saved;
        Ok(())
    }
}

#[cfg(all(test, windows))]
mod failure_tests {
    use super::*;
    use zen_engine::workbook::CellAddress;

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Fault { Write, Sync, Publish, Contender }
    struct FailingIo { fault: Fault, reached: Vec<&'static str> }
    impl SaveIo for FailingIo {
        fn write(&mut self, temporary: &mut tempfile::NamedTempFile, bytes: &[u8]) -> std::io::Result<()> {
            self.reached.push("write");
            if self.fault == Fault::Write {
                let prefix = bytes.len() / 2;
                assert!(prefix > 0);
                temporary.write_all(&bytes[..prefix])?;
                assert_eq!(temporary.as_file().metadata()?.len(), prefix as u64);
                return Err(std::io::Error::other("injected after partial write"));
            }
            FileSaveIo.write(temporary, bytes)
        }
        fn sync(&mut self, temporary: &tempfile::NamedTempFile) -> std::io::Result<()> {
            self.reached.push("sync");
            if self.fault == Fault::Sync {
                assert!(temporary.as_file().metadata()?.len() > 0);
                return Err(std::io::Error::other("injected sync failure"));
            }
            FileSaveIo.sync(temporary)
        }
        fn publish(&mut self, temporary: tempfile::NamedTempFile, target: &Path) -> std::io::Result<()> {
            self.reached.push("publish");
            if self.fault == Fault::Publish {
                return Err(std::io::Error::other("injected publication failure"));
            }
            assert!(self.fault == Fault::Contender);
            // Deterministic arrival after admission, before real noclobber.
            let mut contender = OpenOptions::new().write(true).create_new(true).open(target)?;
            contender.write_all(b"late competing output")?;
            contender.sync_all()?;
            drop(contender);
            FileSaveIo.publish(temporary, target)
        }
    }

    fn fixture() -> &'static [u8] {
        include_bytes!("../../../RCD/red-team-corpus/office-targets-2026-10/xlsx/zen_spreadsheet_multiscenario.xlsx")
    }
    fn state(s: &WorkbookSession) -> (Vec<u8>, Vec<Vec<u8>>, Vec<u8>, usize, u64, bool, usize, bool, bool) {
        (s.bytes().to_vec(), s.frames.iter().map(|f| f.to_vec()).collect(), s.save_point.to_vec(),
         s.cursor, s.revision(), s.is_dirty(), s.retained_archive_bytes(), s.can_undo(), s.can_redo())
    }
    fn check_failure(fault: Fault, expected_stages: &[&str]) {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source.xlsx");
        fs::write(&source, fixture()).unwrap();
        let mut s = WorkbookSession::open(&source, &WorkbookReadLimits::default(), HistoryBudget::default()).unwrap();
        let address = CellAddress::new(3, 3).unwrap();
        s.set_number("Transactions", address, 42.0).unwrap();
        let old_output = directory.path().join("previous-save.xlsx");
        s.save_as_new(&old_output).unwrap();
        let old_bytes = fs::read(&old_output).unwrap();
        s.set_number("Transactions", address, 43.0).unwrap();
        s.set_number("Transactions", address, 44.0).unwrap();
        s.undo().unwrap();
        let before = state(&s);
        assert!(s.is_dirty() && s.can_redo());
        let target = directory.path().join("attempt.xlsx");
        let mut io = FailingIo { fault, reached: vec![] };
        assert!(matches!(s.save_as_new_with(&target, &mut io), Err(SaveError::Io(_))));
        assert_eq!(io.reached, expected_stages);
        assert_eq!(state(&s), before);
        assert_eq!(fs::read(&source).unwrap(), fixture());
        assert_eq!(fs::read(&old_output).unwrap(), old_bytes);
        if fault == Fault::Contender {
            assert_eq!(fs::read(&target).unwrap(), b"late competing output");
        } else { assert!(!target.exists()); }
        // A dropped temporary must not leave a partial output or temp artifact.
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), if fault == Fault::Contender { 3 } else { 2 });
        s.redo().unwrap(); // Failed saving did not discard the future frame.
        s.undo().unwrap();
        let retry = directory.path().join("retry.xlsx");
        s.save_as_new(&retry).unwrap();
        assert!(!s.is_dirty());
        assert_eq!(fs::read(retry).unwrap(), s.bytes());
        s.undo().unwrap(); // Return to the prior 42 save content, now dirty.
        assert!(s.is_dirty());
        s.redo().unwrap(); assert!(!s.is_dirty());
    }

    #[test]
    fn partial_write_failure_preserves_frame_history_save_point_and_outputs() {
        check_failure(Fault::Write, &["write"]);
    }
    #[test]
    fn sync_failure_preserves_frame_history_save_point_and_outputs() {
        check_failure(Fault::Sync, &["write", "sync"]);
    }
    #[test]
    fn publication_failure_preserves_frame_history_save_point_and_outputs() {
        check_failure(Fault::Publish, &["write", "sync", "publish"]);
    }
    #[test]
    fn late_existing_target_is_never_replaced_and_retry_still_succeeds() {
        check_failure(Fault::Contender, &["write", "sync", "publish"]);
    }

    fn policy_file_session(source_path:&Path) -> WorkbookSession {
        use zen_opc::opaque_xml::{OpaqueXmlDeclaration,OpaqueXmlPolicy,sha256};
        let source=fs::read(source_path).unwrap();
        let archive=zen_opc::ZipArchive::read(&source).unwrap();
        let payload=archive.get("customXml/zen-probe.xml").unwrap();
        let d=OpaqueXmlDeclaration{
            source_sha256:sha256(&source),part_name:"/customXml/zen-probe.xml".into(),
            payload_sha256:sha256(payload),payload_bytes:payload.len(),content_type:"application/xml".into(),
            relationship_id:"rIdProbe".into(),relationship_target:"../customXml/zen-probe.xml".into(),
            controlled_owned_source:true,semantically_independent:true,review_basis:"Owned sealed budget; private persistence fault injection".into(),
        };
        let policy=Arc::new(OpaqueXmlPolicy::bind_initial(&source,&d).unwrap());
        WorkbookSession::open_with_opaque_xml_policy(source_path,&WorkbookReadLimits::default(),HistoryBudget::default(),policy).unwrap()
    }
    #[test]
    fn opaque_policy_is_retained_across_each_actual_save_io_failure() {
        use zen_engine::workbook::TypedValue;
        for fault in [Fault::Write,Fault::Sync,Fault::Publish,Fault::Contender] {
            let directory=tempfile::tempdir().unwrap();let source=directory.path().join("source.xlsx");
            let original=include_bytes!("../../../RCD/engine-acceptance/eng-005-business-v2/budget.xlsx");
            fs::write(&source,original).unwrap();
            let mut s=policy_file_session(&source);let identity=s.opaque_xml_policy_identity();
            let a=CellAddress::new(11,1).unwrap();
            s.set_value("Budget",a,Some(TypedValue::Text("Reviewed".into()))).unwrap();
            let prior=directory.path().join("prior.xlsx");s.save_as_new(&prior).unwrap();let prior_bytes=fs::read(&prior).unwrap();
            s.set_value("Budget",a,Some(TypedValue::Boolean(true))).unwrap();
            s.set_value("Budget",a,None).unwrap();s.undo().unwrap();
            let before=state(&s);let target=directory.path().join("attempt.xlsx");
            let mut io=FailingIo{fault,reached:vec![]};
            assert!(matches!(s.save_as_new_with_receipt_with(&target,&mut io),Err(SaveError::Io(_))));
            assert_eq!(state(&s),before);assert_eq!(s.opaque_xml_policy_identity(),identity);
            assert_eq!(fs::read(&source).unwrap(),original);assert_eq!(fs::read(&prior).unwrap(),prior_bytes);
            if fault==Fault::Contender {assert_eq!(fs::read(&target).unwrap(),b"late competing output");}
            else {assert!(!target.exists());}
            assert_eq!(fs::read_dir(directory.path()).unwrap().count(),if fault==Fault::Contender{3}else{2});
            s.redo().unwrap();assert_eq!(s.opaque_xml_policy_identity(),identity);
        }
    }
    struct CorruptingTemporary;
    impl SaveIo for CorruptingTemporary {
        fn write(&mut self, temporary:&mut tempfile::NamedTempFile, _bytes:&[u8])->std::io::Result<()> {
            temporary.write_all(b"owned wrong temporary bytes")
        }
        fn sync(&mut self, temporary:&tempfile::NamedTempFile)->std::io::Result<()> {FileSaveIo.sync(temporary)}
        fn publish(&mut self, _temporary:tempfile::NamedTempFile, _target:&Path)->std::io::Result<()> {
            panic!("unverified temporary must not publish")
        }
    }
    #[test]
    fn opaque_policy_checks_actual_temporary_before_publication() {
        use zen_engine::workbook::TypedValue;
        let directory=tempfile::tempdir().unwrap();let source=directory.path().join("source.xlsx");
        let original=include_bytes!("../../../RCD/engine-acceptance/eng-005-business-v2/budget.xlsx");
        fs::write(&source,original).unwrap();
        let mut s=policy_file_session(&source);
        s.set_value("Budget",CellAddress::new(11,1).unwrap(),Some(TypedValue::Text("Reviewed".into()))).unwrap();
        let before=state(&s);let identity=s.opaque_xml_policy_identity();
        let target=directory.path().join("attempt.xlsx");
        assert!(matches!(s.save_as_new_with_receipt_with(&target,&mut CorruptingTemporary),Err(SaveError::TemporaryChanged)));
        assert_eq!(state(&s),before);assert_eq!(s.opaque_xml_policy_identity(),identity);
        assert!(!target.exists());assert_eq!(fs::read(&source).unwrap(),original);
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(),1);
    }

}
