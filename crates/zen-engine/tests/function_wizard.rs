use zen_engine::{
    calc::{CellCoord, CellValue},
    sheet::{BasicFunction, Sheet},
};

fn cell(address: &str) -> CellCoord {
    CellCoord::from_a1(address).unwrap()
}
fn sample() -> Sheet {
    let mut sheet = Sheet::new("测试", 8, 8);
    sheet
        .set_cells_bulk(&[
            (cell("A1"), "10".into()),
            (cell("A2"), "20".into()),
            (cell("A3"), "30".into()),
        ])
        .unwrap();
    sheet
}

#[test]
fn all_five_functions_preview_without_mutating_source() {
    let sheet = sample();
    for (function, expected) in [
        (BasicFunction::Sum, 60.0),
        (BasicFunction::Average, 20.0),
        (BasicFunction::Count, 3.0),
        (BasicFunction::Max, 30.0),
        (BasicFunction::Min, 10.0),
    ] {
        let (candidate, formula, value) = sheet
            .function_candidate(cell("B4"), function, "A1:A3")
            .unwrap();
        assert_eq!(value, expected);
        assert_eq!(candidate.get_raw(cell("B4")), formula);
        assert_eq!(sheet.get_raw(cell("B4")), "");
        assert_eq!(sheet.get_value(cell("A1")), CellValue::Number(10.0));
    }
}

#[test]
fn normalized_arguments_and_existing_format_survive() {
    let mut sheet = sample();
    sheet.update_styles(&[cell("B4")], |style| {
        style.bold = true;
        style.wrap = true;
    });
    let (candidate, formula, value) = sheet
        .function_candidate(cell("B4"), BasicFunction::Sum, " a3:a1 ； 5，-2 ")
        .unwrap();
    assert_eq!(formula, "=SUM(A1:A3,5,-2)");
    assert_eq!(value, 63.0);
    assert!(candidate.style(cell("B4")).bold && candidate.style(cell("B4")).wrap);
}

#[test]
fn invalid_references_and_unsupported_syntax_are_rejected() {
    let sheet = sample();
    for input in [
        "",
        "A0",
        "I1",
        "A9",
        "A1:B9",
        "A1:",
        "A1:B2:C3",
        "$A$1",
        "Sheet2!A1",
        "SUM(A1:A3)",
        "A1+1",
        "A1,,A2",
        "中文1",
    ] {
        assert!(
            sheet
                .function_candidate(cell("B4"), BasicFunction::Sum, input)
                .is_err(),
            "{input}"
        );
    }
    assert!(
        sheet
            .function_candidate(cell("I1"), BasicFunction::Sum, "A1")
            .is_err()
    );
}

#[test]
fn direct_and_indirect_cycles_are_rejected_atomically() {
    let mut sheet = sample();
    for input in ["B4", "A1:B4"] {
        assert!(
            sheet
                .function_candidate(cell("B4"), BasicFunction::Sum, input)
                .is_err()
        );
    }
    sheet.set_cell(cell("A1"), "=B4");
    let error = sheet
        .function_candidate(cell("B4"), BasicFunction::Sum, "A1")
        .err()
        .unwrap();
    assert!(error.contains("CYCLE"), "{error}");
    assert_eq!(sheet.get_raw(cell("B4")), "");
}

#[test]
fn source_error_and_nonfinite_results_cannot_be_committed() {
    let mut sheet = sample();
    sheet.set_cell(cell("A1"), "=AVERAGE(C1:C2)");
    assert!(
        sheet
            .function_candidate(cell("B4"), BasicFunction::Sum, "A1")
            .is_err()
    );
    for input in ["NaN", "inf", "1e309", "1e308,1e308"] {
        assert!(
            sheet
                .function_candidate(cell("B4"), BasicFunction::Sum, input)
                .is_err(),
            "{input}"
        );
    }
}

#[test]
fn argument_and_range_budgets_are_enforced() {
    let sheet = sample();
    assert!(
        sheet
            .build_function_formula(cell("B4"), BasicFunction::Sum, &vec!["1"; 31].join(","))
            .is_err()
    );
    assert!(
        sheet
            .build_function_formula(cell("B4"), BasicFunction::Sum, &"1".repeat(4097))
            .is_err()
    );
    let large = Sheet::new("容量", 100_002, 2);
    assert!(
        large
            .build_function_formula(cell("B1"), BasicFunction::Sum, "A1:A100001")
            .is_err()
    );
    assert!(
        large
            .build_function_formula(cell("B1"), BasicFunction::Sum, "A1:A100000,A1")
            .is_err()
    );
}

#[test]
fn snapshot_restore_preserves_formula_and_page_settings() {
    use zen_engine::sheet::{PageOrientation, PaperSize};
    let mut sheet = sample();
    sheet.set_page_orientation(PageOrientation::Landscape);
    sheet.set_paper_size(PaperSize::A3);
    let previous = sheet.clone();
    let (next, _, _) = sheet
        .function_candidate(cell("B4"), BasicFunction::Sum, "A1:A3")
        .unwrap();
    let restored = previous.clone();
    assert_eq!(restored.get_raw(cell("B4")), "");
    assert_eq!(next.get_raw(cell("B4")), "=SUM(A1:A3)");
    assert_eq!(next.page_settings(), restored.page_settings());
}
