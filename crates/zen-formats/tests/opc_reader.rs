use std::{
    collections::{BTreeSet, HashSet},
    io::{Cursor, Write},
};
use zen_formats::opc::{
    DiagnosticCode, OFFICE_DOCUMENT_RELATIONSHIP, OpcError, Package, PartName, ReadLimits,
    RelationshipTarget,
};

const CT: &str = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/></Types>"#;
fn rels(body: &str) -> String {
    format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{body}</Relationships>"#
    )
}
fn link(id: &str, target: &str, mode: &str) -> String {
    format!(
        r#"<Relationship Id="{id}" Type="{OFFICE_DOCUMENT_RELATIONSHIP}" Target="{target}" {mode}/>"#
    )
}
fn zip(items: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in items {
        writer
            .start_file(
                *name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}
fn fixture(root: &str, extras: &[(&str, &[u8])]) -> Vec<u8> {
    let mut items = vec![
        ("[Content_Types].xml", CT.as_bytes()),
        ("_rels/.rels", root.as_bytes()),
        ("doc/main.xml", b"<opaque/>".as_slice()),
    ];
    items.extend_from_slice(extras);
    zip(&items)
}
fn read(bytes: &[u8]) -> Result<Package, OpcError> {
    Package::read(bytes, &ReadLimits::default())
}

#[test]
fn root_relationship_locates_main_without_fixed_workbook_path() {
    let bytes = fixture(&rels(&link("rId1", "doc/main.xml", "")), &[]);
    let p = read(&bytes).unwrap();
    assert_eq!(
        p.office_document().unwrap().unwrap().name().as_str(),
        "/doc/main.xml"
    );
    assert_eq!(p.parts().count(), 2);
    assert_eq!(
        p.incoming(&PartName::parse("/DOC/main.XML").unwrap())
            .count(),
        1
    );
    assert_eq!(p.content_types_bytes(), CT.as_bytes());
}
#[test]
fn case_identity_agrees_for_lookup_order_and_hash() {
    let a = PartName::parse("/Doc/Main.xml").unwrap();
    let b = PartName::parse("/doc/main.XML").unwrap();
    assert_eq!(a, b);
    assert_eq!(HashSet::from([a.clone(), b.clone()]).len(), 1);
    assert_eq!(BTreeSet::from([a, b]).len(), 1);
    let p = read(&fixture(&rels(&link("r1", "DOC/MAIN.XML", "")), &[])).unwrap();
    assert!(p.office_document().unwrap().is_some());
}
#[test]
fn external_reference_is_retained_verbatim_and_never_followed() {
    let p = read(&fixture(
        &rels(&link(
            "r1",
            "https://example.invalid/book.xlsx#part",
            "TargetMode=\"External\"",
        )),
        &[],
    ))
    .unwrap();
    assert!(
        matches!(&p.relationships()[0].target, RelationshipTarget::External{reference} if reference == "https://example.invalid/book.xlsx#part")
    );
    assert!(p.office_document().is_err());
}
#[test]
fn relative_target_and_fragment_resolve_from_source() {
    let source = rels(&link("r1", "../doc/main.xml#bookmark", ""));
    let p = read(&fixture(
        &rels(""),
        &[("doc/_rels/main.xml.rels", source.as_bytes())],
    ))
    .unwrap();
    assert!(
        matches!(&p.relationships()[0].target, RelationshipTarget::Internal{part,fragment} if part.as_str()=="/doc/main.xml" && fragment.as_deref()==Some("bookmark"))
    );
}
#[test]
fn dangling_targets_are_diagnosed_and_root_failure_is_explicit() {
    let p = read(&fixture(&rels(&link("r1", "missing.xml", "")), &[])).unwrap();
    assert!(
        p.diagnostics()
            .iter()
            .any(|d| d.code == DiagnosticCode::DanglingTarget)
    );
    assert!(p.office_document().is_err());
}
#[test]
fn opaque_part_and_unmapped_physical_item_are_preserved() {
    let p = read(&fixture(&rels(""), &[("[trash]/old", b"unknown bytes")])).unwrap();
    assert_eq!(p.unmapped_items()[0].bytes, b"unknown bytes");
    assert_eq!(
        p.part(&PartName::parse("/doc/main.xml").unwrap())
            .unwrap()
            .bytes(),
        b"<opaque/>"
    );
    assert!(p.office_document().unwrap().is_none());
}
#[test]
fn utf16_both_byte_orders_are_supported_for_metadata() {
    for big in [false, true] {
        let mut ct = if big {
            vec![0xfe, 0xff]
        } else {
            vec![0xff, 0xfe]
        };
        for c in CT.encode_utf16() {
            ct.extend_from_slice(&if big {
                c.to_be_bytes()
            } else {
                c.to_le_bytes()
            });
        }
        let p = read(&zip(&[
            ("[Content_Types].xml", &ct),
            ("main.xml", b"<main/>"),
        ]))
        .unwrap();
        assert_eq!(p.parts().count(), 1);
        assert_eq!(p.content_types_bytes(), ct);
    }
}
#[test]
fn content_type_override_wins_and_duplicate_defaults_fail() {
    let ct = CT.replace(
        "</Types>",
        r#"<Override PartName="/DOC/MAIN.XML" ContentType="application/custom"/></Types>"#,
    );
    let p = read(&zip(&[
        ("[Content_Types].xml", ct.as_bytes()),
        ("doc/main.xml", b"x"),
    ]))
    .unwrap();
    assert_eq!(
        p.parts().next().unwrap().content_type(),
        "application/custom"
    );
    let bad = CT.replace(
        "</Types>",
        r#"<Default Extension="XML" ContentType="application/xml"/></Types>"#,
    );
    assert!(read(&zip(&[("[Content_Types].xml", bad.as_bytes())])).is_err());
}
#[test]
fn case_equivalent_parts_and_derived_parts_are_rejected() {
    for extra in ["DOC/MAIN.XML", "doc/main.xml/child.xml"] {
        assert!(matches!(
            read(&fixture(&rels(""), &[(extra, b"x")])),
            Err(OpcError::Invalid(_))
        ));
    }
}
#[test]
fn repeated_relationship_ids_and_multiple_roots_fail() {
    let two = format!(
        "{}{}",
        link("r1", "doc/main.xml", ""),
        link("r1", "doc/main.xml", "")
    );
    assert!(read(&fixture(&rels(&two), &[])).is_err());
    let two = format!(
        "{}{}",
        link("r1", "doc/main.xml", ""),
        link("r2", "doc/main.xml", "")
    );
    assert!(
        read(&fixture(&rels(&two), &[]))
            .unwrap()
            .office_document()
            .is_err()
    );
}
#[test]
fn relationships_require_source_and_cannot_target_relationship_parts() {
    assert!(
        read(&fixture(
            &rels(""),
            &[("missing/_rels/part.xml.rels", rels("").as_bytes())]
        ))
        .is_err()
    );
    assert!(read(&fixture(&rels(&link("r1", "_rels/.rels", "")), &[])).is_err());
}
#[test]
fn invalid_xml_namespace_dtd_and_target_mode_are_rejected() {
    for root in [
        rels(&link("r1", "doc/main.xml", "TargetMode=\"Other\"")),
        rels("").replace("/relationships", "/wrong"),
        format!("<!DOCTYPE Relationships [<!ENTITY evil 'x'>]>{}", rels("")),
    ] {
        assert!(read(&fixture(&root, &[])).is_err(), "{root}");
    }
}
#[test]
fn directory_and_query_relationship_targets_fail_explicitly() {
    for target in [
        "doc/",
        "doc/.",
        "doc/..",
        ".",
        "..",
        "doc/main.xml?query=1",
        "../../escape.xml",
    ] {
        assert!(
            read(&fixture(&rels(&link("r1", target, "")), &[])).is_err(),
            "{target}"
        );
    }
}
#[test]
fn malformed_and_unsupported_part_names_fail() {
    for name in [
        "relative.xml",
        "/",
        "/a//b",
        "/a.",
        "/%41.xml",
        "/a%2Fb",
        "/a%5Cb",
        "/a%+1",
        "/a%",
    ] {
        assert!(PartName::parse(name).is_err(), "{name}");
    }
    assert!(matches!(
        PartName::parse("/中文.xml"),
        Err(OpcError::Unsupported(_))
    ));
}
#[test]
fn resource_budgets_stop_reads() {
    let bytes = fixture(&rels(&link("r1", "doc/main.xml", "")), &[]);
    for limits in [
        ReadLimits {
            max_archive_bytes: 10,
            ..Default::default()
        },
        ReadLimits {
            max_entries: 1,
            ..Default::default()
        },
        ReadLimits {
            max_part_bytes: 1,
            ..Default::default()
        },
        ReadLimits {
            max_total_bytes: 1,
            ..Default::default()
        },
        ReadLimits {
            max_metadata_xml_bytes: 1,
            ..Default::default()
        },
        ReadLimits {
            max_xml_nodes: 1,
            ..Default::default()
        },
        ReadLimits {
            max_relationships: 0,
            ..Default::default()
        },
    ] {
        assert!(matches!(
            Package::read(&bytes, &limits),
            Err(OpcError::Limit(_)) | Err(OpcError::Xml(_))
        ));
    }
}
#[test]
fn encrypted_and_interleaved_input_report_unsupported() {
    let mut bytes = fixture(&rels(""), &[]);
    let central = bytes.windows(4).position(|b| b == b"PK\x01\x02").unwrap();
    bytes[central + 8] |= 1;
    assert!(matches!(read(&bytes), Err(OpcError::Unsupported(_))));
    assert!(matches!(
        read(&fixture(&rels(""), &[("doc/[0].piece", b"x")])),
        Err(OpcError::Unsupported(_))
    ));
}
#[test]
fn damaged_crc_and_truncated_archive_fail() {
    let mut bytes = fixture(&rels(""), &[]);
    let body = bytes.windows(9).position(|b| b == b"<opaque/>").unwrap();
    bytes[body + 1] ^= 1;
    assert!(read(&bytes).is_err());
    assert!(read(&bytes[..bytes.len() - 10]).is_err());
}
#[test]
fn physical_duplicate_names_are_detected_before_zip_library_dedup() {
    let mut bytes = zip(&[
        ("[Content_Types].xml", CT.as_bytes()),
        ("one.xml", b"1"),
        ("two.xml", b"2"),
    ]);
    for i in 0..bytes.len() - 7 {
        if &bytes[i..i + 7] == b"two.xml" {
            bytes[i..i + 7].copy_from_slice(b"one.xml");
        }
    }
    assert!(matches!(read(&bytes), Err(OpcError::Invalid(_))));
}
#[test]
fn current_xlsx_export_is_readable_by_new_opc_api() {
    #[path = "../examples/support/fixture.rs"]
    mod fixture;
    let sheets = fixture::sample_workbook();
    let result = zen_formats::xlsx::export_workbook(&sheets.iter().collect::<Vec<_>>()).unwrap();
    let p = read(&result.bytes).unwrap();
    assert_eq!(
        p.office_document().unwrap().unwrap().name().as_str(),
        "/xl/workbook.xml"
    );
    assert!(
        !p.diagnostics()
            .iter()
            .any(|d| d.code == DiagnosticCode::DanglingTarget)
    );
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("样本.xlsx");
    zen_formats::save_atomic(&file, &result.bytes).unwrap();
    assert_eq!(
        Package::open(&file, &ReadLimits::default())
            .unwrap()
            .parts()
            .count(),
        p.parts().count()
    );
}
