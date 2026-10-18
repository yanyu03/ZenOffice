"""Native Word fields/pagination for the new, not-yet-frozen local fixture."""
from pathlib import Path
import win32com.client
import json
ROOT = Path(__file__).resolve().parents[1]
folder = ROOT / "RCD/red-team-corpus/academic-word-2026-10"
if (folder / "manifest.json").exists(): raise SystemExit("Target already frozen")
out = ROOT / "target/shipping_upgrade_20261016/paper_baseline"
out.mkdir(parents=True,exist_ok=True)
app = win32com.client.DispatchEx("Word.Application")
app.Visible=False; app.DisplayAlerts=0; app.AutomationSecurity=3
doc=None
try:
    doc=app.Documents.Open(str(folder / "zen_academic_multiscenario.docx"),ReadOnly=False,AddToRecentFiles=False)
    doc.Fields.Update(); doc.Repaginate()
    for toc in doc.TablesOfContents: toc.Update()
    doc.Repaginate(); doc.Save()
    doc.ExportAsFixedFormat(str(out / "academic.pdf"),17)
    result={"pages":doc.ComputeStatistics(2),"sections":doc.Sections.Count,"footnotes":doc.Footnotes.Count,
            "inline_shapes":doc.InlineShapes.Count,"equations":doc.OMaths.Count,"toc":doc.TablesOfContents.Count}
    (out / "word_baseline.json").write_text(json.dumps(result,indent=2),encoding="utf-8")
    print(result)
finally:
    if doc is not None: doc.Close(0)
    app.Quit()
