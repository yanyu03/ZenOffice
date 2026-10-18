"""Validate the declared synthetic subset, never Excel/Zen host acceptance."""
import argparse
import hashlib
import json
import re
import zipfile
from datetime import date
from fractions import Fraction
from pathlib import Path
from xml.etree import ElementTree as ET

NS = {'s': 'http://schemas.openxmlformats.org/spreadsheetml/2006/main'}


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def validate(folder):
    plan = json.loads((folder / 'plan.json').read_text(encoding='utf-8'))
    assert plan['comparison']['tolerance'] == 0
    assert len(plan['cases']) == 3 and {c['id'] for c in plan['cases']} == {'budget', 'orders', 'attendance'}
    files = ['plan.json']
    for spec in plan['cases']:
        filename = spec['id'] + '.xlsx'
        with zipfile.ZipFile(folder / filename) as archive:
            assert archive.testzip() is None
            book = ET.fromstring(archive.read('xl/workbook.xml'))
            props = book.find('s:workbookPr', NS)
            assert props is None or props.get('date1904', 'false').lower() in {'false', '0'}
            sheets = book.findall('s:sheets/s:sheet', NS)
            assert len(sheets) == 1 and sheets[0].get('name') == spec['sheet']
            root = ET.fromstring(archive.read('xl/worksheets/sheet1.xml'))
            cells = {c.get('r'): c for c in root.findall('.//s:c', NS)}
            shared = []
            if 'xl/sharedStrings.xml' in archive.namelist():
                shared = [''.join(si.itertext()) for si in ET.fromstring(archive.read('xl/sharedStrings.xml'))]
            numbers = {a: Fraction(c.findtext('s:v', namespaces=NS)) for a, c in cells.items()
                       if c.get('t', 'n') == 'n' and c.find('s:f', NS) is None and c.find('s:v', NS) is not None}
            formulas = {a: '=' + c.findtext('s:f', namespaces=NS) for a, c in cells.items() if c.find('s:f', NS) is not None}
            assert formulas == spec['formulas'], (spec['id'], 'formula identity')
            for row_index, row in enumerate(spec['rows'], 4):
                label = cells[f'A{row_index}']
                if label.get('t') == 's':
                    text = shared[int(label.findtext('s:v', namespaces=NS))]
                elif label.get('t') == 'str':
                    text = label.findtext('s:v', namespaces=NS)
                else:
                    text = ''.join(label.find('s:is', NS).itertext())
                assert text == row[0]
                assert numbers[f'B{row_index}'] == Fraction(str(row[1]))
                assert numbers[f'C{row_index}'] == Fraction(str(row[2]))
            assert numbers[spec['date']['cell']] == spec['date']['serial']
            assert (date.fromisoformat(spec['date']['iso']) - date(1899, 12, 30)).days == spec['date']['serial']
            err = cells[spec['error']]
            assert err.get('t') == 'e' and err.findtext('s:v', namespaces=NS) == '#DIV/0!'
            assert [a for a, c in cells.items() if c.get('t') == 'e'] == [spec['error']]

            def evaluate(address, visiting=()):
                assert address not in visiting, 'cycle'
                if address in numbers:
                    return numbers[address]
                formula = formulas[address][1:]
                aggregate = re.fullmatch(r'SUM\(([A-Z]+)(\d+):\1(\d+)\)', formula)
                if aggregate:
                    col, first, last = aggregate.groups()
                    return sum((evaluate(f'{col}{r}', visiting + (address,)) for r in range(int(first), int(last) + 1)), Fraction(0))
                binary = re.fullmatch(r'([A-Z]+\d+)([-+*])([A-Z]+\d+)', formula)
                assert binary, ('unsupported oracle formula', formula)
                left, op, right = binary.groups()
                a, b = evaluate(left, visiting + (address,)), evaluate(right, visiting + (address,))
                return {'+': lambda: a + b, '-': lambda: a - b, '*': lambda: a * b}[op]()

            for index, stage in enumerate(spec['stages']):
                if stage['edit']:
                    edit = stage['edit']
                    assert edit['cell'] in numbers and edit['cell'] not in formulas
                    numbers[edit['cell']] = Fraction(str(edit['value']))
                for address, expected in stage['expected'].items():
                    exact = Fraction(str(expected))
                    assert evaluate(address) == exact, (spec['id'], stage['id'], address)
                    if index == 0:
                        assert Fraction(cells[address].findtext('s:v', namespaces=NS)) == exact
            files.append(filename)
    return {name: sha(folder / name) for name in files}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('folder', type=Path)
    parser.add_argument('--seal-identity', type=Path, help='Initial seal only; refuses an existing manifest')
    args = parser.parse_args()
    hashes = validate(args.folder)
    manifest_path = args.folder / 'manifest.json'
    if args.seal_identity:
        identity = json.loads(args.seal_identity.read_text(encoding='utf-8-sig'))
        with manifest_path.open('x', encoding='utf-8', newline='\n') as output:
            json.dump({'id': 'RES-002-inputs-v1', 'sha256': hashes, 'office_file_identity': identity,
                       'host_runs': 0, 'scope': 'Synthetic input preparation only; host application version not yet measured'}, output, indent=2)
            output.write('\n')
    else:
        manifest = json.loads(manifest_path.read_text(encoding='utf-8'))
        assert manifest['sha256'] == hashes, 'Frozen input/hash mismatch; do not reseal'
    print('PASS: 3 independent fixtures, initial OOXML values/formulas/error/date system; 9 exact arithmetic stage expectations; frozen hashes')
    print('Excel host runs: NOT_RUN. Zen acceptance: NOT_RUN. Oracle is a bounded rational-arithmetic cross-check.')


if __name__ == '__main__':
    main()
