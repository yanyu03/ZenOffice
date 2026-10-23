use std::collections::BTreeMap;
use zen_engine::{
    reference_plan::{
        arithmetic::{ArithmeticError, validate_arithmetic},
        binding::bind_points,
    },
    workbook::{DateSystem, SheetId, SheetKind, SheetVisibility, Workbook, Worksheet},
};
fn book() -> Workbook {
    Workbook::new(
        vec![
            Worksheet::new(
                SheetId::new(8).unwrap(),
                "O'Brien 表".into(),
                SheetVisibility::Visible,
                SheetKind::Worksheet,
                BTreeMap::new(),
            )
            .unwrap(),
        ],
        DateSystem::Excel1900,
    )
    .unwrap()
}
fn check(f: &str, budget: usize) -> Result<(), ArithmeticError> {
    let w = book();
    let b = bind_points(f, &w, SheetId::new(8).unwrap(), 1000, 1).unwrap();
    validate_arithmetic(&b, budget)
}
#[test]
fn accepts_bounded_arithmetic_without_evaluating_precedence() {
    for f in [
        "=A1+2*(B2-3)^-2",
        "--A1+-B2",
        "=1E+2+.25e-3-2.",
        " ( ( A1 ) ) ",
        "=1/0",
        "=1e999",
        "=A1^B2^C3",
    ] {
        assert_eq!(check(f, 100), Ok(()), "{f}");
    }
}
#[test]
fn keeps_quoted_unicode_reference_as_one_operand() {
    let w = book();
    let f = "= -'O''Brien 表'!$a$1 + $B2";
    let b = bind_points(f, &w, SheetId::new(8).unwrap(), 2, 1).unwrap();
    assert_eq!(validate_arithmetic(&b, 4), Ok(()));
    assert_eq!(b.reference_text(0), Some("'O''Brien 表'!$a$1"));
    assert_eq!(b.points()[0].anchor().sheet, SheetId::new(8).unwrap());
}
#[test]
fn refuses_missing_operands_adjacent_expressions_and_unbalanced_parens() {
    let w = book();
    assert!(bind_points("=1.2.3", &w, SheetId::new(8).unwrap(), 2, 1).is_err());

    for f in [
        "",
        "=",
        "=+",
        "=()",
        "=A1+",
        "=A1**B2",
        "=A1 B2",
        "=A1(B2)",
        "=(A1",
        "=A1)",
        "=1 2",
        "=A1= B2",
        "=A1+()",
        "=(A1)(B2)",
    ] {
        assert!(check(f, 100).is_err(), "{f}");
    }
}
#[test]
fn explicitly_refuses_functions_strings_and_non_arithmetic_operators() {
    for f in [
        "=SUM(A1)",
        "=UNKNOWN(A1)",
        "=\"A1\"",
        "=A1&B2",
        "=A1%",
        "=A1>B2",
        "=A1,B2",
    ] {
        assert!(check(f, 100).is_err(), "{f}");
    }
}
#[test]
fn token_budget_counts_unary_parentheses_numbers_and_references() {
    assert_eq!(check("=-A1+2", 4), Ok(()));
    assert_eq!(check("=-A1+2", 3), Err(ArithmeticError::TokenBudget));
    assert_eq!(check("=1", 0), Err(ArithmeticError::TokenBudget));
    let f = format!("{}1", "-".repeat(4096));
    assert_eq!(check(&f, usize::MAX), Err(ArithmeticError::TokenBudget));
}
#[test]
fn fixed_nesting_cap_is_iterative_and_exact() {
    let yes = format!("{}A1{}", "(".repeat(64), ")".repeat(64));
    assert_eq!(check(&yes, 129), Ok(()));
    let no = format!("{}A1{}", "(".repeat(65), ")".repeat(65));
    assert_eq!(check(&no, 200), Err(ArithmeticError::Nesting));
}
#[test]
fn refusal_does_not_change_source_points_or_workbook() {
    let w = book();
    let f = "=A1 B2";
    let b = bind_points(f, &w, SheetId::new(8).unwrap(), 2, 1).unwrap();
    let before = b.points().iter().map(|p| p.anchor()).collect::<Vec<_>>();
    assert!(validate_arithmetic(&b, 10).is_err());
    assert_eq!(b.source(), f);
    assert_eq!(
        before,
        b.points().iter().map(|p| p.anchor()).collect::<Vec<_>>()
    );
    assert!(w.sheets()[0].cells().is_empty());
}
