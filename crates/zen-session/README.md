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

`HistoryBudget` limits history frame count and retained encoded archive bytes, including a saved byte frame that a later branch removes from undo/redo history. A shared save-point frame is counted only once; a detached one remains charged until replaced by a successful save. It does not measure exact process heap usage. Only the current parsed projection is retained; candidate/restore parsing is transient and independently bounded by `WorkbookReadLimits`. Budget exhaustion rejects the operation and retains existing history; it never silently evicts old frames. Allocation failure from history reservation is reported before the commit boundary.

The first ENG-003 slice adds `WorkbookSession::open(path, limits, budget)` and `save_as_new(path)`. On Windows it publishes only to a new filename after checking the original source's bounded bytes; a read handle denies concurrent source writers/deleters for the check/publication interval. Saving does not retarget the original source. `from_bytes` sessions have no external source identity and explicitly refuse this file-backed save API.

Publication uses a flushed same-directory `NamedTempFile::persist_noclobber`, so an existing target (including a same-path or hardlink alias) is never replaced. This intentionally does not call the older overwrite-capable `zen_formats::save_atomic` after an existence check. Supported user paths are ordinary local paths; parent traversal, device/UNC/verbatim input, reserved filenames/alternate streams and observed reparse points are rejected. Canonical paths created internally are used only after admission. This is a trusted-parent path tier, not an atomic guarantee against hostile directory-namespace changes or power loss; Windows filesystem/permission and reparse variants still require explicit acceptance. Non-Windows publication is currently rejected.

`is_dirty()` compares current bytes with the most recent successful save point (original input initially). Undo away from that content is dirty, redo back to it is clean. Failed saving changes neither the save point, revision nor history. Semantically similar but differently packaged/recalculated content is not silently treated as byte-identical. Overwrite/in-place save, broader path support, fault injection at every write/flush/publication boundary and Office host comparisons remain pending. ENG-003 as a whole is not complete.

## Session command

Run `cargo run --locked -p zen-session --bin zen-session -- source.xlsx` on Windows. One process retains one `WorkbookSession`; commands on standard input use literal tabs between fields, with no quoting or escape expansion. Paths and sheet names may contain spaces, but not tabs/newlines. Rows and columns are one-based numeric indices. Commands are UTF-8, at most 16 KiB per line including its terminator:

```text
set<TAB>Transactions<TAB>4<TAB>4<TAB>42
set<TAB>Transactions<TAB>4<TAB>4<TAB>43
undo
get<TAB>Transactions<TAB>4<TAB>4
redo
save<TAB>new-output.xlsx
reopen<TAB>new-output.xlsx
status
quit
```

Replace `<TAB>` with a literal tab. `get` reports the stored value text (formula caches included), not a formatted display or formula edit. Each accepted command emits revision/dirty/undo/redo status; rejections go to stderr and leave the service state unchanged. A refused command makes the final exit code 2 even if later commands succeed. Malformed UTF-8 or oversized lines stop the process; dirty EOF also returns 2. Reopen and quit refuse dirty work; successful reopen replaces the session only after parsing, resetting history to the new source. Save uses the same create-new API and never overwrites. EOF does not save automatically; clients must save before closing input. This is a bounded scripting interface, not a crash-recovery transcript or stable general-purpose wire protocol.

The tests use an independent handwritten cross-sheet XLSX and the pinned business workbook. They cover two sequential inputs, independent expected caches, byte-exact history, branching, no-ops and explicit rejection. Internal fault tests exercise candidate parsing and revision exhaustion without exposing an unsafe candidate-commit API to callers. Office host comparison is a later task; Rust results do not establish Office compatibility.

Windows save fault tests use a private I/O seam in the same save transaction: a real partial temporary-file write followed by an injected error, an injected `sync_all` error, an injected publication error, and a deterministic competing target created after admission followed by real `persist_noclobber`. They compare current bytes, every history frame, cursor, revision, saved bytes, dirty/undo/redo and retained-byte accounting; verify source/previous/competing outputs and temporary cleanup; then retry using normal production I/O. The seam has no public API, environment switch or global fault state. This measures error handling and a specific ordered competing-target arrival; it does not reproduce hardware disk-full/flush failures, arbitrary concurrent directory mutation or power loss. Actual reparse-path acceptance remains pending.
