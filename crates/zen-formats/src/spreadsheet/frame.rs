//! Byte-bound projection context, not proof of disk publication or an allowed
//! edit. zen-session alone issues its SavedWorkbookReceipt after publication.
use super::{ReadError, ReadWorkbook, WorkbookReadLimits};
use std::sync::Arc;
use zen_opc::opaque_xml::{OpaqueXmlPolicy, sha256};

/// Immutable in-memory context. Retains only a digest, numeric limits and a
/// shared policy; no archive bytes or workbook projection are retained.
/// Not serializable. The caller must establish candidate/edit lineage.
#[derive(Debug, Clone)]
pub struct WorkbookFrameContext {
    frame_sha256: [u8; 32],
    archive_len: usize,
    limits: WorkbookReadLimits,
    policy: Option<Arc<OpaqueXmlPolicy>>,
}
impl WorkbookFrameContext {
    pub fn frame_sha256(&self) -> [u8; 32] {
        self.frame_sha256
    }
    pub fn archive_len(&self) -> usize {
        self.archive_len
    }
    pub fn limits(&self) -> &WorkbookReadLimits {
        &self.limits
    }
    pub fn policy_identity(&self) -> Option<[u8; 32]> {
        self.policy.as_ref().map(|p| p.identity())
    }
    /// Shared descriptor payload only, not exact process heap accounting.
    pub fn retained_policy_descriptor_bytes(&self) -> usize {
        self.policy
            .as_ref()
            .map_or(0, |p| p.retained_descriptor_bytes())
    }
    pub fn read(&self, bytes: &[u8]) -> Result<ReadWorkbook, ReadError> {
        if bytes.len() != self.archive_len || sha256(bytes) != self.frame_sha256 {
            return Err(ReadError::FrameIdentity);
        }
        let mut result = ReadWorkbook::read(bytes, &self.limits)?;
        if let Some(policy) = &self.policy {
            policy.validate_candidate(bytes)?;
        }
        result.opaque_xml_policy = self.policy.clone();
        Ok(result)
    }
}
impl ReadWorkbook {
    /// Validates a candidate under explicit limits and records its exact bytes.
    /// This creates a projection context only, never a saved-file receipt.
    pub fn prepare_frame_reopen(
        &self,
        bytes: &[u8],
        limits: &WorkbookReadLimits,
    ) -> Result<WorkbookFrameContext, ReadError> {
        self.reread_candidate(bytes, limits)?;
        Ok(WorkbookFrameContext {
            frame_sha256: sha256(bytes),
            archive_len: bytes.len(),
            limits: limits.clone(),
            policy: self.opaque_xml_policy.clone(),
        })
    }
}
