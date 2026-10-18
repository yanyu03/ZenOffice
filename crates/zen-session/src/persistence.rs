//! First saving tier: trusted local paths, Windows, create-new only.
//! It never overwrites an existing target or claims namespace compare-and-swap.
use crate::{HistoryBudget, SessionError, WorkbookSession};
use std::{fs::{self, File, OpenOptions}, io::{Read, Write}, path::{Component, Path, PathBuf}, sync::Arc};
use zen_formats::spreadsheet::WorkbookReadLimits;

#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Session(#[from] SessionError),
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

impl WorkbookSession {
    pub fn open(path: &Path, limits: &WorkbookReadLimits, budget: HistoryBudget) -> Result<Self, SaveError> {
        let path = local_path(path)?;
        let (_guard, bytes) = read_source(&path, limits.package.max_archive_bytes)?;
        let path = fs::canonicalize(path)?;
        let mut session = Self::from_bytes(&bytes, limits, budget)?;
        session.source = Some(path);
        Ok(session)
    }

    /// Publish a complete current candidate only to a new filename. The source
    /// remains the original input; Save As does not silently retarget its identity.
    pub fn save_as_new(&mut self, target: &Path) -> Result<(), SaveError> {
        if !cfg!(windows) { return Err(SaveError::UnsupportedPlatform); }
        let source = self.source.as_ref().ok_or(SaveError::NoFileSource)?;
        let target = local_path(target)?;
        let filename = target.file_name().ok_or(SaveError::UnsupportedPath)?;
        let parent = target.parent().ok_or(SaveError::UnsupportedPath)?;
        check_chain(parent)?;
        let parent = fs::canonicalize(parent)?;
        let target = parent.join(filename);
        match fs::symlink_metadata(&target) {
            Ok(_) => return Err(SaveError::DestinationExists),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
        let (_source_guard, bytes) = read_source(source, self.limits.package.max_archive_bytes)?;
        if bytes.as_slice() != self.frames[0].as_ref() { return Err(SaveError::SourceChanged); }
        let saved = Arc::clone(&self.frames[self.cursor]);
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(&saved)?;
        temporary.as_file().sync_all()?;
        // Noclobber is the publication condition, not a check followed by overwrite.
        // If another process creates target first, its bytes are left intact.
        temporary.persist_noclobber(&target).map_err(|e| SaveError::Io(e.error))?;
        self.save_point = saved;
        Ok(())
    }
}
