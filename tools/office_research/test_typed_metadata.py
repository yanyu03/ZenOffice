"""Independent XML/OPC assertions for fresh synthetic metadata, not Office tests."""
import argparse
from io import BytesIO
import json
from pathlib import Path
import posixpath
import unittest
import xml.etree.ElementTree as ET
from zipfile import ZipFile, ZIP_DEFLATED
from typed_metadata import metadata_parts

M = '{http://schemas.openxmlformats.org/spreadsheetml/2006/main}'
R = '{http://schemas.openxmlformats.org/package/2006/relationships}'
C = '{http://schemas.openxmlformats.org/package/2006/content-types}'
ROOT = Path(__file__).resolve().parents[2]


class MetadataTests(unittest.TestCase):
    def setUp(self):
        self.parts = metadata_parts('Excel1900','Budget','budget')

    def test_real_plan_cases_have_explicit_systems(self):
        plan = json.loads((ROOT/'docs/engine_specs/ENG-005-business-v2.json').read_text(encoding='utf-8'))
        for case in plan['cases']:
            with self.subTest(case=case['id']):
                parts = metadata_parts(case['date_system'],case['sheet'],case['id'])
                wb = ET.fromstring(parts['xl/workbook.xml'])
                self.assertEqual(wb.find(M+'workbookPr').attrib,
                    {'date1904':'1' if case['id']=='orders' else '0'})
                self.assertEqual([s.get('name') for s in wb.find(M+'sheets')],['Summary',case['sheet']])

    def test_manual_calculation_and_schema_order(self):
        wb = ET.fromstring(self.parts['xl/workbook.xml'])
        self.assertEqual([c.tag for c in wb],[M+'workbookPr',M+'sheets',M+'calcPr'])
        self.assertEqual(wb.find(M+'calcPr').attrib,
                         {'calcMode':'manual','fullCalcOnLoad':'0','forceFullCalc':'0'})

    def test_date_style_is_builtin14_not_custom_or_children(self):
        styles = ET.fromstring(self.parts['xl/styles.xml'])
        self.assertIsNone(styles.find(M+'numFmts'))
        xfs = styles.find(M+'cellXfs')
        self.assertEqual(xfs.get('count'),'2')
        self.assertEqual(len(xfs),2)
        self.assertEqual(xfs[1].attrib,{'numFmtId':'14','fontId':'0','fillId':'0',
                                      'borderId':'0','xfId':'0','applyNumberFormat':'1'})
        self.assertEqual(len(xfs[1]),0)
        for group in ('fonts','fills','borders','cellStyleXfs','cellStyles'):
            element = styles.find(M+group)
            self.assertEqual(int(element.get('count')),len(element))

    def test_owning_probe_relationship_and_no_external_targets(self):
        rels = ET.fromstring(self.parts['xl/_rels/workbook.xml.rels'])
        self.assertEqual(len({r.get('Id') for r in rels}),4)
        for rel in rels:
            self.assertIsNone(rel.get('TargetMode'))
            resolved = posixpath.normpath(posixpath.join('xl',rel.get('Target')))
            self.assertIn(resolved,set(self.parts)|{'xl/worksheets/sheet1.xml','xl/worksheets/sheet2.xml'})
        probe = next(r for r in rels if r.get('Type').endswith('/customXml'))
        self.assertEqual(probe.get('Target'),'../customXml/zen-probe.xml')
        data = ET.fromstring(self.parts['customXml/zen-probe.xml'])
        self.assertEqual(data.get('case'),'budget')
        self.assertIn('Owned synthetic',data.text)
        root = ET.fromstring(self.parts['_rels/.rels'])
        self.assertEqual(root[0].get('Target'),'xl/workbook.xml')

    def test_content_types_cover_workbook_styles_sheets_and_probe(self):
        ct = ET.fromstring(self.parts['[Content_Types].xml'])
        overrides = {e.get('PartName'):e.get('ContentType') for e in ct.findall(C+'Override')}
        self.assertEqual(len(overrides),4)
        self.assertEqual(overrides['/xl/workbook.xml'],
            'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml')
        self.assertTrue(overrides['/xl/styles.xml'].endswith('.styles+xml'))
        self.assertEqual({e.get('Extension'):e.get('ContentType') for e in ct.findall(C+'Default')}['xml'],'application/xml')

    def test_xml_escaping_and_unicode_roundtrip(self):
        parts = metadata_parts('Excel1904','研发 & <预算>','owned_01')
        wb = ET.fromstring(parts['xl/workbook.xml'])
        self.assertEqual(wb.find(M+'sheets')[1].get('name'),'研发 & <预算>')

    def test_bad_parameters_refuse_without_returning_parts(self):
        for system in ('1904','',None,True,[]):
            with self.subTest(system=system):
                with self.assertRaises(ValueError): metadata_parts(system,'Budget','budget')
        for sheet in ('','Summary','summary','x'*32,'bad/name','bad:name','bad[0]',"'bad",'bad\nname',42):
            with self.subTest(sheet=sheet):
                with self.assertRaises(ValueError): metadata_parts('Excel1900',sheet,'budget')
        for identity in ('','../private','a b','a'*65,None):
            with self.subTest(identity=identity):
                with self.assertRaises(ValueError): metadata_parts('Excel1900','Budget',identity)

    def test_in_memory_zip_roundtrip_keeps_exact_parts(self):
        # Owned micro-package only; never a business input or acceptance manifest.
        payload = dict(self.parts)
        for i in (1,2):
            payload['xl/worksheets/sheet'+str(i)+'.xml'] = (
                '<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">'
                '<sheetData><row r="1"><c r="A1" s="1"><v>61.5</v></c></row></sheetData></worksheet>').encode()
        stream = BytesIO()
        with ZipFile(stream,'w',ZIP_DEFLATED) as archive:
            for name,data in payload.items(): archive.writestr(name,data)
        with ZipFile(BytesIO(stream.getvalue())) as archive:
            self.assertIsNone(archive.testzip())
            self.assertEqual(set(archive.namelist()),set(payload))
            for name,data in payload.items():
                self.assertEqual(archive.read(name),data)
                ET.fromstring(archive.read(name))
            sheet=ET.fromstring(archive.read('xl/worksheets/sheet2.xml'))
            self.assertEqual(sheet.find('.//'+M+'c').get('s'),'1')

    def test_deterministic_no_shared_mutable_result(self):
        other = metadata_parts('Excel1900','Budget','budget')
        self.assertEqual(self.parts,other)
        self.parts['xl/styles.xml']=b'changed owned result'
        self.assertNotEqual(self.parts,metadata_parts('Excel1900','Budget','budget'))


if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--log',type=Path)
    args=parser.parse_args()
    suite=unittest.defaultTestLoader.loadTestsFromTestCase(MetadataTests)
    if args.log:
        with args.log.open('x',encoding='utf-8') as stream:
            result=unittest.TextTestRunner(stream=stream,verbosity=2).run(suite)
    else:
        result=unittest.TextTestRunner(verbosity=2).run(suite)
    raise SystemExit(0 if result.wasSuccessful() else 1)
