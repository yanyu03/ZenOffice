"""Threshold and failure safety, plus an isolated owned-process cancellation."""
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import threading
import time
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('zen_quota_guard', Path(__file__).with_name('guard.py'))
g = importlib.util.module_from_spec(spec)
spec.loader.exec_module(g)


def payload(used):
    return {'rateLimits': {'primary': {'usedPercent': used}}}


class QuotaTests(unittest.TestCase):
    def test_effective_minimum_prefers_multi_bucket_view(self):
        p = {'rateLimits': {'primary': {'usedPercent': 99}}, 'rateLimitsByLimitId': {
            'codex': {'primary': {'usedPercent': 10}, 'secondary': {'usedPercent': 86}}}}
        state = g.evaluate(p, {}, now=1000)
        self.assertEqual(state['remaining_percent'], 14)
        self.assertTrue(state['latched'])

    def test_exact_15_is_allowed(self):
        self.assertTrue(g.evaluate(payload(85), {}, now=1000)['allow_new_work'])

    def test_below_15_latches(self):
        self.assertTrue(g.evaluate(payload(85.01), {}, now=1000)['latched'])

    def test_latch_survives_quota_recovery(self):
        low = g.evaluate(payload(90), {}, now=1000)
        high = g.evaluate(payload(1), low, now=1001)
        self.assertEqual(high['status'], 'HALTED')

    def test_latch_survives_failed_query(self):
        low = g.evaluate(payload(90), {}, now=1000)
        failed = g.evaluate(None, low, now=1001, error='unavailable')
        self.assertTrue(failed['latched'])
        self.assertFalse(failed['allow_new_work'])

    def test_missing_fields_are_unknown(self):
        state = g.evaluate({'rateLimits': {'primary': None}}, {}, now=1000)
        self.assertEqual(state['status'], 'UNKNOWN')
        self.assertFalse(state['allow_new_work'])

    def test_empty_map_falls_back_to_legacy(self):
        p = payload(4)
        p['rateLimitsByLimitId'] = {}
        self.assertEqual(g.evaluate(p, {}, now=1000)['remaining_percent'], 96)

    def test_malformed_multibucket_map_does_not_fall_back(self):
        p = payload(4)
        p['rateLimitsByLimitId'] = 'invalid'
        self.assertFalse(g.evaluate(p, {}, now=1000)['allow_new_work'])

    def test_bad_percentages_fail_closed(self):
        for value in [True, None, '4', -1, 101, float('nan'), float('inf')]:
            with self.subTest(value=value):
                self.assertEqual(g.evaluate(payload(value), {}, now=1000)['status'], 'UNKNOWN')

    def test_invalid_nonempty_window_does_not_hide_behind_good_one(self):
        p = payload(1)
        p['rateLimits']['secondary'] = {'usedPercent': 'bad'}
        self.assertFalse(g.evaluate(p, {}, now=1000)['allow_new_work'])

    def test_stale_and_future_timestamps_block(self):
        state = g.evaluate(payload(1), {}, now=1000)
        self.assertFalse(g.snapshot(state, now=1151)['allow_new_work'])
        self.assertFalse(g.snapshot(state, now=999)['allow_new_work'])

    def test_query_failure_blocks_previously_ready_state(self):
        state = g.evaluate(payload(1), {}, now=1000)
        self.assertFalse(g.evaluate(None, state, now=1001, error='offline')['allow_new_work'])

    def test_saved_permission_flag_is_not_trusted(self):
        state = {'allow_new_work': True, 'remaining_percent': 1, 'checked_unix': 1000}
        self.assertEqual(g.snapshot(state, now=1000)['status'], 'HALTED')

    def test_explicit_resume_requires_fresh_nonlow_quota(self):
        low = g.evaluate(payload(90), {})
        with self.assertRaises(ValueError):
            g.resume_state(low)
        high = g.evaluate(payload(4), low)
        self.assertTrue(g.resume_state(high)['allow_new_work'])
        high['checked_unix'] -= 151
        with self.assertRaises(ValueError):
            g.resume_state(high)

    def test_corrupt_state_retains_existing_alarm(self):
        with tempfile.TemporaryDirectory() as name:
            directory = Path(name)
            (directory / 'state.json').write_text('corrupt')
            (directory / 'ALERT.json').write_text('{}')
            self.assertTrue(g.load_state(directory)['latched'])

    def test_alarm_marker_overrides_a_late_ready_file(self):
        with tempfile.TemporaryDirectory() as name:
            directory = Path(name)
            g.atomic_json(directory / 'state.json', g.evaluate(payload(1), {}))
            (directory / 'ALERT.json').write_text('{}')
            self.assertFalse(g.snapshot(g.load_state(directory))['allow_new_work'])

    def test_late_normal_writer_cannot_erase_halt(self):
        with tempfile.TemporaryDirectory() as name:
            directory = Path(name)
            ready = g.evaluate(payload(1), {})
            low = g.evaluate(payload(90), {})
            with patch('winsound.MessageBeep', create=True):
                g.persist(directory, low, alarm=True)
            actual = g.persist(directory, ready)
            self.assertFalse(actual['allow_new_work'])
            self.assertTrue(g.load_state(directory)['latched'])

    def test_explicit_resume_clears_persisted_alarm(self):
        with tempfile.TemporaryDirectory() as name:
            directory = Path(name)
            low = g.evaluate(payload(90), {})
            with patch('winsound.MessageBeep', create=True):
                g.persist(directory, low, alarm=True)
            ready = g.resume_state(g.evaluate(payload(1), low))
            g.persist(directory, ready, allow_clear=True)
            self.assertFalse((directory / 'ALERT.json').exists())
            self.assertTrue(g.snapshot(g.load_state(directory))['allow_new_work'])

    def test_missing_gate_prevents_child_creation(self):
        with tempfile.TemporaryDirectory() as name:
            self.assertEqual(g.run_guarded(['not-an-executable'], Path(name)), 75)

    def test_owned_child_normal_completion_keeps_its_exit_code(self):
        with tempfile.TemporaryDirectory() as name:
            directory = Path(name)
            g.persist(directory, g.evaluate(payload(1), {}))
            result = g.run_guarded([sys.executable, '-c', 'raise SystemExit(6)'], directory, poll=0.05)
            self.assertEqual(result, 6)
            record = json.loads(next(directory.glob('command-*.json')).read_text())
            self.assertEqual(record['status'], 'FAILED')

    def test_live_owned_child_stops_when_quota_gate_closes(self):
        with tempfile.TemporaryDirectory() as name:
            directory = Path(name)
            g.persist(directory, g.evaluate(payload(1), {}))
            def close_gate():
                time.sleep(0.4)
                g.persist(directory, g.evaluate(payload(90), g.load_state(directory)))
            worker = threading.Thread(target=close_gate)
            worker.start()
            started = time.monotonic()
            result = g.run_guarded([sys.executable, '-c', 'import time; time.sleep(30)'], directory, poll=0.05)
            worker.join(timeout=2)
            self.assertEqual(result, 75)
            self.assertLess(time.monotonic() - started, 10)
            record = json.loads(next(directory.glob('command-*.json')).read_text())
            self.assertTrue(record['confirmed_direct_child_stopped'])
            self.assertFalse(record['descendant_stop_verified'])

    def test_low_simulation_never_claims_desktop_interrupt(self):
        state = g.evaluate(payload(90), {})
        self.assertFalse(state['desktop_hard_interrupt_available'])

    def test_rpc_rejects_inference_and_thread_mutation(self):
        rpc = g.Rpc.__new__(g.Rpc)
        for method in ['turn/start', 'turn/interrupt', 'thread/goal/set']:
            with self.assertRaises(ValueError):
                rpc.call(method)


if __name__ == '__main__':
    unittest.main()
