# zen-session

ENG-002 introduces an in-memory workbook session for existing finite numeric inputs. It reuses `ReadWorkbook::write_number_edit` for admission, package preservation and recalculation, then re-reads the candidate with the original limits before committing. The projected workbook is exposed only by shared reference.

```rust
use zen_engine::workbook::CellAddress;
use zen_formats::spreadsheet::WorkbookReadLimits;
use zen_session::{WorkbookSession, HistoryBudget};

let mut session = WorkbookSession::from_bytes(
    &input_bytes, &WorkbookReadLimits::default(), HistoryBudget::default(),
).unwrap();
session.set_number("Transactions", CellAddress::new(3, 3).unwrap(), 42.0).unwrap();
session.undo().unwrap();
session.redo().unwrap();
// session.bytes() is a validated candidate archive, not a completed filesystem save.
```

Each committed edit adds an immutable archive frame. Undo/redo re-read the selected frame before moving the cursor and increase the revision. A numeric no-op keeps the exact ZIP bytes, revision and redo history. A new edit after undo replaces the redo branch only after candidate preparation succeeds. A same-value no-op on an unsupported package does not grant permission for subsequent edits.

`HistoryBudget` limits frame count and the sum of retained encoded archive lengths. It does not measure exact process heap usage. Only the current parsed projection is retained; candidate/restore parsing is transient and independently bounded by `WorkbookReadLimits`. Budget exhaustion rejects the operation and retains existing history; it never silently evicts old frames. Allocation failure from history reservation is reported before the commit boundary.

Until ENG-003 adds safe saving, `is_dirty()` compares the current archive bytes with the original frame. Undo to the original clears it. Semantically similar but differently packaged/recalculated content is not silently treated as byte-identical. There is no filesystem open/save, source conflict check, mutable limits, save-point mutation, CLI session or UI in this crate yet. These require separate ENG-003 acceptance; callers should not invent unchecked save logic around the candidate bytes.

The tests use an independent handwritten cross-sheet XLSX and the pinned business workbook. They cover two sequential inputs, independent expected caches, byte-exact history, branching, no-ops and explicit rejection. Internal fault tests exercise candidate parsing and revision exhaustion without exposing an unsafe candidate-commit API to callers. Office host comparison is a later task; Rust results do not establish Office compatibility.
