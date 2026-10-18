//! Check physical entries before ZipArchive deduplicates identical raw names.
use super::{OpcError, ReadLimits};
use std::collections::BTreeSet;

fn word(bytes: &[u8], position: usize) -> Result<u16, OpcError> {
    let slice = bytes
        .get(position..position + 2)
        .ok_or_else(|| OpcError::Invalid("Truncated ZIP directory".into()))?;
    Ok(u16::from_le_bytes(slice.try_into().unwrap()))
}
fn dword(bytes: &[u8], position: usize) -> Result<u32, OpcError> {
    let slice = bytes
        .get(position..position + 4)
        .ok_or_else(|| OpcError::Invalid("Truncated ZIP directory".into()))?;
    Ok(u32::from_le_bytes(slice.try_into().unwrap()))
}

pub(super) fn preflight(bytes: &[u8], limits: &ReadLimits) -> Result<usize, OpcError> {
    if bytes.len() as u64 > limits.max_archive_bytes {
        return Err(OpcError::Limit("archive bytes"));
    }
    if !bytes.starts_with(b"PK\x03\x04") {
        return Err(OpcError::Unsupported("only ordinary ZIP-based OPC packages are supported; no encrypted OLE container or executable prefix".into()));
    }
    let start = bytes.len().saturating_sub(65557);
    let eocd = (start..bytes.len().saturating_sub(21))
        .rev()
        .find(|&i| {
            bytes.get(i..i + 4) == Some(b"PK\x05\x06")
                && word(bytes, i + 20).is_ok_and(|n| i + 22 + n as usize == bytes.len())
        })
        .ok_or_else(|| OpcError::Invalid("ZIP end directory missing".into()))?;
    if word(bytes, eocd + 4)? != 0
        || word(bytes, eocd + 6)? != 0
        || word(bytes, eocd + 8)? != word(bytes, eocd + 10)?
    {
        return Err(OpcError::Unsupported("multi-volume ZIP packages".into()));
    }
    let count = word(bytes, eocd + 10)? as usize;
    let size = dword(bytes, eocd + 12)? as usize;
    let offset = dword(bytes, eocd + 16)? as usize;
    if count == u16::MAX as usize || size == u32::MAX as usize || offset == u32::MAX as usize {
        return Err(OpcError::Unsupported("ZIP64 packages".into()));
    }
    if count > limits.max_entries {
        return Err(OpcError::Limit("ZIP entries"));
    }
    if offset.checked_add(size) != Some(eocd) {
        return Err(OpcError::Unsupported(
            "non-contiguous or signed ZIP central directory".into(),
        ));
    }
    let mut position = offset;
    let mut names = BTreeSet::new();
    let mut total = 0u64;
    for _ in 0..count {
        if bytes.get(position..position + 4) != Some(b"PK\x01\x02") {
            return Err(OpcError::Invalid("Invalid ZIP central entry".into()));
        }
        if word(bytes, position + 8)? & 1 != 0 {
            return Err(OpcError::Unsupported(
                "ZIP entry encryption is prohibited by OPC".into(),
            ));
        }
        if !matches!(word(bytes, position + 10)?, 0 | 8) {
            return Err(OpcError::Unsupported(
                "ZIP compression other than stored/DEFLATE".into(),
            ));
        }
        let compressed = dword(bytes, position + 20)? as u64;
        let expanded = dword(bytes, position + 24)? as u64;
        if expanded == u32::MAX as u64
            || compressed == u32::MAX as u64
            || dword(bytes, position + 42)? == u32::MAX
        {
            return Err(OpcError::Unsupported("ZIP64 entries".into()));
        }
        if word(bytes, position + 34)? != 0 {
            return Err(OpcError::Unsupported("multi-volume ZIP entries".into()));
        }
        if expanded > limits.max_part_bytes {
            return Err(OpcError::Limit("part bytes"));
        }
        if expanded
            > compressed
                .max(1)
                .saturating_mul(limits.max_compression_ratio)
        {
            return Err(OpcError::Limit("compression ratio"));
        }
        total = total
            .checked_add(expanded)
            .ok_or(OpcError::Limit("expanded package bytes"))?;
        if total > limits.max_total_bytes {
            return Err(OpcError::Limit("expanded package bytes"));
        }
        let name_len = word(bytes, position + 28)? as usize;
        let extra_len = word(bytes, position + 30)? as usize;
        let comment_len = word(bytes, position + 32)? as usize;
        let end = position
            .checked_add(46 + name_len + extra_len + comment_len)
            .filter(|&end| end <= eocd)
            .ok_or_else(|| OpcError::Invalid("ZIP directory entry out of range".into()))?;
        let name = bytes
            .get(position + 46..position + 46 + name_len)
            .ok_or_else(|| OpcError::Invalid("Truncated ZIP name".into()))?;
        if !name.is_ascii() {
            return Err(OpcError::Unsupported(
                "OPC ZIP item names must be ASCII".into(),
            ));
        }
        if !names.insert(name.to_vec()) {
            return Err(OpcError::Invalid("Duplicate physical ZIP item name".into()));
        }
        position = end;
    }
    if position != eocd {
        return Err(OpcError::Invalid(
            "ZIP entry count/directory size mismatch".into(),
        ));
    }
    Ok(count)
}
