import copy,json,unittest
from pathlib import Path
from verify_typed_excel import snapshots,compare_cells,xml_snapshot
ROOT=Path(__file__).resolve().parents[2]
class TypedComparison(unittest.TestCase):
 def test_all_three_initial_archives_match_literal_plan(self):
  folder=ROOT/'RCD/engine-acceptance/eng-005-business-v2'
  for spec in json.loads((folder/'plan.json').read_text())['cases']:
   with self.subTest(case=spec['id']):
    system,cells=xml_snapshot(folder/(spec['id']+'.xlsx'))
    self.assertEqual(system,spec['date_system']=='Excel1904')
    self.assertGreater(compare_cells(cells,snapshots(spec)[0],spec['id']),0)
 def test_com_boolean_number_blank_text_are_not_coerced(self):
  for v,vt,expected in [(False,'System.Boolean',0),(0.0,'System.Double',False),('', 'System.String',None),(None,'null','')]:
   with self.subTest(value=v,type=vt):
    with self.assertRaises(ValueError):compare_cells({'S!A1':{'value2':v,'value_type':vt,'has_formula':False}}, {'S!A1':{'value':expected,'formula':None}},'owned',True)
 def test_formula_and_stale_cache_differences_refuse(self):
  want={'S!A1':{'value':3,'formula':'1+2'}}
  for value,formula in [(2,'1+2'),(3,'1+1'),(3,None)]:
   with self.assertRaises(ValueError):compare_cells({'S!A1':{'value':value,'formula':formula}},want,'owned')
 def test_error_cache_never_counts_as_text(self):
  with self.assertRaises(ValueError):compare_cells({'S!A1':{'value':'#DIV/0!','formula':None,'error':True}}, {'S!A1':{'value':'#DIV/0!','formula':None}},'owned')
 def test_expected_deltas_preserve_unmentioned_cells_and_formula_identity(self):
  spec=json.loads((ROOT/'RCD/engine-acceptance/eng-005-business-v2/plan.json').read_text())['cases'][0]
  stages=snapshots(spec)
  self.assertEqual(len(stages),9)
  self.assertEqual(stages[1]['Budget!B12']['value'],'Reviewed')
  self.assertIs(stages[2]['Budget!B12']['value'],True)
  self.assertIsNone(stages[3]['Budget!B12']['value'])
  self.assertEqual(stages[5]['Budget!D4'],{'value':0,'formula':'B5-C5'})
  self.assertEqual(stages[6]['Budget!D4'],stages[5]['Budget!D4'])
  self.assertEqual(stages[7]['Budget!D4']['value'],200)
  self.assertEqual(stages[8]['Budget!D4'],{'value':None,'formula':None})
if __name__=='__main__':unittest.main()
