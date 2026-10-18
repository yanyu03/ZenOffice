"""Check a frozen taskbook and create/validate separate task execution records.

Never writes the taskbook or regenerates its hashes. --init creates a new record;
fill actual results separately, then --report checks it against frozen criteria.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import stat
import sys

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_MANIFEST = ROOT / 'docs/taskbooks/ZEN-ENGINE-v1.0.lock.json'


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read_json(path):
    return json.loads(path.read_text(encoding='utf-8-sig'))


def frozen(manifest):
    manifest = manifest.resolve()
    config = read_json(manifest)
    if config.get('baseline_id') != 'ZEN-ENGINE-v1.0':
        raise ValueError('Unrecognized taskbook identity')
    paths = []
    for name, expected in config['files'].items():
        path = (manifest.parent / name).resolve()
        if path.parent != manifest.parent:
            raise ValueError('Taskbook path escapes frozen directory')
        if digest(path) != expected:
            raise ValueError('Frozen taskbook hash mismatch: ' + name + '; stop, do not rewrite baseline')
        paths.append(path)
    # Read-only is an accidental-edit guard, not a protection against a privileged actor.
    for path in paths + [manifest]:
        if os.name == 'nt' and not (path.stat().st_file_attributes & stat.FILE_ATTRIBUTE_READONLY):
            raise ValueError('Frozen file lost read-only attribute: ' + path.name)
    tasks_file = manifest.parent / config['task_definitions']
    if tasks_file not in paths:
        raise ValueError('Task definitions are not hash protected')
    return config, read_json(tasks_file)


def check_record(record, manifest=DEFAULT_MANIFEST):
    baseline, definitions = frozen(manifest)
    if record.get('baseline_id') != baseline['baseline_id'] or record.get('baseline_files') != baseline['files']:
        raise ValueError('Execution record refers to a different taskbook')
    task_id = record.get('task_id')
    if task_id not in definitions['tasks']:
        raise ValueError('Unknown task ID')
    task = definitions['tasks'][task_id]
    if record.get('status') not in ['DONE', 'PARTIAL', 'BLOCKED', 'FAILED']:
        raise ValueError('Record is not closed with an explicit actual status')
    for field in ['started_at', 'ended_at', 'actual_commit', 'actual_scope', 'commands', 'deviations', 'untested', 'next_step']:
        if field not in record or record[field] in ('', None):
            raise ValueError('Missing actual field: ' + field)
    if not isinstance(record['commands'], list) or not record['commands']:
        raise ValueError('Commands must record at least one reproducible action')
    criteria = record.get('criteria', {})
    if set(criteria) != set(task['criteria']):
        raise ValueError('Criterion set differs from frozen task')
    for cid, actual in criteria.items():
        if actual.get('requirement') != task['criteria'][cid]:
            raise ValueError('Record changed frozen requirement text: ' + cid)
        if actual.get('result') not in ['PASS', 'FAIL', 'NOT_RUN']:
            raise ValueError('Invalid criterion result: ' + cid)
        if actual['result'] in ['PASS', 'FAIL']:
            if not actual.get('evidence') or not actual.get('observation'):
                raise ValueError('Measured criterion lacks evidence/observation: ' + cid)
            n = actual.get('sample_count')
            if isinstance(n, bool) or not isinstance(n, int) or n < 1:
                raise ValueError('Measured criterion lacks positive sample count: ' + cid)
        if actual['result'] == 'NOT_RUN' and (actual.get('sample_count') != 0 or actual.get('evidence')):
            raise ValueError('NOT_RUN must not carry measured samples/evidence: ' + cid)
    if record['status'] == 'DONE' and any(v['result'] != 'PASS' for v in criteria.values()):
        raise ValueError('DONE requires PASS for each frozen criterion; do not lower the taskbook')
    return True


def initialize(task_id, output, manifest=DEFAULT_MANIFEST):
    baseline, definitions = frozen(manifest)
    task = definitions['tasks'][task_id]
    record = {'baseline_id': baseline['baseline_id'], 'baseline_files': baseline['files'],
              'task_id': task_id, 'started_at': datetime.now(timezone.utc).isoformat(),
              'ended_at': '', 'status': 'IN_PROGRESS', 'actual_commit': '', 'actual_scope': '',
              'commands': [], 'deviations': [], 'untested': [], 'next_step': '',
              'criteria': {cid: {'requirement': requirement, 'result': 'NOT_RUN', 'observation': '',
                                 'sample_count': 0, 'evidence': []} for cid, requirement in task['criteria'].items()}}
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open('x', encoding='utf-8') as stream:
        json.dump(record, stream, ensure_ascii=False, indent=2)
        stream.write('\n')
    return record


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest', type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument('--report', type=Path)
    parser.add_argument('--init', metavar='TASK_ID')
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    if args.report and args.init:
        parser.error('Choose --init or --report')
    try:
        baseline, tasks = frozen(args.manifest)
        if args.init:
            if not args.output:
                parser.error('--init requires a new --output record path')
            initialize(args.init, args.output, args.manifest)
            print('CREATED separate execution record: ' + str(args.output))
        elif args.report:
            check_record(read_json(args.report), args.manifest)
            print('PASS: record matches frozen criteria; evidence truth still requires review')
        else:
            print('PASS: frozen taskbook hashes/read-only attributes; ' + str(len(tasks['tasks'])) + ' task definitions')
    except (ValueError, KeyError, OSError) as error:
        print('STOP: ' + str(error), file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
