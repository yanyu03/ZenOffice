"""Fresh, bounded ENG-005 candidate author. Does not repair or seal old packages.

Uses the predeclared literal initial caches after independent rational audit.
No Zen, Office or artifact-tool dependency. Output only to a NEW file inside an
already existing trusted parent; partial writes are retained, never auto-retried.
"""
import argparse
import hashlib
from io import BytesIO
import json
import math
import os
from pathlib import Path
import re
import xml.etree.ElementTree as ET
from zipfile import ZipFile, ZipInfo, ZIP_DEFLATED
from typed_metadata import metadata_parts, DATE_STYLE_INDEX
from check_typed_plan import check as audit_arithmetic

M='{http://schemas.openxmlformats.org/spreadsheetml/2006/main}'


def xml_text(value):
    if not isinstance(value,str) or not all(c in '\t\n\r' or 32<=ord(c)<=0xD7FF
            or 0xE000<=ord(c)<=0xFFFD or 0x10000<=ord(c)<=0x10FFFF for c in value):
        raise ValueError('text outside XML character contract')
    return value


def numeric(value):
    if type(value) not in (int,float) or (isinstance(value,float) and not math.isfinite(value)):
        raise ValueError('finite numeric literal required')
    return str(value)


def coordinate(address):
    match=re.fullmatch(r'([A-Z]+)([1-9][0-9]*)',address)
    if not match: raise ValueError('invalid cell coordinate')
    col=0
    for char in match[1]: col=col*26+ord(char)-64
    row=int(match[2])
    if row>1048576 or col>16384: raise ValueError('cell outside worksheet limits')
    return row,col


def build_candidate(spec):
    """Return fresh archive bytes, not an acceptance certificate."""
    audit_arithmetic(spec)
    numbers=dict(spec['numbers']);labels=dict(spec['labels']);formulas=dict(spec['formulas'])
    if (set(numbers)&set(labels) or set(numbers)&set(formulas) or set(labels)&set(formulas)
            or spec['date_cell'] not in numbers or spec['type_cell'] not in numbers):
        raise ValueError('overlapping input cells or missing probes')
    if type(numbers[spec['type_cell']]) is not type(spec['type_initial']) or numbers[spec['type_cell']]!=spec['type_initial']:
        raise ValueError('type probe initial value mismatch')
    parts=metadata_parts(spec['date_system'],spec['sheet'],spec['id'])
    summary_labels={'A1':spec['id']+' — independent typed/date fixture','A2':'Business result',
        'A3':'Date serial plus one','A5':'Source: synthetic ZenOffice business-v2 plan',
        'A6':'Planned date system: '+spec['date_system']}
    def worksheet(name,values,texts,expressions):
        cells={}
        def insert(address,kind,value):
            coordinate(address)
            if address in cells: raise ValueError('duplicate cell assignment')
            cells[address]=(kind,value)
        for address,value in values.items(): insert(address,'number',numeric(value))
        for address,value in texts.items(): insert(address,'text',xml_text(value))
        for address,value in expressions.items(): insert(address,'formula',xml_text(value))
        root=ET.Element(M+'worksheet')
        ET.SubElement(root,M+'sheetFormatPr',{'defaultRowHeight':'15'})
        cols=ET.SubElement(root,M+'cols')
        for start,end,width in [('1','1','42'),('2','6','20')]:
            ET.SubElement(cols,M+'col',{'min':start,'max':end,'width':width,'customWidth':'1'})
        data=ET.SubElement(root,M+'sheetData');current=None;row=None
        for address,(kind,value) in sorted(cells.items(),key=lambda item:coordinate(item[0])):
            number,_=coordinate(address)
            if number!=current:
                current=number;row=ET.SubElement(data,M+'row',{'r':str(number)})
            attrs={'r':address}
            if name==spec['sheet'] and address==spec['date_cell']: attrs['s']=str(DATE_STYLE_INDEX)
            if kind=='text': attrs['t']='inlineStr'
            cell=ET.SubElement(row,M+'c',attrs)
            if kind=='text':
                ET.SubElement(ET.SubElement(cell,M+'is'),M+'t',{'{http://www.w3.org/XML/1998/namespace}space':'preserve'}).text=value
            elif kind=='formula':
                ET.SubElement(cell,M+'f').text=value
                key=address if name==spec['sheet'] else name+'!'+address
                ET.SubElement(cell,M+'v').text=numeric(spec['initial_expected'][key])
            else: ET.SubElement(cell,M+'v').text=value
        return ET.tostring(root,encoding='utf-8',xml_declaration=True)
    parts['xl/worksheets/sheet1.xml']=worksheet('Summary',{},summary_labels,spec['summary_formulas'])
    parts['xl/worksheets/sheet2.xml']=worksheet(spec['sheet'],numbers,labels,formulas)
    stream=BytesIO()
    with ZipFile(stream,'w') as archive:
        for name,payload in sorted(parts.items()):
            entry=ZipInfo(name,date_time=(1980,1,1,0,0,0));entry.compress_type=ZIP_DEFLATED
            archive.writestr(entry,payload)
    return stream.getvalue()


def write_new_candidate(plan_path,case_id,output):
    if plan_path.stat().st_size>256*1024: raise ValueError('oversized plan')
    raw=plan_path.read_bytes();plan=json.loads(raw.decode('utf-8'))
    if plan.get('id')!='ENG-005-business-v2' or len(plan.get('cases',[]))!=3:
        raise ValueError('wrong plan contract')
    selected=[c for c in plan['cases'] if c['id']==case_id]
    if len(selected)!=1: raise ValueError('unknown or duplicate case')
    payload=build_candidate(selected[0])
    # All validation precedes create-new; no mkdir or overwrite fallback.
    with output.open('xb') as stream:
        stream.write(payload);stream.flush();os.fsync(stream.fileno())
    return {'kind':'fresh candidate only; render/sealing/Zen/Office NOT_RUN',
        'case':case_id,'plan_sha256':hashlib.sha256(raw).hexdigest(),
        'candidate_sha256':hashlib.sha256(payload).hexdigest(),'bytes':len(payload)}


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('plan',type=Path);parser.add_argument('case_id');parser.add_argument('output',type=Path)
    args=parser.parse_args()
    try: print(json.dumps(write_new_candidate(args.plan,args.case_id,args.output)))
    except (ValueError,KeyError,TypeError,OSError,OverflowError):
        print(json.dumps({'status':'REFUSED','scope':'candidate creation stopped; no retry, overwrite or repair'}))
        raise SystemExit(1)
