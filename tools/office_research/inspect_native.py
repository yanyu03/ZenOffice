"""Read declared Office artifacts; save local evidence without publishing vendor bytes.

Requires pefile==2024.8.26 and the user's installed dsha skill MCP client.
Reads inventory from inspect_typelibs.ps1; never starts an Office application.
"""
import argparse
import hashlib
import json
from pathlib import Path
import sys
import time

import pefile


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--inventory', type=Path, required=True)
    ap.add_argument('--output', type=Path, required=True)
    ap.add_argument('--dsha-scripts', type=Path, required=True)
    args = ap.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    sys.path.insert(0, str(args.dsha_scripts.resolve()))
    from mcp_client import Mcp
    inventory = json.loads(args.inventory.read_text(encoding='utf-8-sig'))
    identities = {i['name']: i for i in inventory}
    client = Mcp(timeout=5400, log_path=out / 'mcp.stderr.log')
    operations = []
    closed = False
    started = time.monotonic()

    def invoke(label, tool, arguments=None):
        before = time.monotonic()
        response = client.tool(tool, arguments)
        (out / (label + '.json')).write_text(json.dumps(response, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
        if response.get('isError'):
            raise RuntimeError(f'{label}: REA rejected request; retained raw response')
        data = response.get('structuredContent')
        if data is None:
            data = json.loads(next(x['text'] for x in response['content'] if x['type'] == 'text'))
        operations.append({'label': label, 'tool': tool, 'evidence_id': data.get('evidence_id'), 'seconds': round(time.monotonic()-before, 3)})
        print(json.dumps(operations[-1]), flush=True)
        return data

    status = 'FAILED'
    try:
        catalog = client.request('tools/list')
        (out / 'tool-catalog.json').write_text(json.dumps(catalog, ensure_ascii=False), encoding='utf-8')
        pe_summary = []
        for item in inventory:
            path = Path(item['path'])
            assert hashlib.sha256(path.read_bytes()).hexdigest() == item['sha256'], 'Target identity changed'
            pe = pefile.PE(str(path))
            exports = [{'name': e.name.decode(errors='replace') if e.name else None,
                        'ordinal': e.ordinal, 'address': hex(pe.OPTIONAL_HEADER.ImageBase + e.address),
                        'forwarder': e.forwarder.decode(errors='replace') if e.forwarder else None}
                       for e in getattr(pe, 'DIRECTORY_ENTRY_EXPORT', type('', (), {'symbols': []})()).symbols]
            imports = [{'dll': d.dll.decode(errors='replace'),
                        'members': [e.name.decode(errors='replace') if e.name else '#' + str(e.ordinal) for e in d.imports]}
                       for d in getattr(pe, 'DIRECTORY_ENTRY_IMPORT', [])]
            delay_imports = [{'dll': d.dll.decode(errors='replace'),
                             'members': [e.name.decode(errors='replace') if e.name else '#' + str(e.ordinal) for e in d.imports]}
                            for d in getattr(pe, 'DIRECTORY_ENTRY_DELAY_IMPORT', [])]
            pe_summary.append({'name': item['name'], 'sha256': item['sha256'], 'machine': hex(pe.FILE_HEADER.Machine),
                               'image_base': hex(pe.OPTIONAL_HEADER.ImageBase),
                               'entry': hex(pe.OPTIONAL_HEADER.ImageBase + pe.OPTIONAL_HEADER.AddressOfEntryPoint),
                               'exports': exports, 'imports': imports, 'delay_imports': delay_imports,
                               'sections': [{'name': s.Name.rstrip(b'\0').decode(errors='replace'), 'bytes': s.SizeOfRawData} for s in pe.sections]})
            pe.close()
            invoke(item['name'] + '-resources', 'inspect_pe_resources', {'path': str(path), 'max_file_bytes': 134217728, 'max_entries': 65536})
        (out / 'pe-summary.json').write_text(json.dumps(pe_summary, indent=2), encoding='utf-8')
        for name in ['XLCALL32.DLL', 'WINWORD.EXE', 'POWERPNT.EXE']:
            item = identities[name]
            invoke(name + '-open', 'open_binary', {'path': item['path'], 'provider_id': 'ghidra'})
            invoke(name + '-session', 'binary_session')
            invoke(name + '-overview', 'binary_overview')
            summary = next(x for x in pe_summary if x['name'] == name)
            functions = [e for e in summary['exports'] if e['name'] in ['Excel4v', 'Excel12v'] and not e['forwarder']]
            if not functions:
                functions = [{'name': 'entry', 'address': summary['entry']}]
            for function in functions:
                label = name + '-' + function['name']
                invoke(label + '-pseudo', 'procedure_pseudo_code', {'procedure': function['address']})
                invoke(label + '-callees', 'procedure_callees', {'procedure': function['address']})
            invoke(name + '-close', 'close_binary')
        for index, question in enumerate([
            'What are the actual Excel recalculation, dependency and numeric edge semantics on independent task fixtures?',
            'How do Word range edits, save/reopen and layout behave on independent multi-run/table/image fixtures?',
            'How do PowerPoint slide identity, relationships, shape edits and save/reopen behave on independent decks?',
            'Which core implementations in EXCEL.EXE, WWLIB.DLL and PPCORE.DLL explain observed behavior beyond declarations and launcher boundaries?'
        ]):
            invoke('unknown-' + str(index), 'record_unknown', {
                'question': question, 'severity': 'high', 'domain': 'office-engine-compatibility',
                'required_authority': 'controlled-replay' if index < 3 else 'shipped-artifact',
                'required_confidence': 'observed', 'required_environment': None,
                'recommended_probes': [{'operation': 'independent-fixture-comparison' if index < 3 else 'focused-core-provider-session',
                                         'rationale': 'Static declarations and launcher functions do not establish document editing behavior.'}],
                'relationships': []})
        invoke('export', 'export_evidence_bundle', {'path': str(out / 'evidence-bundle.json')})
        invoke('final-close', 'close_binary')
        closed = True
        status = 'STATIC_OPERATIONS_COMPLETED_REVIEW_REQUIRED'
    finally:
        if not closed:
            try:
                invoke('cleanup-close', 'close_binary')
                closed = True
            except Exception as error:
                print('Cleanup unconfirmed: ' + str(error), file=sys.stderr)
        client.close()
        after = [{'name': i['name'], 'sha256': hashlib.sha256(Path(i['path']).read_bytes()).hexdigest(),
                  'unchanged': hashlib.sha256(Path(i['path']).read_bytes()).hexdigest() == i['sha256']} for i in inventory]
        result = {'status': status, 'application_started': False, 'session_close_completed': closed,
                  'identity_after': after, 'operations': operations, 'seconds': round(time.monotonic()-started, 3)}
        (out / 'result.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
        print(json.dumps({'status': status, 'seconds': result['seconds']}), flush=True)


if __name__ == '__main__':
    main()
