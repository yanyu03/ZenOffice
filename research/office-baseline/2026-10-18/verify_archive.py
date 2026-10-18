"""Verify a research snapshot without modifying it or regenerating hashes."""
import hashlib
import json
from pathlib import Path

archive = Path(__file__).resolve().parent
repo = archive.parents[2]
manifest = json.loads((archive / 'archive-manifest.json').read_text(encoding='utf-8'))
for relative, expected in manifest['files'].items():
    path = (repo / relative).resolve()
    if not path.is_relative_to(repo):
        raise SystemExit('STOP: path escapes repository')
    if hashlib.sha256(path.read_bytes()).hexdigest() != expected:
        raise SystemExit('STOP: changed archive/reference file: ' + relative)
print('PASS: ' + str(len(manifest['files'])) + ' archive/reference hashes; excluded raw payloads are not remotely backed up')
