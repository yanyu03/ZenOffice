"""Read-only sealed business-v2 identity and narrow package verification.

Caller must supply an independently retained manifest SHA; never reseals or edits.
This is input admission only, not evidence of human review or product acceptance.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
from check_typed_package import check_package
from preview_typed_date import preview

EXPECTED_FILES={'plan.json','budget.xlsx','orders.xlsx','attendance.xlsx'}


def digest(data): return hashlib.sha256(data).hexdigest()


def unique_object(pairs):
    result={}
    for key,value in pairs:
        if key in result: raise ValueError('duplicate JSON key')
        result[key]=value
    return result


def verify(folder,expected_manifest_sha):
    if not isinstance(expected_manifest_sha,str) or not re.fullmatch('[0-9a-f]{64}',expected_manifest_sha):
        raise ValueError('explicit lower-case manifest SHA256 required')
    manifest_path=folder/'manifest.json'
    if manifest_path.stat().st_size>256*1024: raise ValueError('oversized manifest')
    raw=manifest_path.read_bytes()
    if digest(raw)!=expected_manifest_sha: raise ValueError('manifest identity changed; do not reseal')
    manifest=json.loads(raw.decode('utf-8'),object_pairs_hook=unique_object)
    if manifest.get('id')!='ENG-005-business-v2-inputs-v1' or set(manifest.get('sha256',{}))!=EXPECTED_FILES:
        raise ValueError('wrong input set or file names')
    payloads={}
    for name,want in manifest['sha256'].items():
        if not isinstance(want,str) or not re.fullmatch('[0-9a-f]{64}',want): raise ValueError('invalid file hash')
        p=folder/name
        if p.is_symlink() or not p.is_file() or p.stat().st_size>20*1024*1024: raise ValueError('file outside input policy')
        data=p.read_bytes()
        if digest(data)!=want: raise ValueError('sealed input identity changed: '+name)
        payloads[name]=data
    if len(payloads['plan.json'])>256*1024: raise ValueError('oversized plan')
    plan=json.loads(payloads['plan.json'].decode('utf-8'),object_pairs_hook=unique_object)
    cases=plan.get('cases',[])
    if plan.get('id')!='ENG-005-business-v2' or len(cases)!=3 or {s['id'] for s in cases}!={'budget','orders','attendance'}:
        raise ValueError('wrong three-case plan')
    results=[]
    for spec in cases:
        data=payloads[spec['id']+'.xlsx']
        package=check_package(data,spec);date,_=preview(data,spec)
        results.append({'case':spec['id'],'input_sha256':digest(data),'initial_formulas_checked':package['initial_formulas_checked'],
                        'date_system':date['system'],'serial':date['serial'],'diagnostic_date':date['interpretation']})
    if manifest_path.read_bytes()!=raw or any((folder/n).read_bytes()!=b for n,b in payloads.items()):
        raise ValueError('sealed input changed during verification')
    return {'status':'PASS','manifest_sha256':expected_manifest_sha,'cases':results,'input_bytes_unchanged':True,
            'scope':'input identity and narrow package policy only; no human-review authentication or engine/native acceptance'}


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('folder',type=Path)
    parser.add_argument('--manifest-sha',required=True);args=parser.parse_args()
    print(json.dumps(verify(args.folder,args.manifest_sha)))
