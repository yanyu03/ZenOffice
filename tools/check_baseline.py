"""Validate public import identity and required fixtures; not a product test suite."""
import ast
import hashlib
import json
import re
import sys
import tomllib
import xml.etree.ElementTree as ET
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BINARY = {'.docx', '.xlsx', '.pptx', '.zip'}


def digest(path):
    data = path.read_bytes()
    if path.suffix not in BINARY:
        data = data.replace(b'\r\n', b'\n')
    return hashlib.sha256(data).hexdigest()


def main():
    manifest = json.loads((ROOT / 'docs/BASELINE_IMPORT.json').read_text(encoding='utf-8'))
    problems = []
    for name, expected in manifest['public_imported_sha256'].items():
        path = (ROOT / name).resolve()
        if not path.is_relative_to(ROOT) or not path.is_file() or digest(path) != expected:
            problems.append(f'import identity: {name}')
    for path in ROOT.rglob('*'):
        relative = path.relative_to(ROOT)
        if any(part in {'.git', 'target', '__pycache__', '.venv'} for part in relative.parts) or not path.is_file():
            continue
        if relative.parts[0] in {'wps-icons', 'qt'} or path.suffix.lower() in {'.rcc', '.kuip', '.exe', '.dll', '.pem', '.key', '.pfx', '.p12'} or path.name == '.env' or path.name.startswith('.env.'):
            problems.append(f'forbidden public file: {relative.as_posix()}')
        if path.name == 'siu_thesis_template_original.docx':
            # An optional fetched copy is allowed locally; Git excludes it.
            continue
        try:
            if path.suffix == '.py':
                ast.parse(path.read_text(encoding='utf-8'), filename=str(relative))
            elif path.suffix == '.toml':
                tomllib.loads(path.read_text(encoding='utf-8'))
            elif path.suffix == '.xml':
                ET.parse(path)
            elif path.suffix in BINARY:
                with zipfile.ZipFile(path) as archive:
                    if archive.testzip() is not None:
                        problems.append(f'corrupt archive: {relative.as_posix()}')
        except Exception as error:
            problems.append(f'parse: {relative.as_posix()}: {type(error).__name__}')
        if path.suffix not in BINARY:
            text = path.read_text(encoding='utf-8', errors='replace')
            # Report paths only; never echo suspected secret values.
            prefixes = ('gh' + 'p_', 'gh' + 'o_', 'github' + '_pat_', 'AK' + 'IA')
            if any(re.search(re.escape(prefix) + r'[A-Za-z0-9_]{16,}', text) for prefix in prefixes):
                problems.append(f'possible embedded credential: {relative.as_posix()}')
    workspace = tomllib.loads((ROOT / 'Cargo.toml').read_text())
    for member in workspace['workspace']['members']:
        if not (ROOT / member / 'Cargo.toml').is_file():
            problems.append(f'missing workspace member: {member}')
    if problems:
        print('\n'.join(problems))
        return 1
    print(f'PASS: {len(manifest["public_imported_sha256"])} imported identities; manifests, source syntax and fixture archives checked')
    print('Rust build, Rust tests and native Office acceptance are separate checks.')
    return 0


if __name__ == '__main__':
    sys.exit(main())
