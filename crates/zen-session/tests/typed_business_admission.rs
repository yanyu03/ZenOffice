//! Ordinary entry remains strict for unknown custom XML. These negative
//! regressions are separate from the explicit-policy eight-step business runner.
use zen_engine::workbook::{CellAddress, TypedValue};
use zen_formats::spreadsheet::WorkbookReadLimits;
use zen_session::{HistoryBudget, WorkbookSession};
fn refuse(source:&[u8],sheet:&str,row:u32,col:u16){
    let mut s=WorkbookSession::from_bytes(source,&WorkbookReadLimits::default(),HistoryBudget::default()).unwrap();
    let before=(s.bytes().to_vec(),s.revision(),s.history_frames(),s.retained_archive_bytes(),s.is_dirty(),s.can_undo(),s.can_redo());
    let error=s.set_value(sheet,CellAddress::new(row,col).unwrap(),Some(TypedValue::Text("Review admission".into()))).unwrap_err();
    assert!(error.to_string().contains("unknown custom XML dependency"),"{error}");
    assert_eq!((s.bytes().to_vec(),s.revision(),s.history_frames(),s.retained_archive_bytes(),s.is_dirty(),s.can_undo(),s.can_redo()),before);
    assert_eq!(s.bytes(),source);
}
#[test]fn budget_unknown_custom_xml_refuses_atomically(){refuse(include_bytes!("../../../RCD/engine-acceptance/eng-005-business-v2/budget.xlsx"),"Budget",11,1);}
#[test]fn orders_unknown_custom_xml_refuses_atomically(){refuse(include_bytes!("../../../RCD/engine-acceptance/eng-005-business-v2/orders.xlsx"),"Orders",8,2);}
#[test]fn attendance_unknown_custom_xml_refuses_atomically(){refuse(include_bytes!("../../../RCD/engine-acceptance/eng-005-business-v2/attendance.xlsx"),"Attendance",8,2);}
