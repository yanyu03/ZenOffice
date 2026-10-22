//! Explicit-policy Windows business-v2 API execution; native Office NOT_RUN. Literal expectations come from
//! the sealed predeclared plan, never from Zen or native output. Input hashes
//! must first be verified by verify_typed_inputs.py (also wired into core CI).
#![cfg(windows)]
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use zen_engine::workbook::{CellAddress, DateSystem, TypedValue};
use zen_formats::spreadsheet::{ReadWorkbook, WorkbookReadLimits};
use zen_session::{EditOutcome, HistoryBudget, WorkbookSession};

use std::sync::Arc;
use zen_opc::opaque_xml::{OpaqueXmlDeclaration, OpaqueXmlPolicy, sha256};

const PLAN: &str = include_str!("../../../RCD/engine-acceptance/eng-005-business-v2/plan.json");
const PIN: &str = "bc82f786f0db1c842188dbdb891b4b6ef3c2f0af5f4c026fe4ae19bf428aaad4";
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}
fn address(s: &str) -> CellAddress {
    let split = s.find(|c: char| c.is_ascii_digit()).unwrap();
    let mut col = 0u16;
    for c in s[..split].bytes() {
        col = col.checked_mul(26).unwrap() + u16::from(c - b'A' + 1);
    }
    CellAddress::new(s[split..].parse::<u32>().unwrap() - 1, col - 1).unwrap()
}
fn write_new(p: &Path, bytes: &[u8]) {
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(p)
        .unwrap();
    f.write_all(bytes).unwrap();
    f.sync_all().unwrap();
}
#[derive(Clone)]
struct Want {
    value: Value,
    formula: Option<String>,
}
type Snapshot = BTreeMap<(String, String), Want>;
fn key(sheet: &str, cell: &str) -> (String, String) {
    match cell.split_once('!') {
        Some((s, c)) => (s.into(), c.into()),
        None => (sheet.into(), cell.into()),
    }
}
fn check(s: &WorkbookSession, wanted: &Snapshot, source: &[u8], spec: &Value) -> usize {
    for ((sheet, cell), want) in wanted {
        let got = s
            .workbook()
            .sheets()
            .iter()
            .find(|x| x.name() == sheet)
            .unwrap()
            .cell(address(cell))
            .unwrap();
        match &want.value {
            Value::Null => assert!(got.value.is_none(), "{sheet}!{cell}: {:?}", got.value),
            Value::Number(n) => match &got.value {
                Some(TypedValue::Number { value, .. }) => {
                    assert_eq!(*value, n.as_f64().unwrap(), "{sheet}!{cell}")
                }
                other => panic!("{sheet}!{cell}: {other:?}"),
            },
            Value::String(v) => assert!(
                matches!(&got.value,Some(TypedValue::Text(t)) if t==v),
                "{sheet}!{cell}: {:?}",
                got.value
            ),
            Value::Bool(v) => assert!(
                matches!(&got.value,Some(TypedValue::Boolean(b)) if b==v),
                "{sheet}!{cell}: {:?}",
                got.value
            ),
            _ => panic!("unsupported independent expected value"),
        }
        assert_eq!(
            got.formula.as_ref().map(|f| f.text.as_str()),
            want.formula.as_deref(),
            "{sheet}!{cell} formula"
        );
    }
    let before = ReadWorkbook::read(source, &WorkbookReadLimits::default()).unwrap();
    let after = s.read_workbook();
    assert_eq!(
        before.workbook().date_system(),
        after.workbook().date_system()
    );
    assert_eq!(
        before.package().parts().count(),
        after.package().parts().count()
    );
    assert_eq!(
        before.package().content_types_bytes(),
        after.package().content_types_bytes()
    );
    for part in before.package().parts() {
        if !["/xl/worksheets/sheet1.xml", "/xl/worksheets/sheet2.xml"]
            .contains(&part.name().as_str())
        {
            assert_eq!(
                part.bytes(),
                after.package().part(part.name()).unwrap().bytes(),
                "preserve {}",
                part.name().as_str()
            );
        }
    }
    let sheet = spec["sheet"].as_str().unwrap();
    let date = address(spec["date_cell"].as_str().unwrap());
    assert_eq!(
        after
            .workbook()
            .sheets()
            .iter()
            .find(|s| s.name() == sheet)
            .unwrap()
            .cell(date)
            .unwrap()
            .style_index,
        Some(1)
    );
    wanted.len()
}
fn owned_policy(source: &[u8], basis: &str) -> Arc<OpaqueXmlPolicy> {
    let zip = zen_opc::ZipArchive::read(source).unwrap();
    let payload = zip.get("customXml/zen-probe.xml").unwrap();
    let declaration = OpaqueXmlDeclaration {
        source_sha256: sha256(source),
        part_name: "/customXml/zen-probe.xml".into(),
        payload_sha256: sha256(payload),
        payload_bytes: payload.len(),
        content_type: "application/xml".into(),
        relationship_id: "rIdProbe".into(),
        relationship_target: "../customXml/zen-probe.xml".into(),
        controlled_owned_source: true,
        semantically_independent: true,
        review_basis: format!("Owned author fixture; original034audit and sealed plan. {basis}"),
    };
    Arc::new(OpaqueXmlPolicy::bind_initial(source, &declaration).unwrap())
}
fn run(id: &str) {
    let fixtures = root().join("RCD/engine-acceptance/eng-005-business-v2");
    let manifest_bytes = fs::read(fixtures.join("manifest.json")).unwrap();
    let hex = |bytes: &[u8]| {
        sha256(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    };
    assert_eq!(hex(&manifest_bytes), PIN, "sealed manifest identity");
    let manifest: Value = serde_json::from_slice(&manifest_bytes).unwrap();
    for (file, expected) in manifest["sha256"].as_object().unwrap() {
        assert_eq!(
            hex(&fs::read(fixtures.join(file)).unwrap()),
            expected.as_str().unwrap(),
            "sealed {file}"
        );
    }

    assert_eq!(
        fs::read(fixtures.join("plan.json")).unwrap(),
        PLAN.as_bytes()
    );
    let source = match id {
        "budget" => {
            include_bytes!("../../../RCD/engine-acceptance/eng-005-business-v2/budget.xlsx")
                .as_slice()
        }
        "orders" => {
            include_bytes!("../../../RCD/engine-acceptance/eng-005-business-v2/orders.xlsx")
                .as_slice()
        }
        "attendance" => {
            include_bytes!("../../../RCD/engine-acceptance/eng-005-business-v2/attendance.xlsx")
                .as_slice()
        }
        _ => panic!("unknown case"),
    };
    assert_eq!(
        fs::read(fixtures.join(format!("{id}.xlsx"))).unwrap(),
        source
    );
    let plan: Value = serde_json::from_str(PLAN).unwrap();
    let spec = plan["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap();
    let sheet = spec["sheet"].as_str().unwrap();
    let system = if spec["date_system"] == "Excel1904" {
        DateSystem::Excel1904
    } else {
        DateSystem::Excel1900
    };
    let mut wanted = Snapshot::new();
    for group in ["numbers", "labels"] {
        for (cell, value) in spec[group].as_object().unwrap() {
            wanted.insert(
                key(sheet, cell),
                Want {
                    value: value.clone(),
                    formula: None,
                },
            );
        }
    }
    for (cell, value) in spec["initial_expected"].as_object().unwrap() {
        let k = key(sheet, cell);
        let formulas = if k.0 == "Summary" {
            &spec["summary_formulas"]
        } else {
            &spec["formulas"]
        };
        wanted.insert(
            k.clone(),
            Want {
                value: value.clone(),
                formula: Some(formulas[&k.1].as_str().unwrap().into()),
            },
        );
    }
    let parent = root().join("target/typed-business");
    fs::create_dir_all(&parent).unwrap();
    let dir = tempfile::Builder::new()
        .prefix(&format!("{id}-"))
        .tempdir_in(&parent)
        .unwrap()
        .keep();
    let input = dir.join("source.xlsx");
    write_new(&input, source);
    let policy = owned_policy(source, id);
    let policy_id = Some(policy.identity());
    let mut s = WorkbookSession::open_with_opaque_xml_policy(
        &input,
        &WorkbookReadLimits::default(),
        HistoryBudget::default(),
        Arc::clone(&policy),
    )
    .unwrap();
    let mut saved = source.to_vec();
    let mut frames = vec![source.to_vec()];
    let mut revision = 0u64;
    let mut assertions = check(&s, &wanted, source, spec);
    let mut records = vec![];
    for (index, op) in spec["operations"].as_array().unwrap().iter().enumerate() {
        let before = s.bytes().to_vec();
        let previous = wanted.clone();
        let cell = op["cell"].as_str().unwrap();
        let a = address(cell);
        let kind = op["kind"].as_str().unwrap();
        let result = match kind {
            "text" => s.set_value(
                sheet,
                a,
                Some(TypedValue::Text(op["value"].as_str().unwrap().into())),
            ),
            "boolean" => s.set_value(
                sheet,
                a,
                Some(TypedValue::Boolean(op["value"].as_bool().unwrap())),
            ),
            "blank" => s.set_value(sheet, a, None),
            "date_serial" => s.set_date_serial(sheet, a, op["value"].as_f64().unwrap(), system),
            "formula" => s.set_formula(sheet, a, op["value"].as_str().unwrap()),
            "number" => s.set_number(sheet, a, op["value"].as_f64().unwrap()),
            "clear_formula" => s.clear_formula(sheet, a),
            _ => panic!("unhandled operation"),
        };
        assert_eq!(
            result.unwrap_or_else(|e| panic!("{id} {}: {e}", op["id"])),
            EditOutcome::Changed
        );
        revision += 1;
        assert_eq!(s.revision(), revision);
        let mut new = Want {
            value: op.get("value").cloned().unwrap_or(Value::Null),
            formula: None,
        };
        if kind == "formula" {
            new.formula = Some(op["value"].as_str().unwrap().into());
            new.value = wanted[&key(sheet, cell)].value.clone();
        }
        wanted.insert(key(sheet, cell), new);
        for (cell, value) in op["expected"].as_object().unwrap() {
            wanted.get_mut(&key(sheet, cell)).unwrap().value = value.clone();
        }
        assert_eq!(s.opaque_xml_policy_identity(), policy_id);
        let after = s.bytes().to_vec();
        frames.push(after.clone());
        assertions += check(&s, &wanted, source, spec);
        assert_eq!(s.history_frames(), frames.len());
        assert_eq!(
            s.retained_archive_bytes(),
            frames.iter().map(Vec::len).sum::<usize>()
        );
        assert_eq!(s.is_dirty(), after != saved);
        s.undo().unwrap();
        revision += 1;
        assert_eq!(s.revision(), revision);
        assert_eq!(s.bytes(), before);
        assert_eq!(s.opaque_xml_policy_identity(), policy_id);
        assertions += check(&s, &previous, source, spec);
        assert!(s.can_redo());
        assert_eq!(s.is_dirty(), before != saved);
        s.redo().unwrap();
        revision += 1;
        assert_eq!(s.revision(), revision);
        assert_eq!(s.bytes(), after);
        assert_eq!(s.opaque_xml_policy_identity(), policy_id);
        assertions += check(&s, &wanted, source, spec);
        assert!(!s.can_redo());
        write_new(&dir.join(format!("state-{}.xlsx", index + 1)), s.bytes());
        if [3, 4, 7].contains(&index) {
            let output = dir.join(format!("saved-{}.xlsx", index + 1));
            s.save_as_new(&output).unwrap();
            saved = s.bytes().to_vec();
            assert!(!s.is_dirty());
            assert_eq!(fs::read(&output).unwrap(), saved);
            let mut reopened = WorkbookSession::open(
                &output,
                &WorkbookReadLimits::default(),
                HistoryBudget::default(),
            )
            .unwrap();
            assertions += check(&reopened, &wanted, source, spec);
            policy
                .validate_candidate(&fs::read(&output).unwrap())
                .unwrap();
            assert_eq!(reopened.opaque_xml_policy_identity(), None); // ordinary reopen remains strict
            let reopened_bytes = reopened.bytes().to_vec();
            assert!(
                reopened
                    .set_value(
                        sheet,
                        address(spec["type_cell"].as_str().unwrap()),
                        Some(TypedValue::Text("Unreviewed reopen must refuse".into()))
                    )
                    .is_err()
            );
            assert_eq!(reopened.bytes(), reopened_bytes);
            assert!(
                WorkbookSession::open_with_opaque_xml_policy(
                    &output,
                    &WorkbookReadLimits::default(),
                    HistoryBudget::default(),
                    Arc::clone(&policy)
                )
                .is_err()
            ); // no source reseal
            assert_eq!(s.opaque_xml_policy_identity(), policy_id);
            let state = (
                s.bytes().to_vec(),
                s.revision(),
                s.history_frames(),
                s.retained_archive_bytes(),
                s.is_dirty(),
                s.can_undo(),
                s.can_redo(),
            );
            assert!(s.save_as_new(&output).is_err());
            assert_eq!(
                (
                    s.bytes().to_vec(),
                    s.revision(),
                    s.history_frames(),
                    s.retained_archive_bytes(),
                    s.is_dirty(),
                    s.can_undo(),
                    s.can_redo()
                ),
                state
            );
            assert_eq!(fs::read(&output).unwrap(), saved);
            s.undo().unwrap();
            revision += 1;
            assert!(s.is_dirty());
            s.redo().unwrap();
            revision += 1;
            assert!(!s.is_dirty());
            assert_eq!(s.revision(), revision);
        }
        assert_eq!(fs::read(&input).unwrap(), source);
        assert_eq!(
            fs::read(fixtures.join(format!("{id}.xlsx"))).unwrap(),
            source
        );
        records.push(json!({"operation":op["id"],"step":index+1,"revision":s.revision(),"history_frames":s.history_frames(),"retained_archive_bytes":s.retained_archive_bytes()}));
    }
    let receipt = json!({"case":id,"status":"PASS","manifest_sha256":PIN,"policy_sha256":policy.identity().iter().map(|b|format!("{b:02x}")).collect::<String>(),"saved_reopen_mode":"ordinary strict readonly projection; editable policy reopen NOT_IMPLEMENTED","forward_operations":records,"cell_snapshot_assertions":assertions,"undo_redo_pairs":11,"saved_reopened_states":3,"existing_output_refusals":3,"native_runs":0});
    write_new(
        &dir.join("receipt.json"),
        serde_json::to_string_pretty(&receipt).unwrap().as_bytes(),
    );
    println!("ZEN_TYPED_CASE {id}: {}", dir.display());
}
#[test]
fn budget_typed_business_api() {
    run("budget");
}
#[test]
fn orders_typed_business_api() {
    run("orders");
}
#[test]
fn attendance_typed_business_api() {
    run("attendance");
}

fn state(
    s: &WorkbookSession,
) -> (
    Vec<u8>,
    u64,
    usize,
    usize,
    bool,
    bool,
    bool,
    Option<[u8; 32]>,
) {
    (
        s.bytes().to_vec(),
        s.revision(),
        s.history_frames(),
        s.retained_archive_bytes(),
        s.is_dirty(),
        s.can_undo(),
        s.can_redo(),
        s.opaque_xml_policy_identity(),
    )
}
fn budget_source() -> &'static [u8] {
    include_bytes!("../../../RCD/engine-acceptance/eng-005-business-v2/budget.xlsx")
}
#[test]
fn explicit_policy_does_not_waive_formula_date_or_history_budgets() {
    let source = budget_source();
    let policy = owned_policy(source, "negative session specimens");
    let mut s = WorkbookSession::from_bytes_with_opaque_xml_policy(
        source,
        &WorkbookReadLimits::default(),
        HistoryBudget::default(),
        Arc::clone(&policy),
    )
    .unwrap();
    let before = state(&s);
    for formula in [
        "MissingDefinedName+1",
        "UNSUPPORTED(A1)",
        "{1,2}",
        "[foreign.xlsx]Sheet1!A1",
    ] {
        assert!(
            s.set_formula("Budget", address("D4"), formula).is_err(),
            "{formula}"
        );
        assert_eq!(state(&s), before);
    }
    assert!(
        s.set_date_serial("Budget", address("B10"), 61.5, DateSystem::Excel1904)
            .is_err()
    );
    assert_eq!(state(&s), before);
    assert!(
        s.set_value(
            "Budget",
            address("B12"),
            Some(TypedValue::Number {
                value: f64::NAN,
                lexical: "NaN".into()
            })
        )
        .is_err()
    );
    assert_eq!(state(&s), before);
    let budget = HistoryBudget {
        max_frames: 1,
        ..HistoryBudget::default()
    };
    let mut s = WorkbookSession::from_bytes_with_opaque_xml_policy(
        source,
        &WorkbookReadLimits::default(),
        budget,
        policy,
    )
    .unwrap();
    let before = state(&s);
    assert!(
        s.set_value(
            "Budget",
            address("B12"),
            Some(TypedValue::Text("budget refusal".into()))
        )
        .is_err()
    );
    assert_eq!(state(&s), before);
}
#[test]
fn explicit_policy_does_not_waive_unrecognized_relationships() {
    let source = budget_source();
    let mut package = zen_opc::Package::from_bytes(source).unwrap();
    let mut rels = package.rels_for("/xl/workbook.xml").unwrap();
    rels.add("unknown", "urn:unrecognized:dependency", "workbook.xml");
    package.set_rels_for("/xl/workbook.xml", &rels).unwrap();
    let changed = package.to_bytes().unwrap();
    let policy = owned_policy(&changed, "owned unknown relationship fault specimen");
    let mut s = WorkbookSession::from_bytes_with_opaque_xml_policy(
        &changed,
        &WorkbookReadLimits::default(),
        HistoryBudget::default(),
        policy,
    )
    .unwrap();
    let before = state(&s);
    let err = s
        .set_value(
            "Budget",
            address("B12"),
            Some(TypedValue::Text("Must refuse".into())),
        )
        .unwrap_err();
    assert!(
        err.to_string().contains("unsupported editing relationship"),
        "{err}"
    );
    assert_eq!(state(&s), before);
}
#[test]
fn policy_source_change_and_locked_output_preserve_all_session_state() {
    use std::os::windows::fs::OpenOptionsExt;
    let dir = tempfile::tempdir().unwrap();
    let source = budget_source();
    let input = dir.path().join("source.xlsx");
    write_new(&input, source);
    let policy = owned_policy(source, "source and output failure specimens");
    let mut s = WorkbookSession::open_with_opaque_xml_policy(
        &input,
        &WorkbookReadLimits::default(),
        HistoryBudget::default(),
        policy,
    )
    .unwrap();
    s.set_value(
        "Budget",
        address("B12"),
        Some(TypedValue::Text("Reviewed".into())),
    )
    .unwrap();
    let before = state(&s);
    let output = dir.path().join("locked.xlsx");
    write_new(&output, b"existing locked output");
    let _lock = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&output)
        .unwrap();
    assert!(s.save_as_new(&output).is_err());
    assert_eq!(state(&s), before);
    drop(_lock);
    assert_eq!(fs::read(output).unwrap(), b"existing locked output");
    fs::write(&input, b"owned external source change").unwrap();
    let target = dir.path().join("must-not-exist.xlsx");
    assert!(matches!(
        s.save_as_new(&target),
        Err(zen_session::SaveError::SourceChanged)
    ));
    assert_eq!(state(&s), before);
    assert!(!target.exists());
}
