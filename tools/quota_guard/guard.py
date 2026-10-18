"""Local quota alarm and cooperative work gate; never reads or logs credentials.

An owned, non-inference app-server reads quota. This cannot interrupt the
desktop application's model turn; the task and its heartbeat enforce the gate.
"""
from __future__ import annotations

import argparse
import contextlib
from datetime import datetime, timezone
import json
import math
import os
from pathlib import Path
import queue
import subprocess
import sys
import threading
import time
import uuid

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_DATA = ROOT / 'target/quota_guard'
THRESHOLD = 15
MAX_AGE = 150
POLL_SECONDS = 60


def atomic_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    temp = path.with_name(path.name + '.' + uuid.uuid4().hex + '.tmp')
    try:
        temp.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
        os.replace(temp, path)
    finally:
        temp.unlink(missing_ok=True)


def load_state(directory):
    try:
        value = json.loads((directory / 'state.json').read_text(encoding='utf-8'))
        if not isinstance(value, dict):
            raise ValueError('state must be an object')
        if (directory / 'ALERT.json').is_file():
            value['latched'] = True
        return value
    except (OSError, ValueError):
        return {'status': 'UNKNOWN', 'allow_new_work': False,
                'latched': (directory / 'ALERT.json').is_file()}


def numeric(value):
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value)


def windows(payload):
    buckets = payload.get('rateLimitsByLimitId')
    if buckets is None or buckets == {}:
        legacy = payload.get('rateLimits')
        buckets = {'codex': legacy} if isinstance(legacy, dict) else {}
    elif not isinstance(buckets, dict):
        raise ValueError('invalid multi-bucket quota response')
    result = []
    for bucket_id, bucket in buckets.items():
        if not isinstance(bucket, dict):
            raise ValueError('invalid quota bucket')
        for name in ('primary', 'secondary'):
            window = bucket.get(name)
            if window is None:
                continue
            if not isinstance(window, dict):
                raise ValueError('invalid quota window')
            used = window.get('usedPercent')
            if not numeric(used) or not 0 <= used <= 100:
                raise ValueError('invalid usedPercent')
            result.append({'bucket': bucket_id, 'window': name, 'used_percent': used,
                           'remaining_percent': 100 - used,
                           'duration_minutes': window.get('windowDurationMins'),
                           'resets_at': window.get('resetsAt')})
    if not result:
        raise ValueError('no usable quota windows')
    return result


def evaluate(payload, previous, now=None, error=None):
    now = time.time() if now is None else now
    state = dict(previous)
    state.update(updated_unix=now, last_attempt_unix=now, threshold=THRESHOLD,
                 max_age_seconds=MAX_AGE, desktop_hard_interrupt_available=False)
    try:
        if error is not None:
            raise ValueError(error)
        values = windows(payload)
        remaining = min(item['remaining_percent'] for item in values)
        state.update(windows=values, remaining_percent=remaining, checked_unix=now,
                     usage_error=None)
        if remaining < THRESHOLD:
            state['latched'] = True
            state.setdefault('latched_at_unix', now)
    except (ValueError, TypeError, AttributeError) as exc:
        # Keep the previous valid value only for display, never as permission.
        state['usage_error'] = str(exc)
    return snapshot(state, now)


def snapshot(state, now=None):
    now = time.time() if now is None else now
    state = dict(state)
    checked = state.get('checked_unix')
    fresh = numeric(checked) and 0 <= now - checked <= MAX_AGE and not state.get('usage_error')
    remaining = state.get('remaining_percent')
    valid = numeric(remaining) and 0 <= remaining <= 100
    # Recheck the threshold, rather than trusting a saved allow flag.
    latched = bool(state.get('latched')) or (valid and remaining < THRESHOLD)
    state.update(fresh=bool(fresh), latched=latched,
                 allow_new_work=bool(fresh and valid and remaining >= THRESHOLD and not latched))
    state['status'] = 'HALTED' if latched else 'READY' if state['allow_new_work'] else 'UNKNOWN'
    return state


def resume_state(state):
    state = snapshot(state)
    if not state['fresh'] or not numeric(state.get('remaining_percent')) or state['remaining_percent'] < THRESHOLD:
        raise ValueError('a fresh quota at or above 15% is required for explicit resume')
    state['latched'] = False
    state.pop('latched_at_unix', None)
    return snapshot(state)


class Rpc:
    """Only initialize and account quota reads; no inference or thread mutations."""
    def __init__(self, executable):
        self.process = subprocess.Popen(
            [str(executable), 'app-server', '--listen', 'stdio://'],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
            text=True, encoding='utf-8', creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
        self.messages = queue.Queue()
        self.sequence = 0
        threading.Thread(target=self._read, daemon=True).start()
        try:
            self.call('initialize', {'clientInfo': {'name': 'zenoffice_quota_alarm', 'version': '1.0'}})
            self.process.stdin.write(json.dumps({'method': 'initialized'}) + '\n')
            self.process.stdin.flush()
        except Exception:
            self.close()
            raise

    def _read(self):
        try:
            for line in self.process.stdout:
                try:
                    self.messages.put(json.loads(line))
                except ValueError:
                    pass
        finally:
            self.messages.put({'_closed': True})

    def call(self, method, params=None):
        if method not in {'initialize', 'account/rateLimits/read'}:
            raise ValueError('outside read-only quota allowlist')
        self.sequence += 1
        request = {'id': self.sequence, 'method': method}
        if params is not None:
            request['params'] = params
        self.process.stdin.write(json.dumps(request) + '\n')
        self.process.stdin.flush()
        deadline = time.monotonic() + 12
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError('quota RPC timeout')
            try:
                message = self.messages.get(timeout=remaining)
            except queue.Empty:
                raise TimeoutError('quota RPC timeout') from None
            if message.get('_closed'):
                raise RuntimeError('quota RPC connection closed')
            if message.get('id') != self.sequence:
                continue
            if 'error' in message:
                raise RuntimeError('quota RPC rejected request')
            return message.get('result', {})

    def close(self):
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=3)


def persist(directory, state, alarm=False, allow_clear=False):
    # Serialize writers so a delayed normal sample cannot erase another
    # reader's low-quota latch. Only explicit resume can clear it.
    with file_lock(directory / 'state.lock'):
        current = load_state(directory)
        was_latched = bool(current.get('latched'))
        if was_latched and not allow_clear:
            state['latched'] = True
            state = snapshot(state)
        alarm = alarm and not was_latched
        if allow_clear:
            (directory / 'ALERT.json').unlink(missing_ok=True)
        if alarm:
            atomic_json(directory / 'ALERT.json', {'at_unix': time.time(),
                        'remaining_percent': state.get('remaining_percent'), 'reason': 'remaining_below_15',
                        'action': 'stop this task at checkpoint; pause its goal and heartbeat',
                        'desktop_hard_interrupt_verified': False})
        atomic_json(directory / 'state.json', state)
    checked = state.get('checked_unix')
    timestamp = datetime.fromtimestamp(checked, timezone.utc).isoformat() if numeric(checked) else 'unknown'
    message = (f'# ZenOffice 额度门禁\n\n状态：{state["status"]}\n\n'
               f'最小剩余：{state.get("remaining_percent", "unknown")}%。有效读数UTC：{timestamp}。\n\n'
               '低于15%锁定；过期/未知不允许新工作。锁定后必须由用户明确恢复。\n\n'
               '后台不能硬中断桌面模型；本任务与自动化在检查点停止。\n')
    (directory / 'STATUS.md').write_text(message, encoding='utf-8')
    if alarm:
        try:
            import winsound
            winsound.MessageBeep(winsound.MB_ICONHAND)
        except (ImportError, RuntimeError):
            pass
    return state


def refresh(directory, executable, rpc=None):
    previous = load_state(directory)
    try:
        rpc = rpc or Rpc(executable)
        state = evaluate(rpc.call('account/rateLimits/read'), previous)
    except Exception:
        # Do not write server output, credentials or transport diagnostics.
        state = evaluate(None, previous, error='official quota query failed')
        if rpc:
            rpc.close()
        rpc = None
    state = persist(directory, state, alarm=state['latched'] and not previous.get('latched', False))
    return state, rpc


@contextlib.contextmanager
def file_lock(path, blocking=True):
    handle = path.open('a+b')
    if handle.seek(0, os.SEEK_END) == 0:
        handle.write(b'0')
        handle.flush()
    handle.seek(0)
    try:
        if os.name == 'nt':
            import msvcrt
            msvcrt.locking(handle.fileno(), msvcrt.LK_LOCK if blocking else msvcrt.LK_NBLCK, 1)
        else:
            import fcntl
            fcntl.flock(handle.fileno(), fcntl.LOCK_EX | (0 if blocking else fcntl.LOCK_NB))
    except OSError:
        handle.close()
        raise RuntimeError('quota file lock unavailable') from None
    try:
        yield
    finally:
        handle.close()


def run_guarded(command, directory, poll=1):
    """Terminate only the direct child owned by this invocation if the gate closes."""
    if not snapshot(load_state(directory))['allow_new_work']:
        return 75
    record_path = directory / ('command-' + uuid.uuid4().hex + '.json')
    log_path = record_path.with_suffix('.log')
    with log_path.open('wb') as log:
        child = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT,
                                 creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
        record = {'pid': child.pid, 'started_unix': time.time(), 'executable': Path(command[0]).name,
                  'status': 'RUNNING', 'descendant_stop_verified': False}
        atomic_json(record_path, record)
        try:
            while child.poll() is None:
                if not snapshot(load_state(directory))['allow_new_work']:
                    child.terminate()
                    try:
                        child.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        child.kill()
                        child.wait(timeout=5)
                    record.update(status='STOPPED_BY_QUOTA_GATE', returncode=child.returncode,
                                  confirmed_direct_child_stopped=child.poll() is not None)
                    atomic_json(record_path, record)
                    return 75
                time.sleep(poll)
            record.update(status='COMPLETED' if child.returncode == 0 else 'FAILED', returncode=child.returncode)
            atomic_json(record_path, record)
            return child.returncode
        finally:
            if child.poll() is None:
                child.terminate()
                child.wait(timeout=5)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--state-dir', type=Path, default=DEFAULT_DATA)
    parser.add_argument('--codex', type=Path)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument('--check', action='store_true')
    mode.add_argument('--once', action='store_true')
    mode.add_argument('--daemon', action='store_true')
    mode.add_argument('--stop', action='store_true', help='request graceful shutdown of this monitor')
    mode.add_argument('--resume', action='store_true', help='only after explicit user resume authorization')
    mode.add_argument('--run', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    directory = args.state_dir.resolve()
    directory.mkdir(parents=True, exist_ok=True)
    if args.stop:
        atomic_json(directory / 'STOP', {'requested_unix': time.time()})
        print('Requested graceful shutdown of this quota monitor only')
        return 0
    if args.run is not None:
        if not args.run:
            parser.error('--run requires a command')
        return run_guarded(args.run, directory)
    if args.check:
        state = snapshot(load_state(directory))
        print(json.dumps(state, ensure_ascii=False))
        return 0 if state['allow_new_work'] else 75 if state['latched'] else 76
    if not args.codex or not args.codex.is_file():
        parser.error('--codex must name an installed official Codex executable')
    if args.daemon:
        with file_lock(directory / 'daemon.lock', blocking=False):
            (directory / 'STOP').unlink(missing_ok=True)
            atomic_json(directory / 'daemon.json', {'pid': os.getpid(), 'started_unix': time.time(),
                                                   'poll_seconds': POLL_SECONDS})
            rpc = None
            try:
                while True:
                    _, rpc = refresh(directory, args.codex, rpc)
                    for _ in range(POLL_SECONDS):
                        if (directory / 'STOP').exists():
                            return 0
                        time.sleep(1)
            finally:
                if rpc:
                    rpc.close()
    state, rpc = refresh(directory, args.codex)
    if rpc:
        rpc.close()
    if args.resume:
        state = resume_state(state)
        state = persist(directory, state, allow_clear=True)
    print(json.dumps(state, ensure_ascii=False))
    return 0 if state['allow_new_work'] else 75 if state['latched'] else 76


if __name__ == '__main__':
    sys.exit(main())
