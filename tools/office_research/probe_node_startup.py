"""Bounded runtime-startup observation, separate from 10s authoring checks.
No retry, artifact import, package generation or readiness grant is performed.
"""
import argparse
import json
import shutil
import subprocess
import time

NODE_SAMPLE = "console.log(JSON.stringify({runtime:'node',version:process.version,node:process.versions.node}))"


def observe(command, timeout=30):
    """Owned child only. Startup budget is not an authoring-test timeout."""
    start = time.perf_counter()
    try:
        child = subprocess.run(command, capture_output=True, encoding="utf-8",
                               timeout=timeout, check=False)
    except subprocess.TimeoutExpired:
        return {"status": "TIMEOUT", "elapsed_seconds": round(time.perf_counter()-start, 6)}
    except OSError:
        return {"status": "LAUNCH_ERROR", "elapsed_seconds": round(time.perf_counter()-start, 6)}
    receipt = {"status": "INVALID", "elapsed_seconds": round(time.perf_counter()-start, 6),
               "exit_code": child.returncode}
    # Do not publish arbitrary child output or local executable paths.
    if child.returncode:
        receipt["status"] = "CHILD_ERROR"
        return receipt
    try:
        value = json.loads(child.stdout)
        version, node = value["version"], value["node"]
        if (value["runtime"] != "node" or not isinstance(version, str)
                or not isinstance(node, str) or version != "v"+node
                or len(node) > 64 or not node or any(c not in "0123456789." for c in node)):
            return receipt
    except (ValueError, KeyError, TypeError):
        return receipt
    receipt.update(status="OBSERVED", version=version)
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--node", default=shutil.which("node"))
    args = parser.parse_args()
    if not args.node:
        parser.error("Node executable required; no skip or fallback")
    result = observe([args.node, "-e", NODE_SAMPLE])
    result["scope"] = "30s owned minimal Node startup only; authoring tests retain 10s limits"
    print(json.dumps(result))
    return 0 if result["status"] == "OBSERVED" else 1


if __name__ == "__main__":
    raise SystemExit(main())
