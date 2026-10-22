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
