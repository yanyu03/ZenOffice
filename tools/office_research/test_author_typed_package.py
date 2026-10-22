"""Independent fresh-author tests; all rejection specimens are predeclared."""
import argparse
import copy
from io import BytesIO
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import xml.etree.ElementTree as ET
from zipfile import ZipFile
from author_typed_package import build_candidate, write_new_candidate
from check_typed_package import check_package

ROOT=Path(__file__).resolve().parents[2]
PLAN=ROOT/'docs/engine_specs/ENG-005-business-v2.json'
M='{http://schemas.openxmlformats.org/spreadsheetml/2006/main}'


class FreshAuthorTests(unittest.TestCase):
    def setUp(self):
        self.before=PLAN.read_bytes();self.plan=json.loads(self.before)
        (ROOT/'target').mkdir(exist_ok=True)
        self.tmp=tempfile.TemporaryDirectory(prefix='fresh-author-test-',dir=ROOT/'target')
        self.addCleanup(self.tmp.cleanup);self.dir=Path(self.tmp.name)

    def tearDown(self): self.assertEqual(PLAN.read_bytes(),self.before)

    def test_three_case_bytes_pass_independent_package_policy(self):
        for spec in self.plan['cases']:
            with self.subTest(case=spec['id']):
                data=build_candidate(spec)
                self.assertEqual(check_package(data,spec)['policy'],'PASS')
                with ZipFile(BytesIO(data)) as z:
                    self.assertIsNone(z.testzip())
                    wb=ET.fromstring(z.read('xl/workbook.xml'))
                    self.assertEqual(wb.find(M+'workbookPr').get('date1904'),'1' if spec['id']=='orders' else '0')

    def test_literal_initial_caches_row_order_and_dates_independently(self):
        spec=self.plan['cases'][0]
        with ZipFile(BytesIO(build_candidate(spec))) as z:
            main=ET.fromstring(z.read('xl/worksheets/sheet2.xml'))
            rows=main.find(M+'sheetData');ids=[int(r.get('r')) for r in rows]
            self.assertEqual(ids,sorted(set(ids)))
            cells={c.get('r'):c for c in main.iter(M+'c')}
            self.assertEqual(cells['D4'].find(M+'f').text,'B4-C4')
            self.assertEqual(cells['D4'].find(M+'v').text,'1000')
            self.assertEqual(cells['D8'].find(M+'v').text,'1300')
            self.assertEqual(cells['B10'].find(M+'v').text,'46314')
            self.assertEqual(cells['B10'].get('s'),'1')
            summary=ET.fromstring(z.read('xl/worksheets/sheet1.xml'))
            by_ref={c.get('r'):c for c in summary.iter(M+'c')}
            self.assertEqual(by_ref['B3'].find(M+'v').text,'46315')

    def test_deterministic_and_does_not_mutate_case(self):
        spec=self.plan['cases'][1];before=copy.deepcopy(spec)
        self.assertEqual(build_candidate(spec),build_candidate(spec))
        self.assertEqual(spec,before)

    def test_create_new_and_existing_output_protection(self):
        output=self.dir/'new.xlsx'
        receipt=write_new_candidate(PLAN,'budget',output)
        self.assertEqual(receipt['case'],'budget')
        original=output.read_bytes()
        with self.assertRaises(FileExistsError): write_new_candidate(PLAN,'orders',output)
        self.assertEqual(output.read_bytes(),original)
        self.assertEqual(len(list(self.dir.iterdir())),1)

    def test_invalid_plan_or_case_never_creates_output(self):
        bad=self.dir/'bad.json';bad.write_text('{',encoding='utf-8')
        with self.assertRaises(ValueError): write_new_candidate(bad,'budget',self.dir/'absent.xlsx')
        with self.assertRaises(ValueError): write_new_candidate(PLAN,'not-a-case',self.dir/'absent.xlsx')
        self.assertFalse((self.dir/'absent.xlsx').exists())
        spec=copy.deepcopy(self.plan['cases'][0]);spec['initial_expected']['D4']=999
        bad.write_text(json.dumps(dict(self.plan,cases=[spec,*self.plan['cases'][1:]])),encoding='utf-8')
        with self.assertRaises(ValueError): write_new_candidate(bad,'budget',self.dir/'absent.xlsx')
        self.assertFalse((self.dir/'absent.xlsx').exists())

    def test_missing_parent_and_overlapping_cell_refused(self):
        with self.assertRaises(FileNotFoundError): write_new_candidate(PLAN,'budget',self.dir/'missing'/'new.xlsx')
        self.assertFalse((self.dir/'missing').exists())
        spec=copy.deepcopy(self.plan['cases'][0]);spec['labels']['B4']='overlap'
        with self.assertRaises(ValueError): build_candidate(spec)

    def test_unicode_space_preserved_and_bad_xml_rejected(self):
        spec=copy.deepcopy(self.plan['cases'][0]);spec['labels']['A1']=' 预算 & <核对> '
        data=build_candidate(spec)
        self.assertEqual(check_package(data,spec)['policy'],'PASS')
        with ZipFile(BytesIO(data)) as z:
            root=ET.fromstring(z.read('xl/worksheets/sheet2.xml'))
            c=next(c for c in root.iter(M+'c') if c.get('r')=='A1')
            text=c.find(M+'is').find(M+'t')
            self.assertEqual(text.text,' 预算 & <核对> ')
            self.assertEqual(text.get('{http://www.w3.org/XML/1998/namespace}space'),'preserve')
        spec['labels']['A1']='bad'+chr(0)
        with self.assertRaises(ValueError): build_candidate(spec)

    def test_fsync_failure_keeps_output_no_automatic_retry(self):
        output=self.dir/'failed.xlsx'
        with patch('author_typed_package.os.fsync',side_effect=OSError('owned injected failure')) as fsync:
            with self.assertRaises(OSError): write_new_candidate(PLAN,'budget',output)
            self.assertEqual(fsync.call_count,1)
        self.assertTrue(output.exists())
        self.assertGreater(output.stat().st_size,0)
        before=output.read_bytes()
        with self.assertRaises(FileExistsError): write_new_candidate(PLAN,'budget',output)
        self.assertEqual(output.read_bytes(),before)


if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--log',type=Path);args=parser.parse_args()
    suite=unittest.defaultTestLoader.loadTestsFromTestCase(FreshAuthorTests)
    if args.log:
        with args.log.open('x',encoding='utf-8') as stream:
            result=unittest.TextTestRunner(stream=stream,verbosity=2).run(suite)
    else: result=unittest.TextTestRunner(verbosity=2).run(suite)
    raise SystemExit(0 if result.wasSuccessful() else 1)
