import importlib.util
import json
from pathlib import Path
import tempfile
import threading
import unittest
from unittest.mock import patch, Mock

spec = importlib.util.spec_from_file_location('zen_continuity', Path(__file__).with_name('continuity.py'))
c = importlib.util.module_from_spec(spec)
spec.loader.exec_module(c)


class ContinuityTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        (self.root / 'docs/taskbooks').mkdir(parents=True)
        (self.root / 'docs/taskbooks/ZEN-ENGINE-v1.0.md').write_text('fixed baseline')
        self.data = self.root / 'target/continuity'
        self.root_patch = patch.object(c, 'ROOT', self.root)
        self.root_patch.start()
        self.git_patch = patch.object(c, 'git', side_effect=self.git)
        self.git_patch.start()

    def tearDown(self):
        self.git_patch.stop()
        self.root_patch.stop()
        self.temp.cleanup()

    def git(self, *args):
        return { 'diff': b'tracked patch', 'ls-files': b'', 'rev-parse': b'a' * 40,
                 'status': b' M tracked.rs\n' }[args[0]]

    def begin(self):
        return c.transition(self.data, 'begin', task='ENG-003', step='before edit')

    def test_checkpoint_finish_preserves_receipts_and_never_replays(self):
        a = self.begin()
        b = c.transition(self.data, 'checkpoint', cycle=a['cycle_id'], step='CI started', handles=['CI:123'])
        self.assertEqual(b['handles'], ['CI:123'])
        self.assertEqual(len(list((self.data / 'receipts').glob('*.json'))), 2)
        self.assertEqual(c.inspect(self.data)['action'], 'RECONCILE_BEFORE_NEW_WORK')
        done = c.transition(self.data, 'finish', cycle=a['cycle_id'], step='validated')
        self.assertEqual(done['status'], 'COMPLETE')
        self.assertFalse(done['auto_replay'])
        self.assertNotEqual(self.begin()['cycle_id'], a['cycle_id'])

    def test_duplicate_begin_does_not_overwrite_active_cycle(self):
        a = self.begin()
        with self.assertRaisesRegex(RuntimeError, 'ACTIVE'):
            self.begin()
        self.assertEqual(c.read(self.data / 'cycle.json'), a)

    def test_wrong_cycle_cannot_checkpoint_or_finish(self):
        a = self.begin()
        for action in ['checkpoint', 'finish']:
            with self.assertRaisesRegex(RuntimeError, 'identity'):
                c.transition(self.data, action, cycle='wrong')
        self.assertEqual(c.read(self.data / 'cycle.json'), a)

    def test_no_age_based_unlock_or_inference_of_success(self):
        a = self.begin()
        a['updated_at'] = '2000-01-01T00:00:00Z'
        c.quota.atomic_json(self.data / 'cycle.json', a)
        self.assertEqual(c.inspect(self.data)['cycle']['status'], 'ACTIVE')
        with self.assertRaises(RuntimeError):
            self.begin()

    def test_failed_checkpoint_does_not_replace_previous_receipt(self):
        a = self.begin()
        with patch.object(c, 'snapshot', side_effect=OSError('simulated disk error')):
            with self.assertRaises(OSError):
                c.transition(self.data, 'checkpoint', cycle=a['cycle_id'], step='not durable')
        self.assertEqual(c.read(self.data / 'cycle.json'), a)

    def test_oversized_patch_stops_without_active_cycle(self):
        with patch.object(c, 'MAX_BYTES', 2):
            with self.assertRaisesRegex(RuntimeError, 'budget'):
                self.begin()
        self.assertFalse((self.data / 'cycle.json').exists())

    def test_untracked_source_is_copied_and_binary_is_explicitly_excluded(self):
        (self.root / 'new.rs').write_text('owned source')
        (self.root / 'fixture.xlsx').write_bytes(b'not copied')
        def git(*args):
            return b'new.rs\0fixture.xlsx\0' if args[0] == 'ls-files' else self.git(*args)
        with patch.object(c, 'git', side_effect=git):
            a = self.begin()
        checkpoint = self.data / 'checkpoints' / a['last_checkpoint']
        self.assertEqual((checkpoint / 'untracked/new.rs').read_text(), 'owned source')
        receipt = c.read(checkpoint / 'checkpoint.json')
        self.assertEqual(receipt['excluded_untracked'], ['fixture.xlsx'])
        self.assertEqual(len(receipt['untracked_copies']), 1)

    def test_stale_command_record_requires_review_and_is_not_rewritten(self):
        folder = self.root / 'target/quota_guard'
        folder.mkdir(parents=True)
        path = folder / 'command-old.json'
        record = {'pid': 123, 'status': 'RUNNING'}
        path.write_text(json.dumps(record))
        result = c.inspect(self.data)
        self.assertEqual(result['action'], 'RECONCILE_BEFORE_NEW_WORK')
        self.assertEqual(c.read(path), record)
        self.assertFalse(result['auto_replay'])

    def test_scoped_sleep_inhibition_restores_previous_thread_state_on_failure(self):
        with patch.object(c.quota, 'set_execution_state', side_effect=[0x80000002, 1]) as setter:
            with self.assertRaises(ValueError):
                with c.quota.inhibit_idle_sleep() as active:
                    self.assertTrue(active)
                    raise ValueError('owned job failed')
            self.assertEqual(setter.call_args_list[0].args, (0x80000001,))
            self.assertEqual(setter.call_args_list[1].args, (0x80000002,))

    def test_failed_sleep_request_is_not_reported_as_active(self):
        with patch.object(c.quota, 'set_execution_state', return_value=0) as setter:
            with c.quota.inhibit_idle_sleep() as active:
                self.assertFalse(active)
            self.assertEqual(setter.call_count, 1)

    def test_simultaneous_cycle_begins_admit_exactly_one(self):
        barrier = threading.Barrier(2)
        outcomes = []
        def attempt():
            barrier.wait()
            try:
                outcomes.append(self.begin()['cycle_id'])
            except RuntimeError:
                outcomes.append(None)
        workers = [threading.Thread(target=attempt) for _ in range(2)]
        for worker in workers: worker.start()
        for worker in workers: worker.join(timeout=5)
        self.assertTrue(all(not worker.is_alive() for worker in workers))
        self.assertEqual(sum(value is not None for value in outcomes), 1)

    def test_caught_command_interruption_is_durable_and_child_is_stopped(self):
        folder = self.root / 'target/quota_guard'
        folder.mkdir(parents=True)
        quota_state = c.quota.evaluate({'rateLimits': {'primary': {'usedPercent': 1}}}, {})
        c.quota.atomic_json(folder / 'state.json', quota_state)
        child = Mock(pid=1234, returncode=None)
        child.poll.side_effect = lambda: child.returncode
        def terminate(): child.returncode = -1
        child.terminate.side_effect = terminate
        with patch.object(c.quota.subprocess, 'Popen', return_value=child), \
             patch.object(c.quota.time, 'sleep', side_effect=KeyboardInterrupt):
            with self.assertRaises(KeyboardInterrupt):
                c.quota._run_guarded(['owned-test-command'], folder, poll=1, inhibited=False)
        child.terminate.assert_called_once()
        record = c.read(next(folder.glob('command-*.json')))
        self.assertEqual(record['status'], 'INTERRUPTED')


if __name__ == '__main__':
    unittest.main()
