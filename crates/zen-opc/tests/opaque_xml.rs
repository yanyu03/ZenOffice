use zen_opc::opaque_xml::*;
use zen_opc::{Package, Relationships, ZipArchive, ZipWriter};
const PAYLOAD: &[u8] =
    br#"<memo xmlns="urn:owned:independent-test" owner="test">Independent owned metadata</memo>"#;
fn package() -> Package {
    let mut p = Package::with_common_defaults();
    p.put_text("/xl/workbook.xml", "<workbook/>").unwrap();
    p.put("/customXml/memo.xml", PAYLOAD).unwrap();
    let mut rels = Relationships::default();
    rels.add("custom1", CUSTOM_XML_REL, "../customXml/memo.xml");
    p.set_rels_for("/xl/workbook.xml", &rels).unwrap();
    p
}
fn declaration(bytes: &[u8]) -> OpaqueXmlDeclaration {
    OpaqueXmlDeclaration {
        source_sha256: sha256(bytes),
        part_name: "/customXml/memo.xml".into(),
        payload_sha256: sha256(PAYLOAD),
        payload_bytes: PAYLOAD.len(),
        content_type: "application/xml".into(),
        relationship_id: "custom1".into(),
        relationship_target: "../customXml/memo.xml".into(),
        controlled_owned_source: true,
        semantically_independent: true,
        review_basis: "Owned generic fixture authored here; no worksheet consumers".into(),
    }
}
fn rejects(p: Package, expected: &str) {
    let mut writer = ZipWriter::store_only();
    writer.add("[Content_Types].xml", p.content_types.to_xml());
    for part in &p.parts {
        writer.add(part.name.trim_start_matches('/'), part.data.clone());
    }
    let bytes = writer.finish().unwrap();
    let d = declaration(&bytes);
    let err = validate_opaque_xml_descriptor(&bytes, &[d])
        .unwrap_err()
        .to_string();
    assert!(err.contains(expected), "{err}");
}
#[test]
fn generic_leaf_receipt_is_read_only() {
    let bytes = package().to_bytes().unwrap();
    let d = declaration(&bytes);
    let before = bytes.clone();
    let r = validate_opaque_xml_descriptor(&bytes, &[d.clone()]).unwrap();
    assert_eq!(r.source_sha256(), d.source_sha256);
    assert_eq!(r.payload_sha256(), d.payload_sha256);
    assert_eq!(before, bytes);
}
#[test]
fn standard_sha256_vector() {
    assert_eq!(
        sha256(b"abc"),
        [
            0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae,
            0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61,
            0xf2, 0x00, 0x15, 0xad
        ]
    );
}
#[test]
fn missing_duplicate_or_untrusted_declarations_refuse() {
    let bytes = package().to_bytes().unwrap();
    let d = declaration(&bytes);
    assert!(validate_opaque_xml_descriptor(&bytes, &[]).is_err());
    assert!(validate_opaque_xml_descriptor(&bytes, &[d.clone(), d.clone()]).is_err());
    for index in 0..3 {
        let mut changed = d.clone();
        match index {
            0 => changed.controlled_owned_source = false,
            1 => changed.semantically_independent = false,
            _ => changed.review_basis.clear(),
        }
        assert!(validate_opaque_xml_descriptor(&bytes, &[changed]).is_err());
    }
}
#[test]
fn all_identity_fields_checked() {
    let bytes = package().to_bytes().unwrap();
    for index in 0..8 {
        let mut d = declaration(&bytes);
        match index {
            0 => d.source_sha256[0] ^= 1,
            1 => d.payload_sha256[0] ^= 1,
            2 => d.payload_bytes += 1,
            3 => d.content_type = "text/xml".into(),
            4 => d.part_name = "/customXml/absent.xml".into(),
            5 => d.relationship_id = "other".into(),
            6 => d.relationship_target = "/customXml/memo.xml".into(),
            _ => d.part_name = "/customXml/%6demo.xml".into(),
        }
        assert!(
            validate_opaque_xml_descriptor(&bytes, &[d]).is_err(),
            "case {index}"
        );
    }
}
#[test]
fn media_mutation_and_duplicate_override_refuse() {
    let mut p = package();
    p.content_types
        .add_override("/customXml/memo.xml", "text/xml")
        .unwrap();
    rejects(p, "media identity");
    let mut p = package();
    p.content_types
        .overrides
        .push(("/customXml/memo.xml".into(), "application/xml".into()));
    p.content_types
        .overrides
        .push(("/customXml/memo.xml".into(), "application/xml".into()));
    rejects(p, "duplicate media override");
}
#[test]
fn undeclared_part_and_properties_refuse() {
    for name in ["/customXml/extra.xml", "/customXml/itemProps1.xml"] {
        let mut p = package();
        p.put_text(name, "<extra/>").unwrap();
        rejects(p, "undeclared custom XML");
    }
}
#[test]
fn wrong_owner_external_and_duplicate_incoming_refuse() {
    for index in 0..4 {
        let mut p = package();
        let mut rels = p.rels_for("/xl/workbook.xml").unwrap();
        match index {
            0 => {
                rels.items[0].mode = Some("External".into());
            }
            1 => {
                rels.items[0].ty = "urn:other".into();
            }
            2 => {
                rels.add("second", CUSTOM_XML_REL, "../customXml/memo.xml");
            }
            _ => {
                p.set_rels_for("/customXml/memo.xml", &rels).unwrap();
            }
        }
        p.set_rels_for("/xl/workbook.xml", &rels).unwrap();
        assert!(
            validate_opaque_xml_descriptor(
                &p.to_bytes().unwrap(),
                &[declaration(&p.to_bytes().unwrap())]
            )
            .is_err()
        );
    }
    let mut p = package();
    let rels = p.rels_for("/xl/workbook.xml").unwrap();
    p.put_text("/word/document.xml", "<document/>").unwrap();
    p.set_rels_for("/word/document.xml", &rels).unwrap();
    rejects(p, "owner/relationship identity");
}
#[test]
fn absent_and_duplicate_relationship_ids_refuse() {
    let mut p = package();
    p.set_rels_for("/xl/workbook.xml", &Relationships::default())
        .unwrap();
    rejects(p, "single workbook relationship");
    let mut p = package();
    let mut rels = p.rels_for("/xl/workbook.xml").unwrap();
    rels.add("custom1", "urn:test", "workbook.xml");
    p.set_rels_for("/xl/workbook.xml", &rels).unwrap();
    rejects(p, "duplicate relationship ID");
}
#[test]
fn known_bindings_extensions_and_protection_refuse() {
    for tag in [
        "dataBinding",
        "xmlMaps",
        "MapInfo",
        "xmlCellPr",
        "singleXmlCells",
        "connections",
        "externalLink",
        "datastoreItem",
        "schemaRef",
        "extLst",
        "documentProtection",
        "writeProtection",
    ] {
        let mut p = package();
        p.put_text("/xl/extra.xml", format!("<{tag}/>")).unwrap();
        rejects(p, "requires separate review");
    }
}
#[test]
fn active_and_signed_packages_refuse() {
    for name in [
        "/_xmlsignatures/sig.xml",
        "/xl/vbaProject.bin",
        "/xl/activeX/a.bin",
        "/xl/embeddings/b.bin",
    ] {
        let mut p = package();
        p.put(name, b"owned mutation").unwrap();
        rejects(p, "active/signed package");
    }
}
#[test]
fn payload_mutation_requires_new_declaration_but_shape_still_checked() {
    for payload in [
        "<r><child/></r>",
        "<r>mixed<child/>text</r>",
        "<!DOCTYPE r><r/>",
        "<r><!--comment--></r>",
    ] {
        let mut p = package();
        p.put_text("/customXml/memo.xml", payload).unwrap();
        let bytes = p.to_bytes().unwrap();
        let mut d = declaration(&bytes);
        d.payload_sha256 = sha256(payload.as_bytes());
        d.payload_bytes = payload.len();
        assert!(validate_opaque_xml_descriptor(&bytes, &[d]).is_err());
    }
}
#[test]
fn duplicate_zip_and_case_alias_refuse() {
    let bytes = package().to_bytes().unwrap();
    let zip = ZipArchive::read(&bytes).unwrap();
    for name in [
        "customXml/memo.xml",
        "customXml/MEMO.xml",
        "customXml/%6demo.xml",
        "customXml/../memo.xml",
    ] {
        let mut w = ZipWriter::new();
        for e in &zip.entries {
            w.add(&e.name, e.data.clone());
        }
        w.add(name, PAYLOAD.to_vec());
        let changed = w.finish().unwrap();
        let d = declaration(&changed);
        assert!(validate_opaque_xml_descriptor(&changed, &[d]).is_err());
    }
}
#[test]
fn descriptor_and_xml_budgets_refuse() {
    let bytes = package().to_bytes().unwrap();
    let mut d = declaration(&bytes);
    d.review_basis = "x".repeat(4097);
    assert!(validate_opaque_xml_descriptor(&bytes, &[d]).is_err());
    let mut p = package();
    p.put_text(
        "/xl/oversize.xml",
        format!("<r>{}</r>", "x".repeat(1024 * 1024)),
    )
    .unwrap();
    rejects(p, "XML budget");
    let mut p = package();
    p.put_text("/xl/nodes.xml", format!("<r>{}</r>", "<n/>".repeat(16384)))
        .unwrap();
    rejects(p, "over-budget XML");
}
#[test]
fn three_sealed_owned_inputs_descriptor_only() {
    for (id, bytes) in [
        (
            "budget",
            include_bytes!("../../../RCD/engine-acceptance/eng-005-business-v2/budget.xlsx")
                .as_slice(),
        ),
        (
            "orders",
            include_bytes!("../../../RCD/engine-acceptance/eng-005-business-v2/orders.xlsx")
                .as_slice(),
        ),
        (
            "attendance",
            include_bytes!("../../../RCD/engine-acceptance/eng-005-business-v2/attendance.xlsx")
                .as_slice(),
        ),
    ] {
        let zip = ZipArchive::read(bytes).unwrap();
        let payload = zip.get("customXml/zen-probe.xml").unwrap();
        let d = OpaqueXmlDeclaration {
            source_sha256: sha256(bytes),
            part_name: "/customXml/zen-probe.xml".into(),
            payload_sha256: sha256(payload),
            payload_bytes: payload.len(),
            content_type: "application/xml".into(),
            relationship_id: "rIdProbe".into(),
            relationship_target: "../customXml/zen-probe.xml".into(),
            controlled_owned_source: true,
            semantically_independent: true,
            review_basis: format!(
                "Owned sealed {id} authored fixture; contract audit034; descriptor only"
            ),
        };
        validate_opaque_xml_descriptor(bytes, &[d]).unwrap();
    }
}

#[test]
fn relationship_traversal_above_root_refuses_even_if_declared() {
    let mut p = package();
    let mut rels = p.rels_for("/xl/workbook.xml").unwrap();
    rels.items[0].target = "../../customXml/memo.xml".into();
    p.set_rels_for("/xl/workbook.xml", &rels).unwrap();
    let bytes = p.to_bytes().unwrap();
    let mut d = declaration(&bytes);
    d.relationship_target = "../../customXml/memo.xml".into();
    assert!(validate_opaque_xml_descriptor(&bytes, &[d]).is_err());
}

fn changed_workbook(p: &mut Package, text: &str) -> Vec<u8> {
    p.put_text("/xl/workbook.xml", text).unwrap();
    p.to_bytes().unwrap()
}
#[test]
fn policy_checks_changed_frames_without_resealing_initial_source() {
    let mut p = package();
    let initial = p.to_bytes().unwrap();
    let d = declaration(&initial);
    let policy = OpaqueXmlPolicy::bind_initial(&initial, &d).unwrap();
    let identity = policy.identity();
    let changed = changed_workbook(&mut p, "<workbook><ownedCandidate/></workbook>");
    assert_ne!(sha256(&changed), d.source_sha256);
    let receipt = policy.validate_candidate(&changed).unwrap();
    assert_eq!(receipt.source_sha256(), sha256(&changed));
    assert_eq!(policy.initial_source_sha256(), sha256(&initial));
    assert!(policy.validate_initial(&changed).is_err());
    assert_eq!(policy.identity(), identity);
    policy.validate_initial(&initial).unwrap();
    // Checking preserved bytes is deliberately not proof of an allowed edit or
    // candidate ancestry. No session or write API receives this policy yet.
}
#[test]
fn policy_copies_caller_declaration_and_has_stable_identity() {
    let bytes = package().to_bytes().unwrap();
    let mut d = declaration(&bytes);
    let a = OpaqueXmlPolicy::bind_initial(&bytes, &d).unwrap();
    let b = OpaqueXmlPolicy::bind_initial(&bytes, &d).unwrap();
    assert_eq!(a.identity(), b.identity());
    d.review_basis = "A different reviewed premise".into();
    let c = OpaqueXmlPolicy::bind_initial(&bytes, &d).unwrap();
    assert_ne!(a.identity(), c.identity());
    d.part_name.clear();
    d.source_sha256 = [0; 32];
    d.controlled_owned_source = false;
    a.validate_initial(&bytes).unwrap();
    assert!(OpaqueXmlPolicy::bind_initial(&bytes, &d).is_err());
    assert!(a.retained_descriptor_bytes() > 0 && a.retained_descriptor_bytes() <= 16 * 1024);
}
#[test]
fn policy_refuses_new_source_and_preserves_original_binding() {
    let mut p = package();
    let initial = p.to_bytes().unwrap();
    let a = OpaqueXmlPolicy::bind_initial(&initial, &declaration(&initial)).unwrap();
    let other = changed_workbook(&mut p, "<workbook other='source'/>");
    assert!(a.validate_initial(&other).is_err());
    assert!(OpaqueXmlPolicy::bind_initial(&other, &declaration(&initial)).is_err());
    let b = OpaqueXmlPolicy::bind_initial(&other, &declaration(&other)).unwrap();
    assert_ne!(a.identity(), b.identity());
    assert_eq!(a.initial_source_sha256(), sha256(&initial));
}
#[test]
fn policy_refuses_semantically_equal_but_changed_owning_row() {
    let p = package();
    let initial = p.to_bytes().unwrap();
    let policy = OpaqueXmlPolicy::bind_initial(&initial, &declaration(&initial)).unwrap();
    let mut p = package();
    let mut rels = p.rels_for("/xl/workbook.xml").unwrap();
    rels.items[0].mode = Some("Internal".into());
    p.set_rels_for("/xl/workbook.xml", &rels).unwrap();
    let changed = p.to_bytes().unwrap();
    // Initial descriptor semantics accept explicit Internal; immutable policy
    // must still refuse a change to the owning row's exact representation.
    validate_opaque_xml_descriptor(&changed, &[declaration(&changed)]).unwrap();
    let identity = policy.identity();
    assert!(
        policy
            .validate_candidate(&changed)
            .unwrap_err()
            .to_string()
            .contains("row changed")
    );
    assert_eq!(policy.identity(), identity);
    policy.validate_candidate(&initial).unwrap();
}
#[test]
fn policy_refuses_effective_media_declaration_migration() {
    let initial = package().to_bytes().unwrap();
    let policy = OpaqueXmlPolicy::bind_initial(&initial, &declaration(&initial)).unwrap();
    let mut p = package();
    p.content_types
        .add_override("/customXml/memo.xml", "application/xml")
        .unwrap();
    let changed = p.to_bytes().unwrap();
    validate_opaque_xml_descriptor(&changed, &[declaration(&changed)]).unwrap();
    assert!(
        policy
            .validate_candidate(&changed)
            .unwrap_err()
            .to_string()
            .contains("row changed")
    );
}
#[test]
fn policy_rechecks_payload_binding_topology_and_active_content_each_frame() {
    let initial = package().to_bytes().unwrap();
    let policy = OpaqueXmlPolicy::bind_initial(&initial, &declaration(&initial)).unwrap();
    let identity = policy.identity();
    for index in 0..6 {
        let mut p = package();
        match index {
            0 => p
                .put_text("/customXml/memo.xml", "<memo>different payload</memo>")
                .unwrap(),
            1 => p.put_text("/customXml/extra.xml", "<extra/>").unwrap(),
            2 => p.put_text("/xl/extra.xml", "<dataBinding/>").unwrap(),
            3 => p
                .put("/xl/vbaProject.bin", b"owned macro specimen")
                .unwrap(),
            4 => {
                let mut r = p.rels_for("/xl/workbook.xml").unwrap();
                r.items[0].target = "../customXml/elsewhere.xml".into();
                p.set_rels_for("/xl/workbook.xml", &r).unwrap();
            }
            _ => {
                p.content_types
                    .add_override("/customXml/memo.xml", "text/xml")
                    .unwrap();
            }
        }
        assert!(
            policy.validate_candidate(&p.to_bytes().unwrap()).is_err(),
            "case {index}"
        );
        assert_eq!(policy.identity(), identity);
        assert_eq!(policy.initial_source_sha256(), sha256(&initial));
        policy.validate_initial(&initial).unwrap();
    }
}
#[test]
fn policy_allows_unrelated_relationship_invalidation_and_archive_repacking() {
    let initial = package().to_bytes().unwrap();
    let policy = OpaqueXmlPolicy::bind_initial(&initial, &declaration(&initial)).unwrap();
    let mut p = package();
    let mut rels = p.rels_for("/xl/workbook.xml").unwrap();
    rels.add(
        "other",
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles",
        "styles.xml",
    );
    p.set_rels_for("/xl/workbook.xml", &rels).unwrap();
    p.put_text("/xl/styles.xml", "<styleSheet/>").unwrap();
    let with_other = p.to_bytes().unwrap();
    policy.validate_candidate(&with_other).unwrap();
    policy.validate_candidate(&initial).unwrap();
    let archive = ZipArchive::read(&initial).unwrap();
    let mut writer = ZipWriter::store_only();
    for e in archive.entries {
        writer.add(e.name, e.data);
    }
    let repacked = writer.finish().unwrap();
    assert_ne!(repacked, initial);
    policy.validate_candidate(&repacked).unwrap();
    assert!(policy.validate_initial(&repacked).is_err());
}
#[test]
fn policy_retained_metadata_budget_is_enforced_before_binding() {
    let mut p = package();
    // Large whitespace in the owning XML row is legal XML; it must not become
    // an unbounded retained descriptor. Store-only avoids ZIP ratio preemption.
    let original = p.rels_for("/xl/workbook.xml").unwrap().to_xml();
    let long = original.replace(
        "Id=\"custom1\"",
        &format!("{}Id=\"custom1\"", " ".repeat(17 * 1024)),
    );
    p.put_text("/xl/_rels/workbook.xml.rels", long).unwrap();
    let mut writer = ZipWriter::store_only();
    writer.add("[Content_Types].xml", p.content_types.to_xml());
    for part in p.parts {
        writer.add(part.name.trim_start_matches('/'), part.data);
    }
    let bytes = writer.finish().unwrap();
    let d = declaration(&bytes);
    validate_opaque_xml_descriptor(&bytes, &[d.clone()]).unwrap();
    assert!(
        OpaqueXmlPolicy::bind_initial(&bytes, &d)
            .unwrap_err()
            .to_string()
            .contains("row budget")
    );
}

#[test]
fn unknown_metadata_semantics_refuse_initial_and_candidate_binding() {
    let initial = package().to_bytes().unwrap();
    let policy = OpaqueXmlPolicy::bind_initial(&initial, &declaration(&initial)).unwrap();
    for (part, needle, replacement) in [
        (
            "/xl/_rels/workbook.xml.rels",
            "<Relationships ",
            "<Relationships xml:base=\"https://invalid.example/\" ",
        ),
        (
            "/xl/_rels/workbook.xml.rels",
            "Id=\"custom1\"",
            "Id=\"custom1\" unknown=\"dependency\"",
        ),
        (
            "/xl/_rels/workbook.xml.rels",
            "/></Relationships>",
            "><consumer/></Relationship></Relationships>",
        ),
        (
            "/[Content_Types].xml",
            "<Types ",
            "<Types xml:base=\"https://invalid.example/\" ",
        ),
        (
            "/[Content_Types].xml",
            "Extension=\"xml\"",
            "Extension=\"xml\" unknown=\"dependency\"",
        ),
        (
            "/[Content_Types].xml",
            "<Default ",
            "unmodeled-text<Default ",
        ),
    ] {
        let mut p = package();
        let source = if part == "/[Content_Types].xml" {
            p.content_types.to_xml()
        } else {
            p.get_text(part).unwrap()
        };
        let changed = source.replace(needle, replacement);
        assert_ne!(changed, source, "invalid fixture {part}");
        let bytes = if part == "/[Content_Types].xml" {
            let archive = ZipArchive::read(&initial).unwrap();
            let mut writer = ZipWriter::new();
            for e in archive.entries {
                if e.name == "[Content_Types].xml" {
                    writer.add(e.name, changed.as_bytes());
                } else {
                    writer.add(e.name, e.data);
                }
            }
            writer.finish().unwrap()
        } else {
            p.put_text(part, changed).unwrap();
            p.to_bytes().unwrap()
        };
        let d = declaration(&bytes);
        assert!(validate_opaque_xml_descriptor(&bytes, &[d.clone()]).is_err());
        assert!(OpaqueXmlPolicy::bind_initial(&bytes, &d).is_err());
        assert!(policy.validate_candidate(&bytes).is_err());
    }
    policy.validate_initial(&initial).unwrap();
}
