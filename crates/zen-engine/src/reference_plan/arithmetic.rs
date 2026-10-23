//! Syntax only for point-reference arithmetic: no evaluation or edit grant.
use super::binding::BoundFormula;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ArithmeticError {
    #[error("arithmetic token budget exceeded")]
    TokenBudget,
    #[error("parentheses exceed the fixed 64-level budget")]
    Nesting,
    #[error("unexpected arithmetic token at byte {0}")]
    Unexpected(usize),
    #[error("incomplete expression or unclosed parentheses")]
    Incomplete,
}
/// Validate operand/operator sequencing for point references, decimal/scientific
/// numbers, unary +/- and binary +-*/^ with parentheses. No AST or precedence
/// evaluation is produced. Functions, strings, comparisons, %, & and ranges
/// remain refused. A successful check is not calculator or structure admission.
/// Original UTF-8 source/spans and stable bindings are borrowed, never changed.
/// Iterative scan, no recursive descent/allocation; max_tokens is capped at4096.
pub fn validate_arithmetic(
    formula: &BoundFormula<'_, '_>,
    max_tokens: usize,
) -> Result<(), ArithmeticError> {
    let source = formula.source().as_bytes();
    let points = formula.points();
    let mut i = 0;
    while source.get(i).is_some_and(u8::is_ascii_whitespace) {
        i += 1;
    }
    if source.get(i) == Some(&b'=') {
        i += 1;
    }
    let mut expect_operand = true;
    let mut depth = 0;
    let mut tokens = 0;
    let mut reference = 0;
    while i < source.len() {
        if source[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if tokens >= max_tokens.min(4096) {
            return Err(ArithmeticError::TokenBudget);
        }
        tokens += 1;
        if expect_operand {
            if matches!(source[i], b'+' | b'-') {
                i += 1;
                continue;
            }
            if source[i] == b'(' {
                if depth == 64 {
                    return Err(ArithmeticError::Nesting);
                }
                depth += 1;
                i += 1;
                continue;
            }
            if let Some(point) = points.get(reference) {
                if point.span().source.start == i {
                    i = point.span().source.end;
                    reference += 1;
                    expect_operand = false;
                    continue;
                }
            }
            if source[i].is_ascii_digit()
                || (source[i] == b'.' && source.get(i + 1).is_some_and(u8::is_ascii_digit))
            {
                while source.get(i).is_some_and(u8::is_ascii_digit) {
                    i += 1;
                }
                if source.get(i) == Some(&b'.') {
                    i += 1;
                    while source.get(i).is_some_and(u8::is_ascii_digit) {
                        i += 1;
                    }
                }
                if matches!(source.get(i), Some(b'e' | b'E')) {
                    i += 1;
                    if matches!(source.get(i), Some(b'+' | b'-')) {
                        i += 1;
                    }
                    let exponent = i;
                    while source.get(i).is_some_and(u8::is_ascii_digit) {
                        i += 1;
                    }
                    if i == exponent {
                        return Err(ArithmeticError::Unexpected(i));
                    }
                }
                expect_operand = false;
                continue;
            }
            return Err(ArithmeticError::Unexpected(i));
        }
        if b"+-*/^".contains(&source[i]) {
            i += 1;
            expect_operand = true;
        } else if source[i] == b')' && depth > 0 {
            depth -= 1;
            i += 1;
        } else {
            return Err(ArithmeticError::Unexpected(i));
        }
    }
    if expect_operand || depth != 0 || reference != points.len() {
        return Err(ArithmeticError::Incomplete);
    }
    Ok(())
}
