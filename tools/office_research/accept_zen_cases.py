"""Windows business acceptance against frozen, actual Excel observations."""
import argparse
import ctypes
import json
import re
import shutil
import subprocess
import zipfile
from fractions import Fraction
from pathlib import Path
from xml.etree import ElementTree as ET
from verify_excel_cases import NS, sha, validate


def coordinates(address):
    column, row = re.fullmatch(r'([A-Z]+)(\d+)', address).groups()
    number = 0
    for letter in column:
        number = number * 26 + ord(letter) - ord('A') + 1
    return int(row), number


def invoke(binary, source, commands, folder, name):
    script = '\n'.join(commands) + '\n'
    (folder / (name + '.commands.txt')).write_text(script, encoding='utf-8')
    completed = subprocess.run([str(binary), str(source)], input=script, text=True, encoding='utf-8',
                               capture_output=True, timeout=60)
    (folder / (name + '.stdout.txt')).write_text(completed.stdout, encoding='utf-8')
    (folder / (name + '.stderr.txt')).write_text(completed.stderr, encoding='utf-8')
    return completed


def check_value(text, expected):
    if isinstance(expected, (float, int)):
        assert Fraction(text) == Fraction(str(expected)), (text, expected)
    else:
        assert text == expected, (text, expected)


def lock_file(path):
    from ctypes import wintypes
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    create = kernel.CreateFileW
    create.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD, ctypes.c_void_p,
                       wintypes.DWORD, wintypes.DWORD, wintypes.HANDLE]
    create.restype = wintypes.HANDLE
    handle = create(str(path), 0x80000000, 0, None, 3, 0x80, None)
    assert handle != ctypes.c_void_p(-1).value, ctypes.get_last_error()
    kernel.CloseHandle.argtypes = [wintypes.HANDLE]
    kernel.CloseHandle.restype = wintypes.BOOL
    return kernel, handle


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--fixtures', type=Path, required=True)
    parser.add_argument('--native', type=Path, required=True)
    parser.add_argument('--new-output', type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    manifest = json.loads((args.fixtures / 'manifest.json').read_text())
    assert validate(args.fixtures) == manifest['sha256']
    plan = json.loads((args.fixtures / 'plan.json').read_text())
    native = json.loads(args.native.read_text(encoding='utf-8'))
    assert native['completed_runs'] == 9 and not native['unexpected_differences']
    root = args.new_output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    summary = {'scope': 'ENG-004 real Zen CLI vs recorded native Excel', 'binary_sha256': sha(binary),
               'native_summary_sha256': sha(args.native), 'cases': [], 'status': 'RUNNING'}
    for spec in plan['cases']:
        record = {'id': spec['id'], 'status': 'RUNNING'}
        summary['cases'].append(record)
        folder = root / spec['id']
        folder.mkdir()
        source = folder / 'source.xlsx'
        shutil.copyfile(args.fixtures / (spec['id'] + '.xlsx'), source)
        output = folder / 'saved.xlsx'
        commands, values_expected = [], []
        runs = [r for r in native['runs'] if r['id'].startswith(spec['id'] + '-')]
        assert len(runs) == 3
        addresses = list(runs[0]['observations'][0]['values'])

        def observe(stage):
            expected = runs[0]['observations'][stage]['values']
            assert all(r['observations'][stage]['values'] == expected for r in runs)
            for address in addresses:
                row, column = coordinates(address)
                commands.append(f'get\t{spec["sheet"]}\t{row}\t{column}')
                values_expected.append('#DIV/0!' if address == spec['error'] else expected[address])

        try:
            observe(0)
            for stage_index in (1, 2):
                edit = spec['stages'][stage_index]['edit']
                row, column = coordinates(edit['cell'])
                commands.append(f'set\t{spec["sheet"]}\t{row}\t{column}\t{edit["value"]}')
                observe(stage_index)
            for action, stage in [('undo',1),('undo',0),('redo',1),('redo',2)]:
                commands.append(action)
                observe(stage)
            commands.append(f'save\t{output}')
            commands.append('undo'); observe(1)
            commands.append('redo'); observe(2)
            commands.append(f'reopen\t{output}'); observe(3)
            commands.append('quit')
            result = invoke(binary, source, commands, folder, 'business')
            record['cli_exit'] = result.returncode
            assert result.returncode == 0, result.stderr
            values = [json.loads(m.group(1)) for line in result.stdout.splitlines()
                      if (m := re.fullmatch(r'VALUE Some\((".*")\)', line))]
            assert len(values) == len(values_expected), (len(values), len(values_expected))
            for actual, expected in zip(values, values_expected):
                check_value(actual, expected)
            assert 'OK save revision=6 dirty=false' in result.stdout
            assert 'OK undo revision=7 dirty=true' in result.stdout
            assert 'OK redo revision=8 dirty=false' in result.stdout
            assert 'OK reopen revision=0 dirty=false undo=false redo=false' in result.stdout
            assert sha(source) == manifest['sha256'][spec['id'] + '.xlsx']
            with zipfile.ZipFile(source) as before, zipfile.ZipFile(output) as after:
                allowed = {'xl/worksheets/sheet1.xml', 'xl/workbook.xml', 'xl/calcChain.xml'}
                assert set(before.namelist()) ^ set(after.namelist()) <= allowed
                preserved = []
                for name in set(before.namelist()) - allowed:
                    assert before.read(name) == after.read(name), name
                    preserved.append(name)
                sheet = ET.fromstring(after.read('xl/worksheets/sheet1.xml'))
                cells = {c.get('r'): c for c in sheet.findall('.//s:c', NS)}
                for address, formula in spec['formulas'].items():
                    assert '=' + cells[address].findtext('s:f', namespaces=NS) == formula
                for address, expected in spec['stages'][-1]['expected'].items():
                    check_value(cells[address].findtext('s:v', namespaces=NS), expected)
                assert cells[spec['error']].get('t') == 'e'
                assert cells[spec['error']].findtext('s:v', namespaces=NS) == '#DIV/0!'
            # Independently locked source refuses open; old target refuses save.
            kernel, handle = lock_file(source)
            try:
                locked = invoke(binary, source, ['status','quit'], folder, 'locked-source')
                assert locked.returncode == 2
            finally:
                assert kernel.CloseHandle(handle)
            old = folder / 'old-output.xlsx'; old.write_bytes(b'old output')
            edit = spec['stages'][1]['edit']; row, column = coordinates(edit['cell'])
            kernel, handle = lock_file(old)
            try:
                rejected = invoke(binary, source, [f'set\t{spec["sheet"]}\t{row}\t{column}\t{edit["value"]}',
                    f'save\t{old}', 'status', f'save\t{folder / "retry.xlsx"}', 'quit'], folder, 'locked-existing-output')
                assert rejected.returncode == 2
                assert 'OK status revision=1 dirty=true undo=true redo=false' in rejected.stdout
                assert 'OK save revision=1 dirty=false' in rejected.stdout
            finally:
                assert kernel.CloseHandle(handle)
            assert old.read_bytes() == b'old output'
            assert sha(source) == manifest['sha256'][spec['id'] + '.xlsx']
            record.update(status='PASS', observed_values=len(values), native_runs_compared=3,
                          source_sha256=sha(source), output_sha256=sha(output), preserved_parts=sorted(preserved))
        except Exception as error:
            record.update(status='FAIL', error=str(error))
        (root / 'summary.json').write_text(json.dumps(summary, indent=2)+'\n', encoding='utf-8')
    summary['status'] = 'PASS' if all(c['status'] == 'PASS' for c in summary['cases']) else 'FAIL'
    (root / 'summary.json').write_text(json.dumps(summary, indent=2)+'\n', encoding='utf-8')
    print(json.dumps(summary, indent=2))
    if summary['status'] != 'PASS':
        raise SystemExit(1)


if __name__ == '__main__':
    main()
