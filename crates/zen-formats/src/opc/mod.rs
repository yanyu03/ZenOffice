//! Read-only, bounded ECMA-376-2 package foundation.
//! Retains opaque bytes. It does not import worksheet semantics or authorize saves.
mod uri;
pub(crate) mod xml;
mod zip_layout;

use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use std::path::Path;
pub use uri::PartName;

pub const RELATIONSHIPS_MEDIA_TYPE: &str =
    "application/vnd.openxmlformats-package.relationships+xml";
pub const OFFICE_DOCUMENT_RELATIONSHIP: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
pub const STRICT_OFFICE_DOCUMENT_RELATIONSHIP: &str =
    "http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument";

#[derive(Debug, thiserror::Error)]
pub enum OpcError {
    #[error("OPC package is invalid: {0}")]
    Invalid(String),
    #[error("This OPC reader subset does not support: {0}")]
    Unsupported(String),
    #[error("OPC parsing resource limit exceeded: {0}")]
    Limit(&'static str),
    #[error("OPC ZIP read failed: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("OPC XML read failed: {0}")]
    Xml(#[from] roxmltree::Error),
    #[error("OPC file read failed: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone)]
pub struct ReadLimits {
    pub max_archive_bytes: u64,
    pub max_entries: usize,
    pub max_part_bytes: u64,
    pub max_total_bytes: u64,
    pub max_metadata_xml_bytes: u64,
    pub max_xml_nodes: u32,
    pub max_relationships: usize,
    pub max_compression_ratio: u64,
}
impl Default for ReadLimits {
    fn default() -> Self {
        Self {
            max_archive_bytes: 64 * 1024 * 1024,
            max_entries: 4096,
            max_part_bytes: 32 * 1024 * 1024,
            max_total_bytes: 256 * 1024 * 1024,
            max_metadata_xml_bytes: 4 * 1024 * 1024,
            max_xml_nodes: 100_000,
            max_relationships: 16_384,
            max_compression_ratio: 1000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartDisposition {
    /// Package metadata is understood; document semantics are not.
    RelationshipsMetadata,
    /// Original uncompressed bytes retained; edits/round trips are not yet supported.
    PreservedOpaque,
}

#[derive(Debug)]
pub struct PackagePart {
    name: PartName,
    content_type: String,
    bytes: Vec<u8>,
    disposition: PartDisposition,
}
impl PackagePart {
    pub fn name(&self) -> &PartName {
        &self.name
    }
    pub fn content_type(&self) -> &str {
        &self.content_type
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn disposition(&self) -> PartDisposition {
        self.disposition
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelationshipTarget {
    Internal {
        part: PartName,
        fragment: Option<String>,
    },
    /// Stored verbatim; never opened, downloaded or resolved against a host filesystem.
    External { reference: String },
}
#[derive(Debug, Clone)]
pub struct Relationship {
    /// None denotes the package itself; IDs are unique only within this source.
    pub source: Option<PartName>,
    pub id: String,
    pub relationship_type: String,
    pub raw_target: String,
    pub target: RelationshipTarget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticCode {
    DanglingTarget,
    UnmappedZipItem,
    OpaqueDocumentContent,
}
#[derive(Debug, Clone)]
pub struct PackageDiagnostic {
    pub code: DiagnosticCode,
    pub location: String,
    pub message: String,
}
#[derive(Debug)]
pub struct UnmappedZipItem {
    pub name: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug)]
pub struct Package {
    parts: BTreeMap<String, PackagePart>,
    relationships: Vec<Relationship>,
    content_types_bytes: Vec<u8>,
    unmapped_items: Vec<UnmappedZipItem>,
    diagnostics: Vec<PackageDiagnostic>,
}

impl Package {
    pub fn open(path: &Path, limits: &ReadLimits) -> Result<Self, OpcError> {
        let file = std::fs::File::open(path)?;
        if file.metadata()?.len() > limits.max_archive_bytes {
            return Err(OpcError::Limit("archive bytes"));
        }
        // A growing file is also bounded; all later parsing uses this immutable byte snapshot.
        let mut bytes = Vec::new();
        file.take(limits.max_archive_bytes.saturating_add(1))
            .read_to_end(&mut bytes)?;
        Self::read(&bytes, limits)
    }

    pub fn read(bytes: &[u8], limits: &ReadLimits) -> Result<Self, OpcError> {
        let expected_entries = zip_layout::preflight(bytes, limits)?;
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes))?;
        if zip.len() != expected_entries {
            return Err(OpcError::Invalid(
                "ZIP reader entry count differs from physical directory".into(),
            ));
        }
        let mut raw = BTreeMap::new();
        let mut total = 0u64;
        for index in 0..zip.len() {
            // Examine raw metadata before asking the library to decode/decrypt.
            let metadata = zip.by_index_raw(index)?;
            if metadata.encrypted() {
                return Err(OpcError::Unsupported("encrypted ZIP item".into()));
            }
            let name = std::str::from_utf8(metadata.name_raw())
                .map_err(|_| OpcError::Invalid("Invalid ZIP item name".into()))?
                .to_owned();
            let expected_size = metadata.size();
            if metadata.is_symlink() {
                return Err(OpcError::Unsupported(
                    "symbolic links in OPC ZIP items".into(),
                ));
            }
            if metadata.is_dir() {
                if expected_size != 0 {
                    return Err(OpcError::Invalid("Directory ZIP item has content".into()));
                }
                continue;
            }
            if expected_size > limits.max_part_bytes {
                return Err(OpcError::Limit("part bytes"));
            }
            drop(metadata);
            if name
                .rsplit('/')
                .next()
                .is_some_and(|s| s.starts_with('[') && s.ends_with(".piece"))
            {
                return Err(OpcError::Unsupported("interleaved OPC pieces".into()));
            }
            let mut entry = zip.by_index(index)?;
            let mut content = Vec::new();
            (&mut entry)
                .take(expected_size.saturating_add(1))
                .read_to_end(&mut content)?;
            if content.len() as u64 != expected_size {
                return Err(OpcError::Invalid("ZIP expanded size mismatch".into()));
            }
            // Read through EOF to trigger the library's CRC check, even for empty entries.
            let mut end = [0u8; 1];
            if entry.read(&mut end)? != 0 {
                return Err(OpcError::Invalid("ZIP entry exceeds declared size".into()));
            }
            total = total
                .checked_add(content.len() as u64)
                .ok_or(OpcError::Limit("expanded package bytes"))?;
            if total > limits.max_total_bytes {
                return Err(OpcError::Limit("expanded package bytes"));
            }
            if raw.insert(name, content).is_some() {
                return Err(OpcError::Invalid("Duplicate ZIP entry".into()));
            }
        }
        let content_types_bytes = raw
            .remove("[Content_Types].xml")
            .ok_or_else(|| OpcError::Invalid("Missing [Content_Types].xml".into()))?;
        let types = xml::ContentTypes::parse(&content_types_bytes, limits)?;
        let mut package = Self {
            parts: BTreeMap::new(),
            relationships: Vec::new(),
            content_types_bytes,
            unmapped_items: Vec::new(),
            diagnostics: Vec::new(),
        };
        for (item, bytes) in raw {
            let name = match PartName::parse(&format!("/{item}")) {
                Ok(name) => name,
                Err(OpcError::Unsupported(reason)) => return Err(OpcError::Unsupported(reason)),
                Err(_) => {
                    // Part 2 7.2.5.5 allows items which do not map to parts, e.g. [trash].
                    // Retain them without extracting to disk or treating them as relationships.
                    package.diagnostics.push(PackageDiagnostic {
                        code: DiagnosticCode::UnmappedZipItem,
                        location: item.clone(),
                        message: "ZIP item does not map to an OPC part; retained opaque".into(),
                    });
                    package
                        .unmapped_items
                        .push(UnmappedZipItem { name: item, bytes });
                    continue;
                }
            };
            let key = name.key();
            let content_type = types.for_part(&name)?;
            if content_type.eq_ignore_ascii_case(RELATIONSHIPS_MEDIA_TYPE)
                && !name.is_relationship_part()
            {
                return Err(OpcError::Invalid(
                    "Relationships media type requires a reserved Relationships part name".into(),
                ));
            }
            let disposition = if name.is_relationship_part() {
                PartDisposition::RelationshipsMetadata
            } else {
                PartDisposition::PreservedOpaque
            };
            if package
                .parts
                .insert(
                    key,
                    PackagePart {
                        name,
                        content_type,
                        bytes,
                        disposition,
                    },
                )
                .is_some()
            {
                return Err(OpcError::Invalid("ASCII case-equivalent part names".into()));
            }
        }
        // Parts cannot derive from each other, e.g. /a and /a/b.xml (6.2.2.3).
        for name in package.parts.keys() {
            for (position, _) in name.match_indices('/').skip(1) {
                if package.parts.contains_key(&name[..position]) {
                    return Err(OpcError::Invalid(
                        "A part name derives from another part name".into(),
                    ));
                }
            }
        }
        for part in package
            .parts
            .values()
            .filter(|p| p.name.is_relationship_part())
        {
            let source = part.name.relationship_source()?;
            if source
                .as_ref()
                .is_some_and(|s| !package.parts.contains_key(&s.key()))
            {
                return Err(OpcError::Invalid(format!(
                    "Relationships source missing: {}",
                    part.name.as_str()
                )));
            }
            let links = xml::relationships(&part.bytes, source.as_ref(), limits)?;
            if package.relationships.len().saturating_add(links.len()) > limits.max_relationships {
                return Err(OpcError::Limit("relationships"));
            }
            for link in &links {
                if let RelationshipTarget::Internal { part: target, .. } = &link.target {
                    if target.is_relationship_part() {
                        return Err(OpcError::Invalid(
                            "A relationship cannot target a Relationships part".into(),
                        ));
                    }
                    if !package.parts.contains_key(&target.key()) {
                        package.diagnostics.push(PackageDiagnostic {
                            code: DiagnosticCode::DanglingTarget,
                            location: part.name.as_str().into(),
                            message: format!(
                                "{} targets missing part {}",
                                link.id,
                                target.as_str()
                            ),
                        });
                    }
                }
            }
            package.relationships.extend(links);
        }
        package.diagnostics.push(PackageDiagnostic { code: DiagnosticCode::OpaqueDocumentContent, location: "/".into(),
            message: "Package metadata parsed; worksheet/document semantics remain opaque. Editing and round-trip saving are not available in this batch.".into() });
        Ok(package)
    }

    pub fn parts(&self) -> impl Iterator<Item = &PackagePart> {
        self.parts.values()
    }
    pub fn part(&self, name: &PartName) -> Option<&PackagePart> {
        self.parts.get(&name.key())
    }
    pub fn relationships(&self) -> &[Relationship] {
        &self.relationships
    }
    pub fn relationships_from<'a>(
        &'a self,
        source: Option<&'a PartName>,
    ) -> impl Iterator<Item = &'a Relationship> {
        self.relationships
            .iter()
            .filter(move |r| match (&r.source, source) {
                (None, None) => true,
                (Some(a), Some(b)) => a.as_str().eq_ignore_ascii_case(b.as_str()),
                _ => false,
            })
    }
    pub fn incoming<'a>(&'a self, name: &'a PartName) -> impl Iterator<Item = &'a Relationship> {
        self.relationships.iter().filter(move |r| matches!(&r.target, RelationshipTarget::Internal { part, .. } if part.as_str().eq_ignore_ascii_case(name.as_str())))
    }
    pub fn content_types_bytes(&self) -> &[u8] {
        &self.content_types_bytes
    }
    pub fn unmapped_items(&self) -> &[UnmappedZipItem] {
        &self.unmapped_items
    }
    pub fn diagnostics(&self) -> &[PackageDiagnostic] {
        &self.diagnostics
    }

    /// Standard root relationship; never guesses xl/workbook.xml from a filename.
    pub fn office_document(&self) -> Result<Option<&PackagePart>, OpcError> {
        let mut roots = self.relationships_from(None).filter(|r| {
            matches!(
                r.relationship_type.as_str(),
                OFFICE_DOCUMENT_RELATIONSHIP | STRICT_OFFICE_DOCUMENT_RELATIONSHIP
            )
        });
        let Some(root) = roots.next() else {
            return Ok(None);
        };
        if roots.next().is_some() {
            return Err(OpcError::Invalid(
                "Multiple officeDocument package relationships".into(),
            ));
        }
        match &root.target {
            RelationshipTarget::Internal { part, .. } => self
                .part(part)
                .map(Some)
                .ok_or_else(|| OpcError::Invalid("officeDocument target is missing".into())),
            RelationshipTarget::External { .. } => Err(OpcError::Invalid(
                "officeDocument target must be internal".into(),
            )),
        }
    }
}
