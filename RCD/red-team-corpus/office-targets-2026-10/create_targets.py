#!/usr/bin/env python3
"""Create one multi-scenario Office/OPC red-team target per format direction.

The DOCX/XLSX/PPTX files are authored through installed Microsoft Office COM,
not by Zen. This keeps the reference files independent from the code under test.
"""
from __future__ import annotations

import hashlib
import json
import os
import sys
import zipfile
from pathlib import Path
from xml.etree import ElementTree as ET

import win32com.client

ROOT = Path(__file__).resolve().parent


def rgb(r: int, g: int, b: int) -> int:
    return r + (g << 8) + (b << 16)


def make_docx(path: Path) -> dict:
    app = win32com.client.DispatchEx("Word.Application")
    app.Visible = False
    doc = None
    try:
        doc = app.Documents.Add()
        doc.BuiltInDocumentProperties("Title").Value = "Zen Red Team Word Fixture"
        doc.BuiltInDocumentProperties("Subject").Value = "Mixed-format DOCX interoperability target"
        doc.BuiltInDocumentProperties("Author").Value = "ZenOffice Red Team"

        sel = app.Selection
        sel.Style = -63  # wdStyleTitle; numeric built-in IDs avoid localized style names
        sel.TypeText("Quarterly Operations Review")
        sel.TypeParagraph()
        sel.Style = -75  # wdStyleSubtitle
        sel.TypeText("Reference document for Office round-trip testing")
        sel.TypeParagraph()
        sel.TypeParagraph()

        sel.Style = -2  # wdStyleHeading1
        sel.TypeText("1. Executive summary")
        sel.TypeParagraph()
        sel.Style = -1  # wdStyleNormal
        sel.Font.Name = "Arial"
        sel.Font.NameFarEast = "Microsoft YaHei"
        sel.TypeText("Revenue grew 12.5% year over year. The measured result is ")
        sel.Font.Bold = True
        sel.TypeText("$128,450.75")
        sel.Font.Bold = False
        sel.TypeText(" across three operating regions; review the detail table below.")
        sel.TypeParagraph()
        sel.TypeText("Unicode and escaping: 中文、café、naïve, <tag> & ampersand, _x0001_ literal.")
        sel.TypeParagraph()
        sel.TypeParagraph()

        sel.Style = -2  # wdStyleHeading1
        sel.TypeText("2. Regional results")
        sel.TypeParagraph()
        table = doc.Tables.Add(sel.Range, 5, 4)
        table.Borders.Enable = True
        table.Cell(1, 1).Range.Text = "North Region"
        table.Cell(1, 2).Range.Text = "District Summary"
        table.Cell(1, 3).Range.Text = "Quarter"
        table.Cell(1, 4).Range.Text = "Status"
        rows = [
            ("Metric", "Value", "Q4", "Reviewed"),
            ("Revenue", "$128,450.75", "+12.5%", "Approved"),
            ("Orders", "2,418", "+8.2%", "Approved"),
            ("Returns", "31", "-1.4%", "Monitor"),
        ]
        for ri, row in enumerate(rows, start=2):
            for ci, value in enumerate(row, start=1):
                table.Cell(ri, ci).Range.Text = value
        table.Cell(1, 1).Merge(table.Cell(1, 2))
        table.Cell(1, 1).Range.Text = "North Region — District Summary"
        for cell in table.Rows(1).Cells:
            cell.Range.Font.Bold = True
            cell.Shading.BackgroundPatternColor = 0xE2F0D9

        end_range = doc.Range(table.Range.End, table.Range.End)
        end_range.InsertParagraphAfter()
        sel.SetRange(end_range.End, end_range.End)
        sel.TypeText("Source: internal quarterly ledger. This fixture contains no real customer data.")
        sel.TypeParagraph()
        sel.TypeParagraph()

        # A second section exercises a different page orientation and section break.
        sel.InsertBreak(2)  # wdSectionBreakNextPage
        section = doc.Sections(2)
        section.PageSetup.Orientation = 1  # wdOrientLandscape
        sel.Style = -2  # wdStyleHeading1
        sel.TypeText("3. Landscape appendix")
        sel.TypeParagraph()
        wide = doc.Tables.Add(sel.Range, 3, 6)
        wide.Borders.Enable = True
        for ci, value in enumerate(("Region", "Jan", "Feb", "Mar", "Q1 Total", "Variance"), 1):
            wide.Cell(1, ci).Range.Text = value
            wide.Cell(1, ci).Range.Font.Bold = True
        for ri, row in enumerate((
            ("North", "18,200", "19,100", "20,450", "57,750", "+4.1%"),
            ("South", "16,800", "17,050", "18,300", "52,150", "+2.7%"),
        ), 2):
            for ci, value in enumerate(row, 1):
                wide.Cell(ri, ci).Range.Text = value

        for sec in doc.Sections:
            header = sec.Headers(1).Range
            header.Text = "ZEN RED TEAM FIXTURE  |  CONFIDENTIAL TEST DATA"
            header.Font.Size = 8
            footer = sec.Footers(1).Range
            footer.Text = "Office round-trip target  |  Page "
            footer.Collapse(0)
            doc.Fields.Add(footer, 33)  # wdFieldPage

        doc.SaveAs2(str(path), FileFormat=16)  # wdFormatDocumentDefault
        return {"title": "Quarterly Operations Review", "scenarios": [
            "multiple heading levels and mixed run formatting", "Unicode and XML-sensitive text",
            "merged table cells and table styling", "landscape section after portrait section",
            "headers, footers, and page-number field", "two tables with distinct dimensions",
        ]}
    finally:
        if doc is not None:
            doc.Close(SaveChanges=False)
        app.Quit()


def make_xlsx(path: Path) -> dict:
    app = win32com.client.DispatchEx("Excel.Application")
    app.Visible = False
    wb = None
    rows = [
        ("2026-01-05", "North", "Widget A", 12, 19.95),
        ("2026-01-12", "South", "Widget B", 8, 42.50),
        ("2026-01-19", "East", "Widget A", 17, 19.95),
        ("2026-02-02", "North", "Widget C", 6, 125.00),
        ("2026-02-09", "West", "Widget B", 13, 42.50),
        ("2026-02-16", "South", "Widget C", 4, 125.00),
        ("2026-03-02", "East", "Widget B", 21, 42.50),
        ("2026-03-09", "West", "Widget A", 9, 19.95),
    ]
    tax = 0.13
    expected_revenue = sum(qty * price for _, _, _, qty, price in rows)
    try:
        wb = app.Workbooks.Add()
        dash = wb.Worksheets(1)
        tx = wb.Worksheets.Add(None, dash)
        look = wb.Worksheets.Add(None, tx)
        dash.Name, tx.Name, look.Name = "Dashboard", "Transactions", "Lookups"
        dash.Range("A1:F1").Merge()
        dash.Range("A1").Value = "Quarterly Sales Dashboard"
        dash.Range("A1").Font.Bold = True
        dash.Range("A1").Font.Size = 18
        dash.Range("A3:B3").Value = (("Measure", "Value"),)
        dash.Range("A4:A6").Value = (("Gross Revenue",), ("Tax",), ("Net Total",))
        dash.Range("B4").Formula = "=SUM(Transactions!F4:F11)"
        dash.Range("B5").Formula = "=SUM(Transactions!G4:G11)"
        dash.Range("B6").Formula = "=SUM(Transactions!H4:H11)"
        dash.Range("B4:B6").NumberFormat = "$#,##0.00"
        dash.Range("A3:B3").Font.Bold = True
        dash.Range("A3:B3").Interior.Color = rgb(31, 78, 121)
        dash.Range("A3:B3").Font.Color = rgb(255, 255, 255)
        dash.Range("D3:E3").Value = (("Region", "Revenue"),)
        for idx, region in enumerate(("North", "South", "East", "West"), 4):
            dash.Cells(idx, 4).Value = region
            dash.Cells(idx, 5).Formula = f'=SUMIF(Transactions!$B$4:$B$11,D{idx},Transactions!$F$4:$F$11)'
        dash.Range("E4:E7").NumberFormat = "$#,##0.00"
        chart = dash.ChartObjects().Add(Left=330, Top=55, Width=420, Height=250)
        chart.Chart.SetSourceData(dash.Range("D3:E7"))
        chart.Chart.ChartType = 51  # xlColumnClustered
        chart.Chart.HasTitle = True
        chart.Chart.ChartTitle.Text = "Revenue by Region"
        dash.Columns("A:E").AutoFit()

        look.Range("A1:B1").Value = (("Parameter", "Value"),)
        look.Range("A2:B3").Value = (("Sales tax", tax), ("Currency", "USD"))
        look.Range("B2").NumberFormat = "0.0%"
        look.Range("A1:B1").Font.Bold = True
        look.Range("D1").Value = "Product Catalog"
        look.Range("D2:D4").Value = (("Widget A",), ("Widget B",), ("Widget C",))
        wb.Names.Add(Name="SalesTaxRate", RefersTo="=Lookups!$B$2")
        look.Columns("A:D").AutoFit()

        headers = ("Date", "Region", "Product", "Units", "Unit Price", "Revenue", "Tax", "Net")
        tx.Range("A3:H3").Value = (headers,)
        for i, (day, region, product, qty, price) in enumerate(rows, 4):
            tx.Cells(i, 1).Value = day
            tx.Cells(i, 2).Value = region
            tx.Cells(i, 3).Value = product
            tx.Cells(i, 4).Value = qty
            tx.Cells(i, 5).Value = price
            tx.Cells(i, 6).Formula = f"=D{i}*E{i}"
            tx.Cells(i, 7).Formula = f"=F{i}*SalesTaxRate"
            tx.Cells(i, 8).Formula = f"=F{i}+G{i}"
        tx.Range("A4:A11").NumberFormat = "mmm d, yyyy"
        tx.Range("E4:H11").NumberFormat = "$#,##0.00"
        tx.Range("A3:H3").Font.Bold = True
        tx.Range("A3:H3").Interior.Color = rgb(31, 78, 121)
        tx.Range("A3:H3").Font.Color = rgb(255, 255, 255)
        tx.Range("A3:H11").AutoFilter()
        tx.Activate()
        tx.Range("A4").Select()
        app.ActiveWindow.FreezePanes = True
        tx.Columns("A:H").AutoFit()
        tx.Range("F4:F11").FormatConditions.Add(1, 5, "1000")  # xlCellValue, xlGreater
        tx.Range("F4:F11").FormatConditions(1).Interior.Color = rgb(198, 239, 206)
        app.CalculateFullRebuild()
        wb.SaveAs(str(path), FileFormat=51)  # xlOpenXMLWorkbook
        return {"expected_gross_revenue": round(expected_revenue, 2), "tax_rate": tax,
                "sheets": ["Dashboard", "Transactions", "Lookups"], "scenarios": [
                    "three worksheets and cross-sheet formula references", "named range and formulas",
                    "date, currency, percentage, and calculated cells", "merged dashboard title",
                    "autofilter, frozen header, conditional formatting", "native clustered-column chart",
                ]}
    finally:
        if wb is not None:
            wb.Close(SaveChanges=False)
        app.Quit()


def make_pptx(path: Path) -> dict:
    app = win32com.client.DispatchEx("PowerPoint.Application")
    pres = app.Presentations.Add()
    try:
        pres.BuiltInDocumentProperties("Title").Value = "Zen Red Team Presentation Fixture"
        pres.BuiltInDocumentProperties("Author").Value = "ZenOffice Red Team"
        # Slide 1: title layout with subtitle.
        s1 = pres.Slides.Add(1, 1)  # ppLayoutTitle
        s1.Shapes.Title.TextFrame.TextRange.Text = "Quarterly Operations Review"
        s1.Shapes.Placeholders(2).TextFrame.TextRange.Text = "Reference deck | 2026 Q1 | 中文与 English"
        # Slide 2: styled text, preset shapes, and a connector.
        s2 = pres.Slides.Add(2, 12)  # ppLayoutBlank
        box = s2.Shapes.AddTextbox(1, 45, 35, 620, 55)
        box.TextFrame.TextRange.Text = "Key findings"
        box.TextFrame.TextRange.Font.Size = 28
        box.TextFrame.TextRange.Font.Bold = True
        box.TextFrame.TextRange.Font.Color.RGB = rgb(31, 78, 121)
        s2.Shapes.AddShape(5, 55, 130, 170, 95).Fill.ForeColor.RGB = rgb(91, 155, 213)  # rounded rect
        s2.Shapes.AddShape(9, 280, 130, 130, 95).Fill.ForeColor.RGB = rgb(112, 173, 71)  # ellipse
        s2.Shapes.AddShape(1, 470, 130, 170, 95).Fill.ForeColor.RGB = rgb(237, 125, 49)  # rect
        for shp, label in zip((s2.Shapes(2), s2.Shapes(3), s2.Shapes(4)), ("Revenue +12.5%", "Returns -1.4%", "Orders 2,418")):
            shp.TextFrame.TextRange.Text = label
            shp.TextFrame.TextRange.Font.Size = 15
        line = s2.Shapes.AddConnector(1, 225, 177, 280, 177)
        line.Line.ForeColor.RGB = rgb(89, 89, 89)
        # Slide 3: native table, Unicode, and a second text style.
        s3 = pres.Slides.Add(3, 12)
        title = s3.Shapes.AddTextbox(1, 45, 25, 650, 50)
        title.TextFrame.TextRange.Text = "Regional figures — appendix"
        title.TextFrame.TextRange.Font.Size = 24
        tbl = s3.Shapes.AddTable(4, 4, 55, 105, 610, 210).Table
        values = [
            ("Region", "Revenue", "Change", "Status"),
            ("North", "$57,750", "+4.1%", "Approved"),
            ("South", "$52,150", "+2.7%", "Approved"),
            ("East", "$44,320", "+8.0%", "Review"),
        ]
        for r, row in enumerate(values, 1):
            for c, value in enumerate(row, 1):
                cell = tbl.Cell(r, c)
                cell.Shape.TextFrame.TextRange.Text = value
                cell.Shape.TextFrame.TextRange.Font.Size = 14 if r == 1 else 12
                if r == 1:
                    cell.Shape.Fill.ForeColor.RGB = rgb(31, 78, 121)
                    cell.Shape.TextFrame.TextRange.Font.Color.RGB = rgb(255, 255, 255)
                    cell.Shape.TextFrame.TextRange.Font.Bold = True
        pres.SaveAs(str(path), 24)  # ppSaveAsOpenXMLPresentation
        slide_shapes = [s.Shapes.Count for s in pres.Slides]
        return {"title": "Quarterly Operations Review", "slides": 3,
                "shape_counts": slide_shapes, "scenarios": [
                    "title and blank slide layouts", "textbox text formatting and Unicode",
                    "rounded rectangle, ellipse, rectangle, and connector",
                    "native PowerPoint table with styled header", "core title and author metadata",
                ]}
    finally:
        pres.Close()
        app.Quit()


def make_opc(path: Path) -> dict:
    entries = {
        "[Content_Types].xml": b'''<?xml version="1.0" encoding="UTF-8"?>
<ct:Types xmlns:ct="http://schemas.openxmlformats.org/package/2006/content-types">
  <!-- Multiple defaults and an override exercise content-type discovery. -->
  <ct:Default Extension="xml" ContentType="application/xml"/>
  <ct:Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <ct:Default Extension="bin" ContentType="application/octet-stream"/>
  <ct:Override PartName="/custom/main.xml" ContentType="application/vnd.zen.redteam.main+xml"/>
</ct:Types>''',
        "_rels/.rels": b'''<?xml version="1.0" encoding="UTF-8"?>
<r:Relationships xmlns:r="http://schemas.openxmlformats.org/package/2006/relationships">
  <r:Relationship Target="custom/main.xml" Id="root-main" Type="https://example.invalid/relationships/main"/>
</r:Relationships>''',
        "custom/main.xml": '''<?xml version="1.0" encoding="UTF-8"?>
<z:main xmlns:z="https://example.invalid/zen" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006">
 <z:title>OPC multi-scenario fixture</z:title><z:text>Unicode 中文, escaped &amp; text, literal _x0001_.</z:text>
 <z:childRef relationship="child-rel"/><z:opaqueRef relationship="opaque-rel"/>
</z:main>'''.encode("utf-8"),
        "custom/_rels/main.xml.rels": b'''<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="opaque-rel" Type="https://example.invalid/relationships/opaque" Target="../data/payload.bin"/>
  <Relationship Id="child-rel" Type="https://example.invalid/relationships/child" Target="child.xml#section-a"/>
  <Relationship Id="external-rel" Type="https://example.invalid/relationships/external" Target="https://example.invalid/resource?q=1&amp;x=2" TargetMode="External"/>
</Relationships>''',
        "custom/child.xml": b'''<?xml version="1.0" encoding="UTF-8"?>
<child xmlns="https://example.invalid/zen"><value>42</value></child>''',
        "data/payload.bin": bytes(range(256)) * 4,
    }
    with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED) as zf:
        for name, data in entries.items():
            zf.writestr(name, data)
    # Sanity-check the fixture as a ZIP/XML package before declaring it a target.
    with zipfile.ZipFile(path) as zf:
        assert zf.testzip() is None
        for name in ("[Content_Types].xml", "_rels/.rels", "custom/main.xml", "custom/_rels/main.xml.rels", "custom/child.xml"):
            ET.fromstring(zf.read(name))
    return {"entries": sorted(entries), "scenarios": [
        "prefixed and default XML namespaces", "comments and nontrivial attribute ordering",
        "root and part-level relationships", "relative target and fragment",
        "external target with query escaping", "content-type defaults and override",
        "opaque binary payload preservation", "Unicode and OOXML escape-like literal text",
    ]}


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def main() -> int:
    if len(sys.argv) > 1 and sys.argv[1] == "--help":
        print("Creates Office-authored DOCX/XLSX/PPTX plus a synthetic OPC package in this directory.")
        return 0
    # Protect previously accepted reference artifacts from accidental overwrite.
    if (ROOT / "manifest.json").exists() and "--overwrite" not in sys.argv[1:]:
        raise SystemExit("manifest.json already exists; pass --overwrite only when intentionally replacing targets")
    for name in ("docx", "xlsx", "pptx", "opc"):
        (ROOT / name).mkdir(parents=True, exist_ok=True)

    docs = {
        "docx/zen_word_multiscenario.docx": make_docx(ROOT / "docx/zen_word_multiscenario.docx"),
        "xlsx/zen_spreadsheet_multiscenario.xlsx": make_xlsx(ROOT / "xlsx/zen_spreadsheet_multiscenario.xlsx"),
        "pptx/zen_presentation_multiscenario.pptx": make_pptx(ROOT / "pptx/zen_presentation_multiscenario.pptx"),
        "opc/zen_opc_multiscenario.zip": make_opc(ROOT / "opc/zen_opc_multiscenario.zip"),
    }
    manifest = {
        "corpus_id": "zen-office-red-team-targets-2026-10-v1",
        "created_utc": "2026-10-14",
        "office_source": "Locally installed Microsoft Office via COM; exact application versions are recorded below.",
        "generated_by": "create_targets.py",
        "items": [],
    }
    for rel, meta in docs.items():
        p = ROOT / rel
        manifest["items"].append({
            "path": rel.replace("\\", "/"),
            "bytes": p.stat().st_size,
            "sha256": sha256(p),
            "source": "Microsoft Office COM-authored" if rel.split("/", 1)[0] != "opc" else "Synthetic OPC fixture authored by this script",
            **meta,
        })
    manifest["office_versions"] = {}
    for app_name in ("Word.Application", "Excel.Application", "PowerPoint.Application"):
        app = win32com.client.DispatchEx(app_name)
        try:
            manifest["office_versions"][app_name] = str(app.Version)
        finally:
            app.Quit()
    (ROOT / "manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"manifest": str(ROOT / "manifest.json"), "items": manifest["items"], "office_versions": manifest["office_versions"]}, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
