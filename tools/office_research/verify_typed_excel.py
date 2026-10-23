"""Typed native receipts vs sealed literals and Zen archives. No Office imports."""
import argparse, hashlib, json, posixpath, zipfile
from pathlib import Path
from xml.etree import ElementTree as E
NS={'s':'http://schemas.openxmlformats.org/spreadsheetml/2006/main'}
R='http://schemas.openxmlformats.org/officeDocument/2006/relationships'
PIN='bc82f786f0db1c842188dbdb891b4b6ef3c2f0af5f4c026fe4ae19bf428aaad4'
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def need(condition,message):
 if not condition:raise ValueError(message)
def kind(v):
 if v is None:return 'blank'
 if type(v) is bool:return 'boolean'
 if type(v) in (int,float):return 'number'
 if type(v) is str:return 'text'
 raise ValueError('unsupported semantic kind')
def snapshots(spec):
 def key(c):return c if '!' in c else spec['sheet']+'!'+c
 state={key(c):{'value':v,'formula':None} for group in ['numbers','labels'] for c,v in spec[group].items()}
 for c,v in spec['initial_expected'].items():
  k=key(c);sheet,cell=k.split('!');f=spec['summary_formulas' if sheet=='Summary' else 'formulas'][cell]
  state[k]={'value':v,'formula':f}
 result=[json.loads(json.dumps(state))]
 for op in spec['operations']:
  k=key(op['cell']);value=op.get('value');formula=None
  if op['kind']=='formula':value=state[k]['value'];formula=op['value']
  state[k]={'value':value,'formula':formula}
  for c,v in op['expected'].items():state[key(c)]['value']=v
  result.append(json.loads(json.dumps(state)))
 return result

def xml_snapshot(path):
 with zipfile.ZipFile(path) as z:
  need(z.testzip() is None,'invalid ZIP')
  wb=E.fromstring(z.read('xl/workbook.xml'));props=wb.find('s:workbookPr',NS)
  system=props is not None and props.get('date1904') in ('1','true')
  rels=E.fromstring(z.read('xl/_rels/workbook.xml.rels'))
  targets={r.get('Id'):posixpath.normpath(posixpath.join('xl',r.get('Target',''))) for r in rels if r.get('TargetMode')!='External'}
  strings=[]
  # Excel uses the conventional path here; absence means no shared strings.
  if 'xl/sharedStrings.xml' in z.namelist():
   strings=[''.join(si.itertext()) for si in E.fromstring(z.read('xl/sharedStrings.xml'))]
  xfs=[]
  if 'xl/styles.xml' in z.namelist():
   xfs=E.fromstring(z.read('xl/styles.xml')).findall('s:cellXfs/s:xf',NS)
  cells={}
  for sheet in wb.findall('s:sheets/s:sheet',NS):
   target=targets[sheet.get('{'+R+'}id')].lstrip('/')
   root=E.fromstring(z.read(target))
   for c in root.findall('.//s:sheetData/s:row/s:c',NS):
    f=c.find('s:f',NS);v=c.find('s:v',NS);t=c.get('t','n');value=None
    if t=='inlineStr':value=''.join(c.find('s:is',NS).itertext()) if c.find('s:is',NS) is not None else ''
    elif v is not None and v.text is not None:
     if t=='s':value=strings[int(v.text)]
     elif t=='b':value=v.text=='1'
     elif t in ('str','e'):value=v.text
     else:value=float(v.text)
    cells[sheet.get('name')+'!'+c.get('r')]={'value':value,'formula':None if f is None else f.text or '', 'error':t=='e','num_fmt_id':int(xfs[int(c.get('s','0'))].get('numFmtId','0')) if xfs else 0}
  return system,cells

def compare_cells(actual,wanted,where,com=False):
 count=0
 for key,want in wanted.items():
  got=actual.get(key,{'value':None,'formula':None})
  if com:
   v=got.get('value2');vt=got.get('value_type');hf=got.get('has_formula')
   expected_type={'blank':'null','boolean':'System.Boolean','number':'System.Double','text':'System.String'}[kind(want['value'])]
   need(vt==expected_type,f'{where} {key} COM type {vt} != {expected_type}')
   f=got.get('formula') if hf else None
   if isinstance(f,str) and f.startswith('='):f=f[1:]
  else:
   v=got['value'];f=got['formula']
   need(not got.get('error',False),f'{where} {key} unexpected error cache')
  need(kind(v)==kind(want['value']) and v==want['value'],f'{where} {key} value/type mismatch: {v!r} != {want["value"]!r}')
  need(f==want['formula'],f'{where} {key} formula mismatch: {f!r} != {want["formula"]!r}')
  count+=1
 return count

def verify(fixtures,run,zen,expected_exe,expected_hash):
 fixtures,run,zen=map(Path,(fixtures,run,zen))
 need(sha(fixtures/'manifest.json')==PIN,'manifest pin')
 manifest=json.loads((fixtures/'manifest.json').read_text(encoding='utf8'))
 for name,h in manifest['sha256'].items():need(sha(fixtures/name)==h,'sealed '+name)
 receipts=sorted(run.glob('receipt-*.json'));need(bool(receipts),'missing receipt')
 receipt=json.loads(receipts[-1].read_text(encoding='utf8'))
 need(receipt['status']=='OBSERVED' and receipt['cleanup']=='QUIT_REQUESTED' and receipt['process_remaining'] is False,'incomplete/failed cleanup')
 need(receipt['manifest_sha256']==PIN,'receipt manifest')
 need(receipt['process']['id'] not in receipt['before_excel_ids'],'not isolated')
 need(receipt['process']['path'].casefold()==str(expected_exe).casefold(),'wrong executable')
 need(receipt['executable']['sha256']==expected_hash,'wrong executable hash')
 case=receipt['case'];spec=next(c for c in json.loads((fixtures/'plan.json').read_text())['cases'] if c['id']==case)
 need(receipt['source_sha256']==receipt['copy_sha256']==sha(run/'input-copy.xlsx')==manifest['sha256'][case+'.xlsx'],'source/copy identity')
 need(sha(zen/'source.xlsx')==manifest['sha256'][case+'.xlsx'],'Zen original identity')
 wanted=snapshots(spec);counts={'com_cells':0,'native_cache_cells':0,'zen_cells':0}
 names=['initial']+['step-'+str(i) for i in range(1,9)]+['reopened-'+str(i) for i in (4,5,8)]
 need([s['name'] for s in receipt['stages']]==names,'stage sequence')
 system=spec['date_system']=='Excel1904'
 date_key=spec['sheet']+'!'+spec['date_cell']
 date_format=receipt['stages'][0]['cells'][date_key]['number_format']
 need(isinstance(date_format,str) and bool(date_format),'missing native date format')
 native_changes=[]
 for stage in receipt['stages']:
  index=0 if stage['name']=='initial' else int(stage['name'].split('-')[1])
  need(stage['date1904'] is system,'COM date system')
  need(stage['cells'][date_key]['number_format']==date_format,'native date format changed')
  counts['com_cells']+=compare_cells(stage['cells'],wanted[index],stage['name'],com=True)
 need([x['step'] for x in receipt['outputs']]==[4,5,8],'saved selection')
 for entry in receipt['outputs']:
  p=run/entry['name'];need(sha(p)==entry['sha256'],'native output changed')
  sys,cells=xml_snapshot(p);need(sys is system,'native saved date system')
  need(cells[date_key]['num_fmt_id']==14,'native date format id')
  counts['native_cache_cells']+=compare_cells(cells,wanted[entry['step']],entry['name'])
  with zipfile.ZipFile(fixtures/(case+'.xlsx')) as source_zip,zipfile.ZipFile(p) as output_zip:
   before=set(source_zip.namelist());after=set(output_zip.namelist())
   native_changes.append({'step':entry['step'],'removed':sorted(before-after),'added':sorted(after-before),'changed':sorted(n for n in before&after if source_zip.read(n)!=output_zip.read(n)),'probe_preserved': 'customXml/zen-probe.xml' in after and output_zip.read('customXml/zen-probe.xml')==source_zip.read('customXml/zen-probe.xml')})
 # Original eight Zen states compare independently to the same literal plan;
 # transitive equality with COM is not a host-derived expectation.
 for i in range(1,9):
  sys,cells=xml_snapshot(zen/f'state-{i}.xlsx');need(sys is system,'Zen date system')
  need(cells[date_key]['num_fmt_id']==14,'Zen date format id')
  counts['zen_cells']+=compare_cells(cells,wanted[i],f'zen-state-{i}')
 for i in (4,5,8):
  sys,cells=xml_snapshot(zen/f'saved-{i}.xlsx');need(sys is system,'Zen saved system')
  counts['zen_cells']+=compare_cells(cells,wanted[i],f'zen-saved-{i}')
 return {'case':case,'status':'PASS','input_sha256':manifest['sha256'][case+'.xlsx'],'native_receipt_sha256':sha(receipts[-1]),'counts':counts,'stages':12,'saved_reopens':3,'native_runs':1,'native_date_format':date_format,'native_package_changes':native_changes,'native_text_equality':'NOT_REQUIRED: diagnostic only','scope':'one native run vs sealed literals and separately executed Zen states; not native Undo or rendering'}

def main():
 p=argparse.ArgumentParser();p.add_argument('fixtures',type=Path);p.add_argument('run',type=Path);p.add_argument('zen',type=Path);p.add_argument('--excel-executable',required=True);p.add_argument('--excel-sha256',required=True);p.add_argument('--new-output',required=True,type=Path);a=p.parse_args()
 result=verify(a.fixtures,a.run,a.zen,a.excel_executable,a.excel_sha256)
 with a.new_output.open('x',encoding='utf8') as f:json.dump(result,f,indent=2);f.write('\n')
 print(json.dumps(result))
if __name__=='__main__':main()
