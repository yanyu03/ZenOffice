"""Independent owned subprocess specimens; no real Node identity claim."""
import json
import subprocess
import sys
import unittest
from unittest.mock import patch
from probe_node_startup import observe


def child(code):
    return [sys.executable, "-c", code]


class StartupObservation(unittest.TestCase):
    def test_owned_receipt_and_elapsed(self):
        result = observe(child("print('{\"runtime\":\"node\",\"version\":\"v1.2.3\",\"node\":\"1.2.3\"}')"), 3)
        self.assertEqual(result["status"], "OBSERVED")
        self.assertEqual(result["version"], "v1.2.3")
        self.assertGreaterEqual(result["elapsed_seconds"], 0)

    def test_nonzero_does_not_publish_child_data(self):
        result = observe(child("import sys;print('private sentinel');sys.exit(7)"), 3)
        self.assertEqual(result["status"], "CHILD_ERROR")
        self.assertEqual(result["exit_code"], 7)
        self.assertNotIn("private sentinel", json.dumps(result))

    def test_bad_receipts_are_not_observed(self):
        for output in ["{}", "[]", "null", "not json", '{"runtime":"node","version":"v9","node":"8"}',
                       '{"runtime":"other","version":"v1","node":"1"}']:
            with self.subTest(output=output):
                self.assertEqual(observe(child("print("+repr(output)+")"), 3)["status"], "INVALID")

    def test_missing_executable_is_not_silently_skipped(self):
        result = observe(["zen-owned-nonexistent-node-probe-executable"])
        self.assertEqual(result["status"], "LAUNCH_ERROR")

    def test_slow_owned_child_times_out(self):
        result = observe(child("import time;time.sleep(2)"), 0.05)
        self.assertEqual(result["status"], "TIMEOUT")
        self.assertLess(result["elapsed_seconds"], 2)

    def test_timeout_is_not_retried(self):
        with patch("probe_node_startup.subprocess.run",
                   side_effect=subprocess.TimeoutExpired(["owned"], 0.1)) as run:
            self.assertEqual(observe(["owned"], 0.1)["status"], "TIMEOUT")
            run.assert_called_once()

if __name__ == "__main__":
    unittest.main(verbosity=2)
