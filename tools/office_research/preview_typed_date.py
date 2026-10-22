"""Read-only date interpretation card for owned initial typed business inputs.

Not an XLSX author/exporter, spreadsheet renderer, recalculation or native host.
Only declared numeric date cells with the existing narrow package policy qualify.
"""
import argparse
from datetime import date, timedelta
from decimal import Decimal, InvalidOperation, ROUND_FLOOR
import hashlib
from html import escape
from io import BytesIO
import json
from pathlib import Path
import posixpath
import xml.etree.ElementTree as ET
from zipfile import ZipFile
from check_typed_package import check_package

M='{http://schemas.openxmlformats.org/spreadsheetml/2006/main}'
R='{http://schemas.openxmlformats.org/package/2006/relationships}'
ID='{http://schemas.openxmlformats.org/officeDocument/2006/relationships}id'


def interpret_serial(raw, system):
    if system not in ('Excel1900','Excel1904'):
        raise ValueError('explicit supported date system required')
    try: serial=Decimal(str(raw))
    except InvalidOperation: raise ValueError('numeric serial required') from None
    maximum=Decimal(2958466 if system=='Excel1900' else 2957004)
    if not serial.is_finite() or not 0<=serial<maximum:
        raise ValueError('serial outside bounded calendar range')
    days=int(serial.to_integral_value(rounding=ROUND_FLOOR))
    micros=(serial-days)*86400*1000000
    if micros!=micros.to_integral_value():
        raise ValueError('sub-microsecond fraction outside preview policy')
    seconds,us=divmod(int(micros),1000000)
    hour,seconds=divmod(seconds,3600);minute,second=divmod(seconds,60)
    clock=f'{hour:02}:{minute:02}:{second:02}'
    if us: clock+='.'+f'{us:06}'.rstrip('0')
    if system=='Excel1900' and days==0:
        calendar='Excel1900 day 0 (calendar unspecified)';kind='boundary'
    elif system=='Excel1900' and days==60:
        calendar='1900-02-29 (Excel fictional date)';kind='fictional'
    else:
        base=date(1904,1,1) if system=='Excel1904' else date(1899,12,31)
        offset=days if system=='Excel1904' or days<60 else days-1
        calendar=(base+timedelta(days=offset)).isoformat();kind='calendar'
    return {'system':system,'serial':str(serial),'kind':kind,'interpretation':calendar+' '+clock}


def preview(data,spec):
    check_package(data,spec)  # gate before independent date/relationship extraction
    with ZipFile(BytesIO(data)) as archive:
        wb=ET.fromstring(archive.read('xl/workbook.xml'))
        system='Excel1904' if wb.find(M+'workbookPr').get('date1904')=='1' else 'Excel1900'
        sheet=next(s for s in wb.find(M+'sheets') if s.get('name')==spec['sheet'])
        rels=ET.fromstring(archive.read('xl/_rels/workbook.xml.rels'))
        rel=next(r for r in rels if r.get('Id')==sheet.get(ID))
        path=posixpath.normpath(posixpath.join('xl',rel.get('Target')))
        root=ET.fromstring(archive.read(path))
        cell=next(c for c in root.iter(M+'c') if c.get('r')==spec['date_cell'])
        result=interpret_serial(cell.find(M+'v').text,system)
        result.update(case=spec['id'],sheet=spec['sheet'],cell=spec['date_cell'],style_index=cell.get('s'),
                      input_sha256=hashlib.sha256(data).hexdigest(),scope='independent numeric date interpretation; not native Text or layout fidelity')
    lines=[f"{result['sheet']}!{result['cell']} | {system} | raw serial {result['serial']} | style {result['style_index']}",
           result['interpretation'],'Read-only interpretation. No calculation, workbook export or source edits.',
           'ISO calendar/time is a diagnostic view; original builtin14 formatting is unchanged.']
    svg='<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="190" viewBox="0 0 1200 190"><rect width="1200" height="190" fill="white"/>'
    for i,line in enumerate(lines):
        svg+=f'<text x="20" y="{35+i*42}" font-family="Arial" font-size="{24 if i==1 else 18}" fill="#111827">{escape(line)}</text>'
    return result,svg+'</svg>'


def write_preview(plan,case_id,input_path,output):
    if plan.stat().st_size>256*1024 or input_path.stat().st_size>20*1024*1024:
        raise ValueError('input exceeds bounded preview policy')
    raw_plan=plan.read_bytes();document=json.loads(raw_plan.decode('utf-8'))
    if document.get('id')!='ENG-005-business-v2': raise ValueError('wrong plan')
    cases=[c for c in document['cases'] if c['id']==case_id]
    if len(cases)!=1: raise ValueError('ambiguous case')
    before=input_path.read_bytes();result,svg=preview(before,cases[0])
    if input_path.read_bytes()!=before: raise ValueError('source changed during read')
    with output.open('x',encoding='utf-8',newline='') as stream: stream.write(svg)
    if input_path.read_bytes()!=before: raise ValueError('source changed during output')
    result.update(plan_sha256=hashlib.sha256(raw_plan).hexdigest(),svg_sha256=hashlib.sha256(svg.encode()).hexdigest(),input_unchanged=True)
    return result


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('plan',type=Path);parser.add_argument('case_id');parser.add_argument('input',type=Path);parser.add_argument('output',type=Path)
    args=parser.parse_args()
    print(json.dumps(write_preview(args.plan,args.case_id,args.input,args.output)))
