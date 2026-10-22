"""Read-only narrow initial-input policy checks; never seals or repairs a package.

No authoring/engine imports. PASS is only this checker, not schema/visual/Office
or business acceptance. UTF-8 XML and the two-sheet predeclared plan are bounded.
"""
import argparse
from decimal import Decimal, InvalidOperation
from io import BytesIO
import json
from pathlib import Path
import posixpath
import re
import xml.etree.ElementTree as ET
from xml.parsers import expat
from zipfile import ZipFile, BadZipFile

M='{http://schemas.openxmlformats.org/spreadsheetml/2006/main}'
R='{http://schemas.openxmlformats.org/package/2006/relationships}'
ID='{http://schemas.openxmlformats.org/officeDocument/2006/relationships}id'
CT='{http://schemas.openxmlformats.org/package/2006/content-types}'
T='http://schemas.openxmlformats.org/officeDocument/2006/relationships/'


def require(condition, reason):
    if not condition: raise ValueError(reason)


def check_package(data, spec):
    require(isinstance(data,bytes) and len(data)<=20*1024*1024,'input exceeds bounded policy')
    with ZipFile(BytesIO(data)) as z:
        entries=z.infolist(); names=[x.filename for x in entries]
        require(len(names)<=256 and len(names)==len(set(names)),'duplicate or excessive ZIP entries')
        require(all(not n.startswith('/') and ':' not in n and '\\' not in n
                    and all(p not in ('','.','..') for p in n.split('/')) for n in names),'unsafe part name')
        require(sum(x.file_size for x in entries)<=32*1024*1024
                and all(x.file_size<=4*1024*1024 and not x.flag_bits&1 for x in entries),'oversized or encrypted part')
        require(z.testzip() is None,'ZIP CRC mismatch')
        parts={name:z.read(name) for name in names}
    def parse(name):
        require(name in parts,'missing required part: '+name)
        text=parts[name].decode('utf-8-sig')
        require('<!DOCTYPE' not in text.upper() and '<!ENTITY' not in text.upper(),'XML declarations outside policy')
        return ET.fromstring(text)
    def one(root,tag):
        hits=root.findall(tag); require(len(hits)==1,'missing or duplicate '+tag)
        return hits[0]
    def relations(name,owner):
        root=parse(name);require(root.tag==R+'Relationships','wrong relationship namespace')
        result={}
        for rel in root:
            identity=rel.get('Id');target=rel.get('Target','')
            require(rel.tag==R+'Relationship' and identity and identity not in result,'invalid or duplicate relationship')
            require(rel.get('TargetMode') is None and target and not target.startswith('/')
                    and ':' not in target and '\\' not in target,'external or absolute relationship')
            resolved=posixpath.normpath(posixpath.join(owner,target))
            require(not resolved.startswith('../') and resolved in parts,'dangling relationship')
            result[identity]=(rel.get('Type'),resolved)
        return result
    package_rels=relations('_rels/.rels','')
    require(sum(v==(T+'officeDocument','xl/workbook.xml') for v in package_rels.values())==1,'missing workbook ownership')
    wb=parse('xl/workbook.xml'); require(wb.tag==M+'workbook','wrong workbook namespace')
    system=spec['date_system']; require(system in ('Excel1900','Excel1904'),'unknown plan date system')
    require(one(wb,M+'workbookPr').get('date1904')==('1' if system=='Excel1904' else '0'),'wrong explicit date system')
    calc=one(wb,M+'calcPr')
    require(all(calc.get(k)==v for k,v in {'calcMode':'manual','fullCalcOnLoad':'0','forceFullCalc':'0'}.items()),'wrong calculation policy')
    rels=relations('xl/_rels/workbook.xml.rels','xl')
    style_paths=[v[1] for v in rels.values() if v[0]==T+'styles']
    require(len(style_paths)==1,'missing styles ownership')
    styles=parse(style_paths[0]);require(styles.tag==M+'styleSheet','wrong styles namespace')
    custom=styles.find(M+'numFmts')
    require(custom is None or all(f.get('numFmtId')!='14' for f in custom),'builtin14 redefined as custom')
    xfs=one(styles,M+'cellXfs'); require(xfs.get('count')==str(len(xfs)),'xf count mismatch')
    sheets=one(wb,M+'sheets');require(len(sheets)==2,'expected two sheets')
    require(len({s.get('sheetId') for s in sheets})==2 and all(s.get('sheetId') for s in sheets),'duplicate sheet identities')
    by_sheet={}
    for sheet in sheets:
        name=sheet.get('name');require(name not in by_sheet,'duplicate sheet names')
        kind,path=rels.get(sheet.get(ID),(None,None))
        require(kind==T+'worksheet','wrong worksheet relationship')
        root=parse(path); require(root.tag==M+'worksheet','wrong worksheet namespace')
        require(root.find(M+'conditionalFormatting') is None and root.find(M+'tableParts') is None,'unsupported formatting or table parts')
        cells={}
        data_root=one(root,M+'sheetData'); row_ids=set()
        for row in data_root:
            row_id=row.get('r','')
            require(row.tag==M+'row' and re.fullmatch(r'[1-9][0-9]*',row_id) and row_id not in row_ids,'invalid or duplicate row')
            row_ids.add(row_id)
            require(all(c.tag==M+'c' and re.fullmatch(r'[A-Z]+'+row_id,c.get('r','')) for c in row),'cell outside its row')
        for cell in data_root.iter(M+'c'):
            address=cell.get('r','');require(re.fullmatch(r'[A-Z]+[1-9][0-9]*',address) and address not in cells,'bad or duplicate cell address')
            cells[address]=cell
        by_sheet[name]=(cells,path)
    require(set(by_sheet)=={'Summary',spec['sheet']},'wrong sheet names')
    require(len({v[1] for v in by_sheet.values()})==2,'worksheet target reused')
    main=by_sheet[spec['sheet']][0]
    def numeric(cell,want):
        require(cell.get('t','n')=='n' and cell.find(M+'is') is None,'expected numeric kind')
        value=one(cell,M+'v').text
        try: number=Decimal(value);expected=Decimal(str(want))
        except (InvalidOperation,TypeError): raise ValueError('invalid numeric literal') from None
        require(number.is_finite() and number==expected,'numeric/cache mismatch')
    for address,want in spec['numbers'].items():
        require(address in main,'missing numeric input')
        require(main[address].find(M+'f') is None,'numeric input unexpectedly a formula')
        numeric(main[address],want)
    for address,want in spec['labels'].items():
        require(address in main,'missing label');cell=main[address]
        require(cell.get('t')=='inlineStr' and cell.find(M+'f') is None and cell.find(M+'v') is None,'label kind outside inline-string slice')
        require(''.join(t.text or '' for t in one(cell,M+'is').iter(M+'t'))==want,'label mismatch')
    date=main[spec['date_cell']]; index=date.get('s','')
    require(index.isascii() and index.isdigit() and int(index)<len(xfs),'missing date style reference')
    xf=xfs[int(index)]
    allowed={'numFmtId','fontId','fillId','borderId','xfId','applyNumberFormat'}
    require(xf.get('numFmtId')=='14' and xf.get('xfId')=='0' and xf.get('applyNumberFormat')=='1'
            and not len(xf) and set(xf.attrib)<=allowed,'date xf outside declared policy')
    expected_formulas={spec['sheet']:spec['formulas'],'Summary':spec['summary_formulas']}
    checked=0
    for name,(cells,_) in by_sheet.items():
        actual={a for a,c in cells.items() if c.find(M+'f') is not None}
        require(actual==set(expected_formulas[name]),'unexpected or missing formula')
        for address,formula in expected_formulas[name].items():
            f=one(cells[address],M+'f')
            require(not f.attrib and f.text==formula,'formula mismatch or unsupported attributes')
            key=address if name==spec['sheet'] else name+'!'+address
            numeric(cells[address],spec['initial_expected'][key]);checked+=1
    probes=[v[1] for v in rels.values() if v[0]==T+'customXml']
    require(len(probes)==1,'missing owned preservation probe')
    probe=parse(probes[0]);require(probe.tag=='{urn:zenoffice:synthetic-preservation-probe:v1}probe'
                               and probe.get('case')==spec['id'] and bool(probe.text),'wrong probe identity')
    types=parse('[Content_Types].xml');require(types.tag==CT+'Types','wrong content types namespace')
    # Narrow compatibility gate for the observed Packaging reader; not general XML validity.
    lexical_names=[]
    lexical=expat.ParserCreate()
    lexical.StartElementHandler=lambda name,attrs: lexical_names.append(name)
    lexical.Parse(parts['[Content_Types].xml'],True)
    require(lexical_names and lexical_names[0]=='Types'
            and all(n in ('Default','Override') for n in lexical_names[1:]),
            'content types qualified names outside reader compatibility policy')
    override_list=types.findall(CT+'Override'); default_list=types.findall(CT+'Default')
    overrides={e.get('PartName'):e.get('ContentType') for e in override_list}
    defaults={e.get('Extension'):e.get('ContentType') for e in default_list}
    require(len(overrides)==len(override_list) and len(defaults)==len(default_list),'duplicate content type declarations')
    required={'xl/workbook.xml':'sheet.main',style_paths[0]:'styles',**{p:'worksheet' for _,p in by_sheet.values()}}
    for part,kind in required.items():
        require(overrides.get('/'+part)=='application/vnd.openxmlformats-officedocument.spreadsheetml.'+kind+'+xml','wrong required content type')
    require(overrides.get('/'+probes[0],defaults.get('xml'))=='application/xml','wrong probe content type')
    return {'policy':'PASS','case':spec['id'],'initial_formulas_checked':checked,
            'scope':'narrow package policy only; render/sealing/Zen/Office NOT_RUN'}


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('plan',type=Path);parser.add_argument('case_id');parser.add_argument('input',type=Path)
    args=parser.parse_args()
    try:
        require(args.plan.stat().st_size<=256*1024,'oversized plan')
        plan=json.loads(args.plan.read_text(encoding='utf-8'))
        require(plan.get('id')=='ENG-005-business-v2','wrong plan identity')
        cases=[s for s in plan['cases'] if s['id']==args.case_id];require(len(cases)==1,'ambiguous or unknown case')
        require(args.input.stat().st_size<=20*1024*1024,'oversized input')
        print(json.dumps(check_package(args.input.read_bytes(),cases[0])))
    except (ValueError,KeyError,OSError,BadZipFile,ET.ParseError,UnicodeError,TypeError,AttributeError,IndexError):
        print(json.dumps({'policy':'REFUSED','scope':'read-only package check; no repair or execution'}))
        raise SystemExit(1)
