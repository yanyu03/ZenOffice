use zen_engine::reference_plan::syntax::{ScanError, scan_points};

#[test]
fn retains_literal_spans_case_flags_and_decoded_qualifiers() {
    let f = "= $a$1 + Sheet2!B$3 + 'O''Brien 表'!$c4 ";
    let r = scan_points(f, 3).unwrap();
    let expected = [
        ("$a$1", "$a$1", None, 0, 0, true, true),
        ("Sheet2!B$3", "B$3", Some("Sheet2"), 2, 1, false, true),
        (
            "'O''Brien 表'!$c4",
            "$c4",
            Some("O'Brien 表"),
            3,
            2,
            true,
            false,
        ),
    ];
    assert_eq!(r.len(), expected.len());
    for (p, (whole, addr, sheet, row, col, cf, rf)) in r.iter().zip(expected) {
        assert_eq!(&f[p.source.clone()], whole);
        assert_eq!(&f[p.address.clone()], addr);
        assert_eq!(p.sheet.as_deref(), sheet);
        assert_eq!((p.cell.row(), p.cell.col()), (row, col));
        assert_eq!((p.column_fixed, p.row_fixed), (cf, rf));
    }
    // Original byte offsets, including UTF-8 before the final address.
    assert_eq!(r[0].source, 2..6);
    assert_eq!(r[1].source, 9..19);
    assert_eq!(r[2].address.end, f.len() - 1);
}
#[test]
fn ignores_reference_lookalikes_in_escaped_strings() {
    let f = r#"=IF(A1="B2""'假表'!$C$3", "D4", B5)"#;
    let r = scan_points(f, 2).unwrap();
    assert_eq!(
        r.iter().map(|p| &f[p.source.clone()]).collect::<Vec<_>>(),
        ["A1", "B5"]
    );
    assert_eq!(scan_points("=\"A1", 0), Err(ScanError::Quoting(1)));
    assert_eq!(scan_points("=\"A1\"\"B2", 0), Err(ScanError::Quoting(1)));
}
#[test]
fn separates_scientific_numbers_and_function_shaped_identifiers() {
    let f = "=LOG10(A1)+SUM(B2,$C3)+1E10+.5e-2+2.";
    let r = scan_points(f, 3).unwrap();
    assert_eq!(
        r.iter().map(|p| &f[p.source.clone()]).collect::<Vec<_>>(),
        ["A1", "B2", "$C3"]
    );
    // This API deliberately does not validate formula grammar or function support.
    assert!(scan_points("=UNKNOWN(A1)", 1).is_ok());
    assert_eq!(scan_points("A1 B2", 2).unwrap().len(), 2);
}
#[test]
fn refuses_unhandled_owners_and_reference_syntax_without_partial_result() {
    for f in [
        "=A1+B2:C3",
        "=A1+Name",
        "=[Book]Sheet!A1",
        "='[Book]Sheet'!A1",
        "=Sheet1:Sheet2!A1",
        "='Sheet1:Sheet2'!A1",
        "=Table[Column]",
        "={A1,B2}",
        "=@A1",
        "=A1#",
        "=#REF!",
        "=R1C1",
        "=TRUE",
        "=Sheet !A1",
        "=表!A1",
        "=A1+1E+",
        "=1A1",
        "=A1+?",
    ] {
        assert!(scan_points(f, 8).is_err(), "{f}");
    }
}
#[test]
fn refuses_invalid_addresses_and_quotes() {
    for f in [
        "=A0",
        "=A01",
        "=XFE1",
        "=A1048577",
        "=$A",
        "=A$",
        "=A1foo",
        "=A1.2",
        "='bad",
        "='x'A1",
        "=''!A1",
        "='x'!",
        "=Sheet!$",
        "='bad/name'!A1",
        "='$x'!A1!B2",
        "=$A1(",
    ] {
        assert!(scan_points(f, 8).is_err(), "{f}");
    }
    let r = scan_points("=$XFD$1048576", 1).unwrap();
    assert_eq!((r[0].cell.row(), r[0].cell.col()), (1_048_575, 16_383));
}
#[test]
fn enforces_byte_and_reference_budgets() {
    assert_eq!(scan_points("=A1+B2", 1), Err(ScanError::ReferenceBudget));
    assert_eq!(scan_points("=A1", 0), Err(ScanError::ReferenceBudget));
    assert!(scan_points("=42", 0).unwrap().is_empty());
    assert_eq!(
        scan_points(&" ".repeat(8193), usize::MAX),
        Err(ScanError::FormulaBudget)
    );
    assert!(scan_points(&" ".repeat(8192), 0).unwrap().is_empty());
    assert_eq!(
        scan_points(&format!("=\"{}\"", "表".repeat(2730)), 0),
        Err(ScanError::FormulaBudget)
    );
}
#[test]
fn unicode_boundaries_never_panic_or_split_a_reference() {
    for prefix in ["", "=", "=A1+", "='表''单'!A1+"] {
        for token in ["表", "😀", "é", "́", "\u{2003}"] {
            assert!(scan_points(&format!("{prefix}{token}"), 8).is_err());
        }
    }
    let f = "=\"😀A1\"+'表单'!a1";
    let r = scan_points(f, 1).unwrap();
    assert_eq!(&f[r[0].source.clone()], "'表单'!a1");
}
