"""Compare actual Excel receipts/saved packages with predeclared expectations."""
import argparse
import json
import zipfile
from fractions import Fraction
from pathlib import Path
from xml.etree import ElementTree as ET
from verify_excel_cases import NS, sha, validate


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('fixtures', type=Path)
    parser.add_argument('runs', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    manifest = json.loads((args.fixtures / 'manifest.json').read_text(encoding='utf-8'))
    assert validate(args.fixtures) == manifest['sha256']
    plan = json.loads((args.fixtures / 'plan.json').read_text(encoding='utf-8'))
    result = {'task': 'RES-002', 'samples': 3, 'runs_per_sample': 3, 'host_runs': 0,
              'unexpected_differences': [], 'runs': [], 'scope': 'Excel vs predeclared values/formulas/errors/date system, not Zen comparison or native Undo'}
    for spec in plan['cases']:
        for run_index in range(1, 4):
            folder = args.runs / f'{spec["id"]}-{run_index:02}'
            receipt_path = sorted(folder.glob('receipt-*.json'))[-1]
            receipt = json.loads(receipt_path.read_text(encoding='utf-8'))
            assert receipt['case'] == spec['id'] and receipt['status'] == 'OBSERVED'
            assert receipt['cleanup'] == 'QUIT_REQUESTED' and receipt['process_remaining'] is False
            assert receipt['process']['id'] not in receipt['before_excel_ids']
            assert receipt['process']['path'].lower() == manifest['office_file_identity']['path'].lower()
            assert receipt['executable']['sha256'] == manifest['office_file_identity']['sha256']
            assert receipt['source_sha256'] == receipt['copy_sha256'] == manifest['sha256'][spec['id'] + '.xlsx']
            assert receipt['copy_sha256'] == sha(folder / 'input-copy.xlsx')
            assert receipt['output_sha256'] == sha(folder / 'excel-output.xlsx')
            stages = receipt['stages']
            assert [s['name'] for s in stages] == [s['id'] for s in spec['stages']] + ['reopened']
            inputs = {f'{column}{index}': value for index, row in enumerate(spec['rows'], 4)
                      for column, value in zip('ABC', row)}
            for stage_index, observed in enumerate(stages):
                stage = spec['stages'][min(stage_index, 2)]
                if stage_index < 3 and stage['edit']:
                    inputs[stage['edit']['cell']] = stage['edit']['value']
                assert observed['date1904'] is False
                expected = {**inputs, **stage['expected'], spec['date']['cell']: spec['date']['serial']}
                for address, value in expected.items():
                    actual = observed['cells'][address]['value2']
                    assert actual == value, (folder.name, observed['name'], address, actual, value)
                for address, formula in spec['formulas'].items():
                    assert observed['cells'][address]['formula'] == formula
                error = observed['cells'][spec['error']]
                assert error['text'] == '#DIV/0!' and error['value2'] == -2146826281
                assert observed['cells'][spec['date']['cell']]['text'] == spec['date']['iso']
            # Inspect saved caches independently of COM/open-time recalculation.
            with zipfile.ZipFile(folder / 'excel-output.xlsx') as archive:
                assert archive.testzip() is None
                book = ET.fromstring(archive.read('xl/workbook.xml'))
                props = book.find('s:workbookPr', NS)
                assert props is None or props.get('date1904', 'false').lower() in {'false', '0'}
                sheet = ET.fromstring(archive.read('xl/worksheets/sheet1.xml'))
                cells = {c.get('r'): c for c in sheet.findall('.//s:c', NS)}
                for address, value in {**spec['stages'][-1]['expected'], spec['date']['cell']: spec['date']['serial']}.items():
                    assert Fraction(cells[address].findtext('s:v', namespaces=NS)) == Fraction(str(value))
                for address, formula in spec['formulas'].items():
                    assert '=' + cells[address].findtext('s:f', namespaces=NS) == formula
                assert cells[spec['error']].get('t') == 'e'
                assert cells[spec['error']].findtext('s:v', namespaces=NS) == '#DIV/0!'
            result['runs'].append({'id': folder.name, 'receipt_sha256': sha(receipt_path),
                                   'output_sha256': receipt['output_sha256'], 'process': receipt['process'],
                                   'application': receipt['application'], 'executable': receipt['executable'],
                                   'phase_count': len(stages), 'status': 'PASS'})
            result['host_runs'] += 1
    assert result['host_runs'] == 9
    # Exclusive creation keeps previous conclusions/evidence from being replaced.
    with args.output.open('x', encoding='utf-8', newline='\n') as output:
        json.dump(result, output, indent=2)
        output.write('\n')
    print('PASS: 3 samples x 3 independent Excel runs; 36 phase observations, saved caches, original hashes and cleanup receipts verified')
    print('No unexpected value/formula/error/date-system mismatch in this scope. Zen comparison remains NOT_RUN.')


if __name__ == '__main__':
    main()
