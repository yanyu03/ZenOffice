"""Independent literal calendar vectors and read-only output protection."""
import argparse
import copy
import json
from pathlib import Path
import tempfile
import unittest
from author_typed_package import build_candidate
from preview_typed_date import interpret_serial, preview, write_preview

ROOT=Path(__file__).resolve().parents[2]
PLAN=ROOT/'docs/engine_specs/ENG-005-business-v2.json'


class DatePreviewTests(unittest.TestCase):
    def test_1900_boundary_literals(self):
        for serial,want in [('1','1900-01-01 00:00:00'),('59','1900-02-28 00:00:00'),
                            ('60.5','1900-02-29 (Excel fictional date) 12:00:00'),('61.5','1900-03-01 12:00:00')]:
            with self.subTest(serial=serial):self.assertEqual(interpret_serial(serial,'Excel1900')['interpretation'],want)
        self.assertEqual(interpret_serial('0','Excel1900')['kind'],'boundary')
        self.assertIn('calendar unspecified',interpret_serial('0','Excel1900')['interpretation'])

    def test_1904_leap_fraction_and_epoch_literals(self):
        for serial,want in [('0','1904-01-01 00:00:00'),('59.25','1904-02-29 06:00:00'),
                            ('60','1904-03-01 00:00:00'),('1.000001','1904-01-02 00:00:00.0864')]:
            with self.subTest(serial=serial):self.assertEqual(interpret_serial(serial,'Excel1904')['interpretation'],want)

    def test_upper_calendar_bounds(self):
        for system,serial in [('Excel1900','2958465'),('Excel1904','2957003')]:
            self.assertEqual(interpret_serial(serial,system)['interpretation'],'9999-12-31 00:00:00')

    def test_bad_system_serial_or_precision_refuses(self):
        for serial in ('NaN','Infinity','-0.25','bad','2958466','0.00000000000000001',True):
            with self.subTest(serial=serial):
                with self.assertRaises(ValueError):interpret_serial(serial,'Excel1900')
        with self.assertRaises(ValueError):interpret_serial('2957004','Excel1904')
        with self.assertRaises(ValueError):interpret_serial('1','1904')

    def test_three_plan_initial_date_literals(self):
        wants={'budget':'2026-10-19 00:00:00','orders':'1904-02-29 06:00:00','attendance':'1900-02-28 00:00:00'}
        for spec in json.loads(PLAN.read_text(encoding='utf-8'))['cases']:
            payload=build_candidate(spec);result,svg=preview(payload,spec)
            self.assertEqual(result['interpretation'],wants[spec['id']])
            self.assertIn(result['interpretation'],svg)
            self.assertEqual(result['system'],spec['date_system'])

    def test_metadata_does_not_follow_label_or_unverified_spec(self):
        spec=next(s for s in json.loads(PLAN.read_text(encoding='utf-8'))['cases'] if s['id']=='orders')
        other=copy.deepcopy(spec);other['date_system']='Excel1900'
        payload=build_candidate(other)
        with self.assertRaises(ValueError):preview(payload,spec)
        result,_=preview(payload,other)
        self.assertEqual(result['interpretation'],'1900-02-28 06:00:00')

    def test_create_new_and_readonly_protection(self):
        (ROOT/'target').mkdir(exist_ok=True)
        spec=json.loads(PLAN.read_text(encoding='utf-8'))['cases'][1];data=build_candidate(spec)
        with tempfile.TemporaryDirectory(dir=ROOT/'target') as directory:
            root=Path(directory);source=root/'source.xlsx';output=root/'preview.svg';source.write_bytes(data)
            result=write_preview(PLAN,'orders',source,output)
            self.assertTrue(result['input_unchanged']);self.assertEqual(source.read_bytes(),data)
            before=output.read_bytes()
            with self.assertRaises(FileExistsError):write_preview(PLAN,'orders',source,output)
            self.assertEqual(output.read_bytes(),before);self.assertEqual(source.read_bytes(),data)
            with self.assertRaises(ValueError):write_preview(PLAN,'missing',source,root/'bad.svg')
            self.assertFalse((root/'bad.svg').exists())


if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--log',type=Path);args=parser.parse_args()
    suite=unittest.defaultTestLoader.loadTestsFromTestCase(DatePreviewTests)
    if args.log:
        with args.log.open('x',encoding='utf-8') as stream:result=unittest.TextTestRunner(stream=stream,verbosity=2).run(suite)
    else:result=unittest.TextTestRunner(verbosity=2).run(suite)
    raise SystemExit(0 if result.wasSuccessful() else 1)
