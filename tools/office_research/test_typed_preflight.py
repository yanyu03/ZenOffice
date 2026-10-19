"""Authorized fault specimens: refusal must precede dependency load and file writes."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[2]
BUILDER=ROOT/'tools/office_research/prepare_typed_cases.mjs'
PLAN=ROOT/'docs/engine_specs/ENG-005-business-v2.json'
NODE=None


class TypedPreflight(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory(prefix='typed preflight ')
        self.addCleanup(self.tmp.cleanup)
        self.root=Path(self.tmp.name)
        self.before=hashlib.sha256(PLAN.read_bytes()).hexdigest()

    def invoke(self,plan=PLAN,env=None):
        return subprocess.run([NODE,str(BUILDER),str(self.root/'no-runtime'),str(plan),
            str(self.root/'output'),str(self.root/'evidence')],capture_output=True,
            encoding='utf-8',timeout=10,env=env)

    def assert_refused_before_work(self,result):
        self.assertEqual(result.returncode,126,result.stderr)
        record=json.loads(result.stdout)
        self.assertTrue(record['stop'])
        self.assertEqual(record['task']['status'],'NOT_RUN')
        self.assertFalse((self.root/'output').exists())
        self.assertFalse((self.root/'evidence').exists())
        self.assertNotIn('ERR_MODULE_NOT_FOUND',result.stderr)
        self.assertEqual(hashlib.sha256(PLAN.read_bytes()).hexdigest(),self.before)
        return record

    def test_assess_only_needs_no_runtime_and_writes_nothing(self):
        result=subprocess.run([NODE,str(BUILDER),'--assess',str(PLAN)],cwd=self.root,
            capture_output=True,encoding='utf-8',timeout=10)
        self.assertEqual(result.returncode,0,result.stderr)
        self.assertEqual(json.loads(result.stdout)['status'],'AUTHORING_NOT_READY')
        self.assertEqual(list(self.root.iterdir()),[])
        self.assertEqual(hashlib.sha256(PLAN.read_bytes()).hexdigest(),self.before)

    def test_generation_refuses_before_artifact_import_or_outputs(self):
        record=self.assert_refused_before_work(self.invoke())
        self.assertEqual(record['status'],'AUTHORING_NOT_READY')
        self.assertEqual(record['plan_sha256'],self.before)

    def test_existing_outputs_are_not_modified(self):
        output=self.root/'output';output.mkdir();sentinel=output/'keep.xlsx';sentinel.write_bytes(b'owned sentinel')
        before=sentinel.read_bytes()
        result=self.invoke()
        self.assertEqual(result.returncode,126,result.stderr)
        self.assertEqual(sentinel.read_bytes(),before)
        self.assertEqual([x.name for x in output.iterdir()],['keep.xlsx'])
        self.assertFalse((self.root/'evidence').exists())

    def test_invalid_plan_inputs_cannot_reach_authoring(self):
        for label,content in [('malformed','{'),('wrong-kind','{"id":"private-claim"}'),('empty-cases','{"id":"ENG-005-business-v2","cases":[],"construction":{}}')]:
            with self.subTest(label=label):
                plan=self.root/(label+'.json');plan.write_text(content,encoding='utf-8')
                record=self.assert_refused_before_work(self.invoke(plan))
                self.assertEqual(record['status'],'INVALID_PLAN')
                self.assertNotIn('private-claim',result_text(record))

    def test_environment_cannot_claim_unimplemented_handlers_ready(self):
        env=dict(os.environ,ZEN_AUTHORING_READY='1',DSHA_SKIP_CHECKS='1')
        record=self.assert_refused_before_work(self.invoke(env=env))
        self.assertEqual(record['status'],'AUTHORING_NOT_READY')


def result_text(value):return json.dumps(value)


if __name__=='__main__':
    parser=argparse.ArgumentParser()
    parser.add_argument('--node',default=shutil.which('node'))
    args,remaining=parser.parse_known_args()
    if not args.node:parser.error('working Node executable required; no skip')
    NODE=args.node
    unittest.main(argv=[__file__,*remaining],verbosity=2)
