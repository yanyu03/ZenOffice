"""Owned isolated integrity faults; never edits the actual sealed input set."""
import argparse
from io import BytesIO
import json
from pathlib import Path
import tempfile
import unittest
from zipfile import ZipFile
from author_typed_package import build_candidate
from verify_typed_inputs import verify, digest, unique_object

ROOT=Path(__file__).resolve().parents[2]


class SealedInputTests(unittest.TestCase):
    def setUp(self):
        (ROOT/'target').mkdir(exist_ok=True)
        self.temp=tempfile.TemporaryDirectory(dir=ROOT/'target');self.root=Path(self.temp.name)
        self.plan=(ROOT/'docs/engine_specs/ENG-005-business-v2.json').read_bytes()
        (self.root/'plan.json').write_bytes(self.plan)
        for spec in json.loads(self.plan)['cases']:(self.root/(spec['id']+'.xlsx')).write_bytes(build_candidate(spec))
        self.manifest={'id':'ENG-005-business-v2-inputs-v1','sha256':{p.name:digest(p.read_bytes()) for p in self.root.iterdir()}}
        self.pin=self.publish()

    def tearDown(self):self.temp.cleanup()

    def publish(self):
        raw=(json.dumps(self.manifest,indent=2)+'\n').encode();(self.root/'manifest.json').write_bytes(raw);return digest(raw)

    def test_valid_three_cases_and_no_writes(self):
        before={p.name:p.read_bytes() for p in self.root.iterdir()}
        result=verify(self.root,self.pin)
        self.assertEqual(result['status'],'PASS');self.assertEqual(len(result['cases']),3)
        self.assertEqual(sum(c['initial_formulas_checked'] for c in result['cases']),16)
        self.assertEqual(before,{p.name:p.read_bytes() for p in self.root.iterdir()})

    def test_manifest_tampering_and_missing_explicit_pin(self):
        with self.assertRaises(ValueError):verify(self.root,None)
        with self.assertRaises(ValueError):verify(self.root,'0'*64)
        with (self.root/'manifest.json').open('ab') as f:f.write(b' ')
        with self.assertRaisesRegex(ValueError,'manifest identity changed'):verify(self.root,self.pin)

    def test_each_file_tampering_is_detected_without_repair(self):
        for name in self.manifest['sha256']:
            p=self.root/name;original=p.read_bytes();p.write_bytes(original+b'owned fault')
            with self.subTest(name=name):
                with self.assertRaisesRegex(ValueError,'sealed input identity changed'):verify(self.root,self.pin)
                self.assertEqual(p.read_bytes(),original+b'owned fault')
            p.write_bytes(original)  # isolated test specimen only

    def test_missing_files_and_unexpected_manifest_paths(self):
        (self.root/'orders.xlsx').unlink()
        with self.assertRaises(ValueError):verify(self.root,self.pin)
        self.manifest['sha256']['../outside.xlsx']='0'*64
        pin=self.publish()
        with self.assertRaisesRegex(ValueError,'file names'):verify(self.root,pin)

    def test_rehashed_invalid_plan_still_refuses(self):
        plan=json.loads(self.plan);plan['cases'][1]['date_system']='Excel1900'
        raw=json.dumps(plan).encode();(self.root/'plan.json').write_bytes(raw)
        self.manifest['sha256']['plan.json']=digest(raw);pin=self.publish()
        with self.assertRaisesRegex(ValueError,'date system'):verify(self.root,pin)

    def test_rehashed_bad_package_still_refuses(self):
        p=self.root/'budget.xlsx'
        with ZipFile(p) as archive:parts={n:archive.read(n) for n in archive.namelist()}
        parts['xl/workbook.xml']=parts['xl/workbook.xml'].replace(b'calcMode="manual"',b'calcMode="auto"')
        stream=BytesIO()
        with ZipFile(stream,'w') as archive:
            for name,data in parts.items():archive.writestr(name,data)
        raw=stream.getvalue();p.write_bytes(raw);self.manifest['sha256']['budget.xlsx']=digest(raw);pin=self.publish()
        with self.assertRaisesRegex(ValueError,'calculation policy'):verify(self.root,pin)

    def test_duplicate_json_keys_and_duplicate_cases_refuse(self):
        with self.assertRaises(ValueError):json.loads('{"id":1,"id":2}',object_pairs_hook=unique_object)
        plan=json.loads(self.plan);plan['cases'][2]=plan['cases'][1]
        raw=json.dumps(plan).encode();(self.root/'plan.json').write_bytes(raw)
        self.manifest['sha256']['plan.json']=digest(raw);pin=self.publish()
        with self.assertRaisesRegex(ValueError,'three-case'):verify(self.root,pin)


if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--log',type=Path);args=parser.parse_args()
    suite=unittest.defaultTestLoader.loadTestsFromTestCase(SealedInputTests)
    if args.log:
        with args.log.open('x',encoding='utf-8') as stream:result=unittest.TextTestRunner(stream=stream,verbosity=2).run(suite)
    else:result=unittest.TextTestRunner(verbosity=2).run(suite)
    raise SystemExit(0 if result.wasSuccessful() else 1)
