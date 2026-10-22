"""Fresh metadata for owned synthetic typed fixtures; never patches an XLSX.

Standalone construction only. The guarded artifact generator does not call this
module yet. Passing these tests does not certify a business input or Zen/Office.
"""
import re
import xml.etree.ElementTree as ET

MAIN = 'http://schemas.openxmlformats.org/spreadsheetml/2006/main'
REL = 'http://schemas.openxmlformats.org/officeDocument/2006/relationships'
PKG_REL = 'http://schemas.openxmlformats.org/package/2006/relationships'
CT = 'http://schemas.openxmlformats.org/package/2006/content-types'
PROBE = 'urn:zenoffice:synthetic-preservation-probe:v1'
DATE_STYLE_INDEX = 1


def xml(root):
    return ET.tostring(root, encoding='utf-8', xml_declaration=True)


def metadata_parts(date_system, sheet_name, case_id):
    """Return six newly authored OPC parts; callers must supply two worksheets.

    Summary is sheet1, business sheet is sheet2, and the date cell must explicitly
    reference style1. No serial conversion, worksheet/cache authoring, rendering,
    file I/O, integration, or acceptance/sealing is performed here.
    """
    if date_system not in ('Excel1900', 'Excel1904'):
        raise ValueError('date_system must be Excel1900 or Excel1904')
    if (not isinstance(sheet_name, str) or not 1 <= len(sheet_name) <= 31
            or sheet_name.casefold() == 'summary'
            or any(c in sheet_name for c in '[]:*?/\\')
            or sheet_name.startswith("'") or sheet_name.endswith("'")
            or any(ord(c) < 32 or 0xD800 <= ord(c) <= 0xDFFF or ord(c) in (0xFFFE,0xFFFF) for c in sheet_name)):
        raise ValueError('sheet_name must be a distinct valid business sheet name')
    if not isinstance(case_id, str) or not re.fullmatch(r'[A-Za-z0-9_-]{1,64}', case_id):
        raise ValueError('case_id must be a short synthetic identifier')
    def main(name, attrs=None):
        return ET.Element('{' + MAIN + '}' + name, attrs or {})
    def sub(parent, name, attrs=None):
        return ET.SubElement(parent, '{' + MAIN + '}' + name, attrs or {})
    workbook = main('workbook')
    sub(workbook, 'workbookPr', {'date1904':'1' if date_system == 'Excel1904' else '0'})
    sheets = sub(workbook, 'sheets')
    for i, name in enumerate(('Summary', sheet_name), 1):
        sub(sheets, 'sheet', {'name':name,'sheetId':str(i),'{' + REL + '}id':'rIdSheet' + str(i)})
    sub(workbook, 'calcPr', {'calcMode':'manual','fullCalcOnLoad':'0','forceFullCalc':'0'})
    styles = main('styleSheet')
    fonts = sub(styles, 'fonts', {'count':'1'})
    font = sub(fonts, 'font'); sub(font, 'sz', {'val':'11'}); sub(font, 'name', {'val':'Arial'})
    fills = sub(styles, 'fills', {'count':'2'})
    for pattern in ('none','gray125'):
        sub(sub(fills,'fill'), 'patternFill', {'patternType':pattern})
    border = sub(sub(styles,'borders',{'count':'1'}), 'border')
    for side in ('left','right','top','bottom','diagonal'): sub(border, side)
    base = {'numFmtId':'0','fontId':'0','fillId':'0','borderId':'0'}
    sub(sub(styles,'cellStyleXfs',{'count':'1'}),'xf',base)
    xfs = sub(styles, 'cellXfs', {'count':'2'})
    sub(xfs,'xf',dict(base,xfId='0'))
    sub(xfs,'xf',dict(base,numFmtId='14',xfId='0',applyNumberFormat='1'))
    sub(sub(styles,'cellStyles',{'count':'1'}),'cellStyle',{'name':'Normal','xfId':'0','builtinId':'0'})
    rels = ET.Element('{' + PKG_REL + '}Relationships')
    entries = [('rIdSheet1','worksheet','worksheets/sheet1.xml'),
               ('rIdSheet2','worksheet','worksheets/sheet2.xml'),
               ('rIdStyles','styles','styles.xml'),
               ('rIdProbe','customXml','../customXml/zen-probe.xml')]
    for identity, kind, target in entries:
        ET.SubElement(rels, '{' + PKG_REL + '}Relationship', {'Id':identity,'Type':REL+'/'+kind,'Target':target})
    root_rels = ET.Element('{' + PKG_REL + '}Relationships')
    ET.SubElement(root_rels,'{' + PKG_REL + '}Relationship',
                  {'Id':'rIdWorkbook','Type':REL+'/officeDocument','Target':'xl/workbook.xml'})
    # Packaging readers compare qualified names here; use a local default namespace.
    # Avoid process-global ElementTree namespace registration.
    types = ET.Element('Types', {'xmlns':CT})
    for ext, mime in [('rels','application/vnd.openxmlformats-package.relationships+xml'),('xml','application/xml')]:
        ET.SubElement(types,'Default',{'Extension':ext,'ContentType':mime})
    for part, kind in [('workbook','sheet.main'),('styles','styles')]:
        ET.SubElement(types,'Override',{'PartName':'/xl/'+part+'.xml',
            'ContentType':'application/vnd.openxmlformats-officedocument.spreadsheetml.'+kind+'+xml'})
    for i in (1,2):
        ET.SubElement(types,'Override',{'PartName':'/xl/worksheets/sheet'+str(i)+'.xml',
            'ContentType':'application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml'})
    probe = ET.Element('{' + PROBE + '}probe', {'case':case_id})
    probe.text = 'Owned synthetic preservation sentinel; no vendor payload.'
    return {'xl/workbook.xml':xml(workbook),'xl/styles.xml':xml(styles),
            'xl/_rels/workbook.xml.rels':xml(rels),'_rels/.rels':xml(root_rels),
            '[Content_Types].xml':xml(types),'customXml/zen-probe.xml':xml(probe)}
