"""Independent rational arithmetic audit of the predeclared business plan.

This does not open XLSX, run Zen, or launch Office. No eval/exec or engine imports.
Only the small expression grammar declared by this plan is accepted.
"""
import argparse
import ast
import json
import re
from fractions import Fraction
from pathlib import Path


def check(case):
    cells = dict(case['numbers'])
    cells.update(case['labels'])
    formulas = dict(case['formulas'])
    formulas.update({'Summary!' + k: v for k, v in case['summary_formulas'].items()})
    expected = dict(case['initial_expected'])
    if set(expected) != set(formulas):
        raise ValueError('initial literal expectations must cover every formula exactly')
    expected_type = case['type_initial']
    checks = 0

    def key(ref):
        return ref.removeprefix(case['sheet'] + '!')

    def value(ref, stack=()):
        ref = key(ref)
        if ref in stack:
            raise ValueError('cycle in independent plan: ' + ref)
        if ref not in formulas:
            if ref not in cells:
                raise ValueError('undeclared reference: ' + ref)
            v = cells[ref]
            return Fraction(str(v)) if type(v) in (int, float) else v
        expression = formulas[ref]
        total = re.fullmatch(r'SUM\(([A-Z]+)(\d+):\1(\d+)\)', expression)
        if total:
            col, start, end = total.groups()
            return sum((v for row in range(int(start), int(end) + 1)
                        if isinstance(v := value(f'{col}{row}', stack + (ref,)), Fraction)), Fraction())
        names = {}

        def symbol(match):
            name = f'cell_{len(names)}'
            names[name] = value(match.group(), stack + (ref,))
            return name

        tree = ast.parse(re.sub(r'(?:[A-Za-z]+!)?[A-Z]+[1-9][0-9]*', symbol, expression), mode='eval')

        def walk(node):
            if isinstance(node, ast.Expression):
                return walk(node.body)
            if isinstance(node, ast.Name) and node.id in names:
                result = names[node.id]
                if not isinstance(result, Fraction):
                    raise ValueError('non-numeric arithmetic input')
                return result
            if isinstance(node, ast.Constant) and type(node.value) is int:
                return Fraction(node.value)
            if isinstance(node, ast.BinOp):
                left, right = walk(node.left), walk(node.right)
                if isinstance(node.op, ast.Add): return left + right
                if isinstance(node.op, ast.Sub): return left - right
                if isinstance(node.op, ast.Mult): return left * right
            raise ValueError('expression outside the predeclared grammar')
        return walk(tree)

    def snapshot(stage):
        nonlocal checks
        for address, literal in expected.items():
            want = None if literal is None else Fraction(str(literal))
            got = value(address)
            if got != want:
                raise ValueError(f"{case['id']}/{stage}/{address}: {got} != literal {want}")
            checks += 1
        actual_type = cells.get(case['type_cell'])
        if type(actual_type) is not type(expected_type) or actual_type != expected_type:
            raise ValueError('typed probe differs from declared transition')

    snapshot('initial')
    for op in case['operations']:
        address, kind = op['cell'], op['kind']
        if kind == 'formula':
            formulas[address] = op['value']
        elif kind == 'clear_formula':
            if address not in formulas: raise ValueError('clear target is not a formula')
            del formulas[address]
            cells[address] = None
        elif kind in ('number', 'date_serial', 'text', 'boolean', 'blank'):
            if address in formulas: raise ValueError('literal transition over formula is not declared')
            cells[address] = None if kind == 'blank' else op['value']
            if kind == 'date_serial' and address != case['date_cell']:
                raise ValueError('date target mismatch')
        else:
            raise ValueError('unknown operation')
        if address == case['type_cell']:
            expected_type = None if kind == 'blank' else op['value']
        expected.update(op['expected'])
        snapshot(op['id'])
    return {'case': case['id'], 'states': 1 + len(case['operations']), 'numeric_or_blank_assertions': checks}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('plan', type=Path)
    args = parser.parse_args()
    plan = json.loads(args.plan.read_text(encoding='utf-8'))
    results = [check(case) for case in plan['cases']]
    print(json.dumps({'kind': 'plan arithmetic only; XLSX/Zen/Office NOT_RUN', 'results': results}))


if __name__ == '__main__':
    main()
