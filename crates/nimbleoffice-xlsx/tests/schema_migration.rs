//! The `workbook-schema-v5-*.update.bin` fixtures were produced by release
//! 4bdccdd: it opens the matching workbook collaboratively, writes a cell, and
//! persists `encode_state_as_update_v1()`.

use nimbleoffice_xlsx::{CalculationOptions, CellRef, CellValue, SheetId, Workbook};

const SAMPLE: &[u8] = include_bytes!("../../../fixtures/demo/sample.xlsx");
const SAMPLE_V5: &[u8] = include_bytes!("fixtures/workbook-schema-v5-sample.update.bin");
const SHOWCASE_V5: &[u8] = include_bytes!("fixtures/workbook-schema-v5-showcase.update.bin");

fn a1(workbook: &Workbook) -> CellValue {
    workbook
        .sheet(SheetId(0))
        .unwrap()
        .cell(CellRef::parse_a1("A1").unwrap())
        .map(|cell| cell.value.clone())
        .unwrap_or(CellValue::Empty)
}

/// A snapshot only supersedes a replica that has not been edited yet.
#[test]
fn an_edited_replica_does_not_adopt_a_legacy_snapshot() {
    let mut workbook =
        Workbook::open_collaborative_recalculated(SAMPLE, 5_006, CalculationOptions::default())
            .unwrap();
    workbook
        .edit_cell(
            SheetId(0),
            CellRef::parse_a1("A1").unwrap(),
            "local",
            CalculationOptions::default(),
        )
        .unwrap();
    assert!(
        workbook
            .apply_update_v1(SAMPLE_V5, CalculationOptions::default())
            .is_err()
    );
    assert_eq!(
        a1(&workbook),
        CellValue::Text {
            value: "local".into()
        }
    );
}

#[test]
fn a_cleared_cell_prevents_snapshot_replacement() {
    for legacy in [false, true] {
        let mut workbook =
            Workbook::open_collaborative_recalculated(SAMPLE, 5_022, CalculationOptions::default())
                .unwrap();
        let original = a1(&workbook);
        assert_ne!(original, CellValue::Empty);
        let snapshot = if legacy {
            SAMPLE_V5.to_vec()
        } else {
            workbook.encode_state_as_update_v1()
        };
        workbook
            .edit_cell(
                SheetId(0),
                CellRef::parse_a1("A1").unwrap(),
                "",
                CalculationOptions::default(),
            )
            .unwrap();
        assert_eq!(a1(&workbook), CellValue::Empty);
        assert!(workbook.can_undo());
        let result = workbook.apply_update_v1(&snapshot, CalculationOptions::default());
        if legacy {
            assert!(result.is_err());
        } else {
            result.unwrap();
        }
        assert_eq!(a1(&workbook), CellValue::Empty);
        assert!(workbook.can_undo());
        workbook.undo(CalculationOptions::default()).unwrap();
        assert_eq!(a1(&workbook), original);
    }
}

/// Adoption is gated on the base fingerprint, so a snapshot of some other
/// workbook is never taken as this one's state. (Merging one is a separate,
/// older problem: it lands a partial contamination either way.)
#[test]
fn a_snapshot_from_another_workbook_is_never_adopted() {
    let mut workbook =
        Workbook::open_collaborative_recalculated(SAMPLE, 5_008, CalculationOptions::default())
            .unwrap();
    let before = workbook
        .model()
        .sheets
        .iter()
        .map(|sheet| sheet.name.clone())
        .collect::<Vec<_>>();
    let _ = workbook.apply_update_v1(SHOWCASE_V5, CalculationOptions::default());
    assert_eq!(
        workbook
            .model()
            .sheets
            .iter()
            .map(|sheet| sheet.name.clone())
            .collect::<Vec<_>>(),
        before
    );
}
