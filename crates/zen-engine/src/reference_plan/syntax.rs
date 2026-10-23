//! Bounded lexical observations for a deliberately narrow formula vocabulary.
//! Success is NOT formula grammar/evaluation validation, sheet binding, or
//! structural-edit admission. No writer consumes this module yet.
use crate::workbook::CellAddress;
use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PointSpan {
    /// Full reference including explicit sheet qualifier, in UTF-8 bytes.
    pub source: Range<usize>,
    /// Address alone; original case and dollar signs remain in source.
    pub address: Range<usize>,
    /// None means implicit owner sheet. Some is decoded, NOT a bound SheetId.
    pub sheet: Option<String>,
    pub cell: CellAddress,
    pub column_fixed: bool,
    pub row_fixed: bool,
}
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ScanError {
    #[error("formula exceeds the fixed 8192-byte lexical budget")]
    FormulaBudget,
    #[error("reference count exceeds caller budget")]
    ReferenceBudget,
    #[error("unsupported or ambiguous lexical token at byte {0}")]
    Unsupported(usize),
    #[error("unclosed or invalid quoted token at byte {0}")]
    Quoting(usize),
    #[error("invalid or out-of-grid A1 address at byte {0}")]
    Address(usize),
    #[error("cannot allocate bounded lexical observations")]
    Allocation,
}
fn word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'$')
}
fn decoded(s: &str, quoted: bool) -> Result<String, ScanError> {
    let mut out = String::new();
    out.try_reserve_exact(s.len())
        .map_err(|_| ScanError::Allocation)?;
    if quoted {
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            out.push(c);
            if c == '\'' && chars.peek() == Some(&'\'') {
                chars.next();
            }
        }
    } else {
        out.push_str(s);
    }
    Ok(out)
}
fn address(s: &str, at: usize) -> Result<(CellAddress, bool, bool), ScanError> {
    let b = s.as_bytes();
    let mut i = 0;
    let cf = b.get(i) == Some(&b'$');
    i += usize::from(cf);
    let start = i;
    let mut col = 0u32;
    while b.get(i).is_some_and(u8::is_ascii_alphabetic) {
        col = col
            .checked_mul(26)
            .and_then(|v| v.checked_add(u32::from(b[i].to_ascii_uppercase() - b'A' + 1)))
            .filter(|v| *v <= 16_384)
            .ok_or(ScanError::Address(at))?;
        i += 1;
    }
    if i == start {
        return Err(ScanError::Address(at));
    }
    let rf = b.get(i) == Some(&b'$');
    i += usize::from(rf);
    let start = i;
    if b.get(i) == Some(&b'0') {
        return Err(ScanError::Address(at));
    }
    let mut row = 0u32;
    while b.get(i).is_some_and(u8::is_ascii_digit) {
        row = row
            .checked_mul(10)
            .and_then(|v| v.checked_add(u32::from(b[i] - b'0')))
            .filter(|v| *v <= 1_048_576)
            .ok_or(ScanError::Address(at))?;
        i += 1;
    }
    if i != b.len() || i == start || row == 0 {
        return Err(ScanError::Address(at));
    }
    let cell = CellAddress::new(row - 1, (col - 1) as u16).map_err(|_| ScanError::Address(at))?;
    Ok((cell, cf, rf))
}
/// Returns no observations if any token fails. The source remains borrowed.
/// Success is lexical only: "A1 B2" may scan but is not admitted as a formula.
/// Function-shaped identifiers followed by '(' are not checked for support.
/// Ranges/names are refused. Byte limit bounds qualifier sizes; max_refs
/// counts references, not total heap. Allocation failure is not injected.
pub fn scan_points(formula: &str, max_refs: usize) -> Result<Vec<PointSpan>, ScanError> {
    if formula.len() > 8192 {
        return Err(ScanError::FormulaBudget);
    }
    let b = formula.as_bytes();
    let mut refs = Vec::new();
    refs.try_reserve_exact(max_refs.min(formula.len()))
        .map_err(|_| ScanError::Allocation)?;
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_whitespace() || b"=+-*/^&(),<>%".contains(&b[i]) {
            i += 1;
            continue;
        }
        if b[i] == b'"' {
            let start = i;
            i += 1;
            loop {
                if i == b.len() {
                    return Err(ScanError::Quoting(start));
                }
                if b[i] == b'"' {
                    i += 1;
                    if b.get(i) == Some(&b'"') {
                        i += 1;
                    } else {
                        break;
                    }
                } else {
                    i += 1;
                }
            }
            continue;
        }
        if b[i].is_ascii_digit() || (b[i] == b'.' && b.get(i + 1).is_some_and(u8::is_ascii_digit)) {
            let start = i;
            while b.get(i).is_some_and(u8::is_ascii_digit) {
                i += 1;
            }
            if b.get(i) == Some(&b'.') {
                i += 1;
                while b.get(i).is_some_and(u8::is_ascii_digit) {
                    i += 1;
                }
            }
            if matches!(b.get(i), Some(b'e' | b'E')) {
                i += 1;
                if matches!(b.get(i), Some(b'+' | b'-')) {
                    i += 1;
                }
                let exp = i;
                while b.get(i).is_some_and(u8::is_ascii_digit) {
                    i += 1;
                }
                if i == exp {
                    return Err(ScanError::Unsupported(start));
                }
            }
            if b.get(i).is_some_and(|v| word(*v) || !v.is_ascii()) {
                return Err(ScanError::Unsupported(start));
            }
            continue;
        }
        let start = i;
        let mut sheet = None;
        if b[i] == b'\'' {
            i += 1;
            let content = i;
            let end;
            loop {
                if i == b.len() {
                    return Err(ScanError::Quoting(start));
                }
                if b[i] == b'\'' {
                    if b.get(i + 1) == Some(&b'\'') {
                        i += 2;
                    } else {
                        end = i;
                        i += 1;
                        break;
                    }
                } else {
                    i += 1;
                }
            }
            if end == content || b.get(i) != Some(&b'!') {
                return Err(ScanError::Quoting(start));
            }
            // No external/3D or invalid sheet-name characters inside quotes.
            // This is not a complete sheet-name validator.
            if formula[content..end]
                .chars()
                .any(|c| "[]:*?/\\".contains(c) || c.is_control())
            {
                return Err(ScanError::Unsupported(start));
            }
            sheet = Some(decoded(&formula[content..end], true)?);
            i += 1;
        } else if b[i].is_ascii_alphabetic() || b[i] == b'_' || b[i] == b'$' {
            while b.get(i).is_some_and(|v| word(*v)) {
                i += 1;
            }
            if b.get(i) == Some(&b'!') {
                let raw = &formula[start..i];
                if raw.contains('$') {
                    return Err(ScanError::Unsupported(start));
                }
                sheet = Some(decoded(raw, false)?);
                i += 1;
            } else {
                let mut next = i;
                while b.get(next).is_some_and(u8::is_ascii_whitespace) {
                    next += 1;
                }
                if b.get(next) == Some(&b'(') {
                    if formula[start..i].contains('$') {
                        return Err(ScanError::Unsupported(start));
                    }
                    continue; // Lexical function shape, not function support.
                }
                i = start;
            }
        } else {
            return Err(ScanError::Unsupported(i));
        }
        let addr = i;
        while b.get(i).is_some_and(|v| word(*v)) {
            i += 1;
        }
        if b.get(i).is_some_and(|v| !v.is_ascii()) {
            return Err(ScanError::Unsupported(i));
        }
        let (cell, column_fixed, row_fixed) = address(&formula[addr..i], addr)?;
        if refs.len() >= max_refs {
            return Err(ScanError::ReferenceBudget);
        }
        refs.push(PointSpan {
            source: start..i,
            address: addr..i,
            sheet,
            cell,
            column_fixed,
            row_fixed,
        });
    }
    Ok(refs)
}
