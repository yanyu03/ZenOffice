"""Durable local cycle journal; never replays commands, unlocks quota or edits plans."""
from __future__ import annotations
import argparse
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time
import tomllib
import uuid

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('continuity_quota', ROOT / 'tools/quota_guard/guard.py')
quota = importlib.util.module_from_spec(spec)
spec.loader.exec_module(quota)
DATA = ROOT / 'target/continuity'
MAX_BYTES = 16 * 1024 * 1024
EXTENSIONS = {'.rs', '.toml', '.lock', '.py', '.ps1', '.md', '.json', '.yml', '.yaml'}


def read(path):
    return json.loads(path.read_text(encoding='utf-8'))


def git_executable():
    candidate = shutil.which('git')
    if candidate:
        return candidate
    path = Path(os.environ.get('ProgramFiles', 'C:/Program Files')) / 'Git/cmd/git.exe'
    if path.is_file():
        return str(path)
    raise RuntimeError('Installed Git not found')


def git(*args):
    return subprocess.check_output([git_executable(), *args], cwd=ROOT)


def durable_bytes(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open('xb') as stream:
        stream.write(data)
        stream.flush()
        os.fsync(stream.fileno())


def snapshot(directory, step, handles):
    token = uuid.uuid4().hex
    out = directory / 'checkpoints' / token
    out.mkdir(parents=True, exist_ok=False)
    patch = git('diff', '--binary', 'HEAD', '--')
    if len(patch) > MAX_BYTES:
        raise RuntimeError('Tracked patch exceeds checkpoint budget; archive explicitly before continuing')
    durable_bytes(out / 'working-tree.patch', patch)
    copied = []
    excluded = []
    used = len(patch)
    for raw_name in git('ls-files', '--others', '--exclude-standard', '-z').split(b'\0'):
        if not raw_name:
            continue
        name = raw_name.decode('utf-8')
        path = (ROOT / name).resolve()
        if not path.is_relative_to(ROOT) or path.suffix.lower() not in EXTENSIONS or path.name.startswith('.'):
            excluded.append(name)
            continue
        content = path.read_bytes()
        if used + len(content) > MAX_BYTES:
            raise RuntimeError('Untracked source exceeds checkpoint budget; no partial checkpoint committed')
        durable_bytes(out / 'untracked' / name, content)
        copied.append({'path': name, 'sha256': hashlib.sha256(content).hexdigest()})
        used += len(content)
    result = {'created_at': datetime.now(timezone.utc).isoformat(), 'checkpoint': token,
              'head': git('rev-parse', 'HEAD').decode().strip(),
              'git_status': git('status', '--porcelain=v1').decode('utf-8'),
              'step': step, 'handles': handles, 'untracked_copies': copied,
              'excluded_untracked': excluded, 'patch_sha256': hashlib.sha256(patch).hexdigest(),
              'source_taskbook_sha256': hashlib.sha256((ROOT / 'docs/taskbooks/ZEN-ENGINE-v1.0.md').read_bytes()).hexdigest(),
              'auto_replay': False}
    quota.atomic_json(out / 'checkpoint.json', result)
    return result


def transition(directory, action, task=None, cycle=None, step='', handles=None):
    directory.mkdir(parents=True, exist_ok=True)
    with quota.file_lock(directory / 'journal.lock', blocking=False):
        state_path = directory / 'cycle.json'
        current = read(state_path) if state_path.exists() else {}
        if action == 'begin':
            if current.get('status') == 'ACTIVE':
                raise RuntimeError('Existing ACTIVE cycle requires reconciliation; never infer completion from age')
            if not task:
                raise ValueError('A declared task is required')
            cycle = uuid.uuid4().hex
        elif action in {'checkpoint', 'finish'}:
            if current.get('status') != 'ACTIVE' or current.get('cycle_id') != cycle:
                raise RuntimeError('Cycle identity/status mismatch; no takeover or automatic retry')
            task = current['task']
        else:
            raise ValueError('Unknown journal action')
        receipt = snapshot(directory, step, handles or [])
        updated = {'cycle_id': cycle, 'task': task,
                   'status': 'COMPLETE' if action == 'finish' else 'ACTIVE',
                   'updated_at': receipt['created_at'], 'last_checkpoint': receipt['checkpoint'],
                   'step': step, 'handles': handles or [], 'auto_replay': False}
        quota.atomic_json(directory / 'receipts' / (uuid.uuid4().hex + '.json'), updated)
        quota.atomic_json(state_path, updated)
        return updated


def inspect(directory):
    state = read(directory / 'cycle.json') if (directory / 'cycle.json').exists() else {'status': 'NO_CYCLE'}
    unfinished = []
    for path in (ROOT / 'target/quota_guard').glob('command-*.json'):
        data = read(path)
        if data.get('status') in {'RUNNING', 'INTERRUPTED'}:
            unfinished.append({'record': path.name, 'pid': data.get('pid'), 'status': data['status'],
                               'decision': 'inspect live process creation time and actual output; PID alone is insufficient'})
    return {'cycle': state, 'unfinished_owned_command_records': unfinished,
            'action': 'RECONCILE_BEFORE_NEW_WORK' if state.get('status') == 'ACTIVE' or unfinished else 'NO_ACTIVE_JOURNAL',
            'auto_replay': False}


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('action', choices=['begin', 'checkpoint', 'finish', 'inspect', 'automation-status'])
    ap.add_argument('--task')
    ap.add_argument('--cycle')
    ap.add_argument('--step', default='')
    ap.add_argument('--handle', action='append', default=[])
    ap.add_argument('--data-dir', type=Path, default=DATA)
    args = ap.parse_args()
    try:
        if args.action == 'inspect':
            result = inspect(args.data_dir)
        elif args.action == 'automation-status':
            cfg = Path.home() / '.codex/automations/zenoffice/automation.toml'
            result = {'status': tomllib.loads(cfg.read_text(encoding='utf-8'))['status']}
        else:
            result = transition(args.data_dir, args.action, args.task, args.cycle, args.step, args.handle)
        print(json.dumps(result, ensure_ascii=True))
    except (OSError, RuntimeError, ValueError, subprocess.CalledProcessError) as error:
        print(json.dumps({'status': 'STOP', 'error': str(error)}, ensure_ascii=True), file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
