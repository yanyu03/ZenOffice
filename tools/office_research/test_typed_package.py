"""Predeclared isolated package specimens: each refusal ends that specimen only."""
import argparse
import copy
from io import BytesIO
import json
import re
from pathlib import Path
import unittest
import warnings
import xml.etree.ElementTree as ET
from zipfile import ZipFile
from check_typed_package import check_package
from typed_metadata import metadata_parts

M='{http://schemas.openxmlformats.org/spreadsheetml/2006/main}'
ROOT=Path(__file__).resolve().parents[2]
PLAN=ROOT/'docs/engine_specs/ENG-005-business-v2.json'


def specimen(spec):
    # Test-only in-memory layouts with literal caches, not generated business inputs.
    parts=metadata_parts(spec['date_system'],spec['sheet'],spec['id'])
    for i,name in enumerate(('Summary',spec['sheet']),1):
        root=ET.Element(M+'worksheet');data=ET.SubElement(root,M+'sheetData')
        rows={}
        def make_cell(address,attrs):
            number=re.search(r'[0-9]+$',address).group()
            if number not in rows: rows[number]=ET.SubElement(data,M+'row',{'r':number})
            return ET.SubElement(rows[number],M+'c',attrs)
        if name=='Summary': numbers={};labels={};formulas=spec['summary_formulas']
        else: numbers=spec['numbers'];labels=spec['labels'];formulas=spec['formulas']
        for address,number in numbers.items():
            attrs={'r':address}
            if address==spec['date_cell']: attrs['s']='1'
            ET.SubElement(make_cell(address,attrs),M+'v').text=str(number)
        for address,label in labels.items():
            cell=make_cell(address,{'r':address,'t':'inlineStr'})
            ET.SubElement(ET.SubElement(cell,M+'is'),M+'t').text=label
        for address,formula in formulas.items():
            cell=make_cell(address,{'r':address})
            ET.SubElement(cell,M+'f').text=formula
            key=address if name==spec['sheet'] else 'Summary!'+address
            ET.SubElement(cell,M+'v').text=str(spec['initial_expected'][key])
        data[:]=sorted(data,key=lambda r:int(r.get('r')))
        parts['xl/worksheets/sheet'+str(i)+'.xml']=ET.tostring(root)
    return parts


def package(parts,duplicate=None):
    stream=BytesIO()
    with warnings.catch_warnings():
        warnings.simplefilter('ignore',UserWarning)
        with ZipFile(stream,'w') as z:
            for name,data in parts.items(): z.writestr(name,data)
            if duplicate: z.writestr(duplicate,parts[duplicate])
    return stream.getvalue()


class PackagePolicyTests(unittest.TestCase):
    def setUp(self):
        self.plan_before=PLAN.read_bytes()
        self.cases=json.loads(self.plan_before)['cases']
        self.spec=self.cases[0];self.parts=specimen(self.spec)

    def tearDown(self): self.assertEqual(PLAN.read_bytes(),self.plan_before)

    def alter(self,part,fn):
        parts=dict(self.parts);root=ET.fromstring(parts[part]);fn(root)
        parts[part]=ET.tostring(root);return parts

    def refuse(self,parts):
        with self.assertRaises(ValueError): check_package(package(parts),self.spec)

    def test_three_owned_positive_specimens_and_input_bytes_unchanged(self):
        for spec in self.cases:
            with self.subTest(case=spec['id']):
                data=package(specimen(spec));before=bytes(data)
                got=check_package(data,spec)
                self.assertEqual(got['policy'],'PASS')
                self.assertEqual(got['initial_formulas_checked'],len(spec['initial_expected']))
                self.assertEqual(data,before)

    def test_metadata_negative_specimens(self):
        changes=[('date-system',lambda r:r.find(M+'workbookPr').set('date1904','1')),
                 ('missing-system',lambda r:r.remove(r.find(M+'workbookPr'))),
                 ('auto-calc',lambda r:r.find(M+'calcPr').set('calcMode','auto')),
                 ('recalc',lambda r:r.find(M+'calcPr').set('fullCalcOnLoad','1')),
                 ('duplicate-calc',lambda r:r.append(copy.deepcopy(r.find(M+'calcPr'))))]
        for name,fn in changes:
            with self.subTest(name=name): self.refuse(self.alter('xl/workbook.xml',fn))
        for name,fn in [('custom-format',lambda r:r.find(M+'cellXfs')[1].set('numFmtId','200')),
                        ('extra-xf-attribute',lambda r:r.find(M+'cellXfs')[1].set('applyAlignment','1')),
                        ('xf-child',lambda r:ET.SubElement(r.find(M+'cellXfs')[1],M+'alignment'))]:
            with self.subTest(name=name): self.refuse(self.alter('xl/styles.xml',fn))

    def test_cell_value_type_style_formula_and_cache_negatives(self):
        def cell(root,address): return next(c for c in root.iter(M+'c') if c.get('r')==address)
        changes=[('numeric',lambda r:cell(r,'B4').find(M+'v').__setattr__('text','0')),
                 ('type',lambda r:cell(r,'B4').set('t','b')),
                 ('label',lambda r:cell(r,'A1').find('.//'+M+'t').__setattr__('text','wrong')),
                 ('date-style',lambda r:cell(r,'B10').attrib.pop('s')),
                 ('formula',lambda r:cell(r,'D4').find(M+'f').__setattr__('text','B4+C4')),
                 ('cache',lambda r:cell(r,'D4').find(M+'v').__setattr__('text','999')),
                 ('array',lambda r:cell(r,'D4').find(M+'f').set('t','array')),
                 ('duplicate-cell',lambda r:r.find(M+'sheetData')[0].append(copy.deepcopy(cell(r,'B4')))),
                 ('table',lambda r:ET.SubElement(r,M+'tableParts'))]
        for name,fn in changes:
            with self.subTest(name=name): self.refuse(self.alter('xl/worksheets/sheet2.xml',fn))

    def test_probe_content_types_and_relationship_negatives(self):
        parts=dict(self.parts);del parts['customXml/zen-probe.xml'];self.refuse(parts)
        self.refuse(self.alter('customXml/zen-probe.xml',lambda r:r.set('case','wrong')))
        self.refuse(self.alter('xl/_rels/workbook.xml.rels',lambda r:r[0].set('TargetMode','External')))
        self.refuse(self.alter('xl/_rels/workbook.xml.rels',lambda r:r[0].set('Target','../../../outside.xml')))
        self.refuse(self.alter('xl/_rels/workbook.xml.rels',lambda r:r[1].set('Target',r[0].get('Target'))))
        self.refuse(self.alter('[Content_Types].xml',lambda r:r.append(copy.deepcopy(r[-1]))))
        self.refuse(self.alter('[Content_Types].xml',lambda r:r[-1].set('ContentType','application/xml')))

    def test_zip_duplicate_and_unsafe_names(self):
        with self.assertRaises(ValueError): check_package(package(self.parts,'xl/workbook.xml'),self.spec)
        for name in ('../outside.xml','/absolute.xml','xl/../alias.xml'):
            with self.subTest(name=name):
                parts=dict(self.parts);parts[name]=b'owned';self.refuse(parts)

    def test_xml_entity_declaration_is_refused(self):
        parts=dict(self.parts)
        parts['xl/workbook.xml']=b'<!DOCTYPE workbook [<!ENTITY x "owned">]><workbook/>'
        self.refuse(parts)

    def test_other_case_is_not_admitted_by_matching_metadata_alone(self):
        with self.assertRaises(ValueError): check_package(package(self.parts),self.cases[1])


if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--log',type=Path);args=parser.parse_args()
    suite=unittest.defaultTestLoader.loadTestsFromTestCase(PackagePolicyTests)
    if args.log:
        with args.log.open('x',encoding='utf-8') as stream:
            result=unittest.TextTestRunner(stream=stream,verbosity=2).run(suite)
    else: result=unittest.TextTestRunner(verbosity=2).run(suite)
    raise SystemExit(0 if result.wasSuccessful() else 1)
