import copy
import json
from pathlib import Path
import os
import stat
import tempfile
import unittest

from calibrate import check_record, digest, frozen, initialize


class CalibrationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.tasks = self.root / 'tasks.json'
        self.tasks.write_text(json.dumps({'tasks': {'T1': {'criteria': {'C1': 'reopen matches independent expected value'}}}}))
        self.book = self.root / 'book.md'
        self.book.write_text('Frozen acceptance requirement')
        self.manifest = self.root / 'book.lock.json'
        self.manifest.write_text(json.dumps({'baseline_id': 'ZEN-ENGINE-v1.0', 'task_definitions': 'tasks.json',
                                             'files': {'tasks.json': digest(self.tasks), 'book.md': digest(self.book)}}))
        for p in [self.book, self.tasks, self.manifest]:
            p.chmod(stat.S_IREAD)

    def tearDown(self):
        for p in self.root.iterdir():
            p.chmod(stat.S_IREAD | stat.S_IWRITE)
        self.temp.cleanup()

    def record(self):
        r = initialize('T1', self.root / 'run.json', self.manifest)
        r.update(status='DONE', ended_at='2026-10-18T00:01:00Z', actual_commit='a' * 40,
                 actual_scope='one owned fixture', commands=['run independent comparison'], next_step='T2')
        r['criteria']['C1'].update(result='PASS', observation='actual value 42, expected 42', sample_count=1, evidence=['comparison.json'])
        return r

    def test_valid_record_does_not_mutate_baseline(self):
        original = self.manifest.read_bytes()
        self.assertTrue(check_record(self.record(), self.manifest))
        self.assertEqual(self.manifest.read_bytes(), original)

    def test_drift_stops_without_rehash(self):
        self.book.chmod(stat.S_IREAD | stat.S_IWRITE)
        self.book.write_text('Weakened criterion')
        with self.assertRaisesRegex(ValueError, 'hash mismatch'):
            frozen(self.manifest)

    def test_skipped_criterion_cannot_be_done(self):
        r = self.record()
        r['criteria']['C1']['result'] = 'NOT_RUN'
        r['criteria']['C1'].update(sample_count=0, evidence=[])
        with self.assertRaisesRegex(ValueError, 'DONE'):
            check_record(r, self.manifest)

    def test_no_evidence_rejected(self):
        r = self.record()
        r['criteria']['C1']['evidence'] = []
        with self.assertRaisesRegex(ValueError, 'evidence'):
            check_record(r, self.manifest)

    def test_added_criterion_rejected(self):
        r = self.record()
        r['criteria']['different'] = copy.deepcopy(r['criteria']['C1'])
        with self.assertRaisesRegex(ValueError, 'Criterion set'):
            check_record(r, self.manifest)

    def test_wrong_taskbook_rejected(self):
        r = self.record()
        r['baseline_files'] = {}
        with self.assertRaisesRegex(ValueError, 'different taskbook'):
            check_record(r, self.manifest)

    def test_requirement_text_cannot_be_weakened_in_record(self):
        r = self.record()
        r['criteria']['C1']['requirement'] = 'just check file exists'
        with self.assertRaisesRegex(ValueError, 'requirement text'):
            check_record(r, self.manifest)

    def test_not_run_cannot_claim_measured_evidence(self):
        r = self.record()
        r['status'] = 'PARTIAL'
        r['criteria']['C1']['result'] = 'NOT_RUN'
        with self.assertRaisesRegex(ValueError, 'NOT_RUN'):
            check_record(r, self.manifest)

    def test_cannot_overwrite_existing_execution_record(self):
        self.record()
        with self.assertRaises(FileExistsError):
            initialize('T1', self.root / 'run.json', self.manifest)

    @unittest.skipUnless(os.name == 'nt', 'Windows read-only attribute')
    def test_lost_readonly_attribute_stops(self):
        self.book.chmod(stat.S_IREAD | stat.S_IWRITE)
        with self.assertRaisesRegex(ValueError, 'read-only'):
            frozen(self.manifest)


if __name__ == '__main__':
    unittest.main()
