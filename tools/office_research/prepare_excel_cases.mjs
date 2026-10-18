// Public synthetic fixtures; no Office process is started by this builder.
import fs from 'node:fs/promises';
import path from 'node:path';
import { createRequire } from 'node:module';
const [runtimeWorkdir, outputDir, evidenceDir] = process.argv.slice(2);
if (!runtimeWorkdir || !outputDir || !evidenceDir) throw new Error('usage: node prepare_excel_cases.mjs <runtime-junction-dir> <new-output-dir> <evidence-dir>');
const require = createRequire(path.resolve(runtimeWorkdir, 'artifact-loader.cjs'));
const { Workbook, SpreadsheetFile } = require('@oai/artifact-tool');
const cases = [
  { id: 'budget', sheet: 'Budget', title: 'Monthly budget (CNY)',
    headers: ['Category', 'Planned (CNY)', 'Actual (CNY)', 'Variance (CNY)'],
    rows: [['Payroll',10000,9500],['Rent',5000,5000],['Supplies',1000,800]],
    formulas: {D4:'=B4-C4',D5:'=B5-C5',D6:'=B6-C6',B8:'=SUM(B4:B6)',C8:'=SUM(C4:C6)',D8:'=B8-C8',B12:'=1/0'},
    date: {cell:'B10',serial:46314,iso:'2026-10-19'}, error:'B12',
    stages: [
      {id:'initial',edit:null,expected:{D4:500,D5:0,D6:200,B8:16000,C8:15300,D8:700}},
      {id:'edit-payroll',edit:{cell:'C4',value:9800},expected:{D4:200,D5:0,D6:200,B8:16000,C8:15600,D8:400}},
      {id:'edit-supplies',edit:{cell:'B6',value:1200},expected:{D4:200,D5:0,D6:400,B8:16200,C8:15600,D8:600}},
    ]},
  { id:'orders',sheet:'Orders',title:'Order totals (CNY)',
    headers:['Order ID','Quantity','Unit price (CNY)','Line total (CNY)'],
    rows:[['O-101',3,125],['O-102',2,80]],
    formulas:{D4:'=B4*C4',D5:'=B5*C5',D7:'=SUM(D4:D5)',B11:'=1/0'},
    date:{cell:'B9',serial:46314,iso:'2026-10-19'},error:'B11',
    stages:[
      {id:'initial',edit:null,expected:{D4:375,D5:160,D7:535}},
      {id:'edit-quantity',edit:{cell:'B4',value:4},expected:{D4:500,D5:160,D7:660}},
      {id:'edit-price',edit:{cell:'C5',value:90},expected:{D4:500,D5:180,D7:680}},
    ]},
  {id:'attendance',sheet:'Attendance',title:'Monthly attendance (hours)',
    headers:['Employee ID','Days','Hours per day','Total hours'],
    rows:[['EMP-01',20,8],['EMP-02',18,8]],
    formulas:{D4:'=B4*C4',D5:'=B5*C5',D7:'=SUM(D4:D5)',B11:'=1/0'},
    date:{cell:'B9',serial:46296,iso:'2026-10-01'},error:'B11',
    stages:[
      {id:'initial',edit:null,expected:{D4:160,D5:144,D7:304}},
      {id:'edit-days',edit:{cell:'B5',value:19},expected:{D4:160,D5:152,D7:312}},
      {id:'edit-hours',edit:{cell:'C4',value:7.5},expected:{D4:150,D5:152,D7:302}},
    ]},
];
// Independent arithmetic expectations are literal constants declared above,
// not read back from the library under test and then re-labelled as an oracle.
await fs.mkdir(path.dirname(outputDir), {recursive:true});
await fs.mkdir(outputDir, {recursive:false}); // Never replace a frozen fixture set.
await fs.mkdir(evidenceDir,{recursive:true});
const plan = {
  id:'RES-002-inputs-v1', businessSamples:3, hostRunsPerSample:3,
  authoring:'bundled @oai/artifact-tool; synthetic inputs, not Excel outputs',
  dateSystem:'Excel1900; numeric serial plus yyyy-mm-dd style; verify date1904 flag in package and host',
  comparison:{numeric:'exact; chosen inputs/results are integers or binary-exact halves', formulas:'exact invariant formula text, retain any host normalization separately', errors:'exact #DIV/0! in designated probe only', identifiers:'exact', tolerance:0},
  undo:'Do not call Application.Undo for COM edits. Reproduce content states on isolated copies with explicit inverse numeric assignments; those assignments are NOT a test of native Undo. Zen undo/redo acceptance is separate.',
  hostProtocol:['Verify input SHA256 and current Office binary/application identity before each run.', 'Create a fresh owned Excel instance and an isolated file copy; never attach to user workbooks.', 'Observe initial stage then both numeric edits, CalculateFullRebuild after each, capture Value2/Formula/Text/errors/Date1904.', 'SaveAs to a new output, close and reopen; compare reopened values and formulas and OOXML caches independently.', 'Repeat from original input three times per sample; count three files and nine runs separately.', 'Preserve mismatches before any change proposal. Choose REA targets only for a specific observed discrepancy.'],
  cases,
};
await fs.writeFile(path.join(outputDir,'plan.json'),JSON.stringify(plan,null,2)+'\n');
for (const spec of cases) {
  const wb=Workbook.create(); const sh=wb.worksheets.add(spec.sheet);
  sh.getRange('A1').values=[[spec.title]];
  sh.getRange('A2').values=[['Source: synthetic ZenOffice acceptance data']];
  sh.getRange('A3:D3').values=[spec.headers];
  sh.getRange(`A4:C${3+spec.rows.length}`).values=spec.rows;
  sh.getRange(spec.id==='budget'?'A8':'A7').values=[['Total']];
  sh.getRange(spec.date.cell).values=[[spec.date.serial]];
  sh.getRange(spec.id==='budget'?'A10':'A9').values=[[spec.id==='attendance'?'Month beginning':'Reporting date']];
  sh.getRange(spec.id==='budget'?'A12':'A11').values=[['Intentional error probe']];
  for (const [address,formula] of Object.entries(spec.formulas)) sh.getRange(address).formulas=[[formula]];
  sh.getRange('A1:D12').format.font={name:'Arial',size:11};
  sh.getRange('A1').format.font={name:'Arial',size:14,bold:true};
  sh.getRange('A3:D3').format={fill:'#E5E7EB',font:{bold:true,color:'#111827'},rowHeight:24};
  sh.getRange('A1:A12').format.columnWidth=50;
  sh.getRange('B1:D12').format.columnWidth=22;
  sh.getRange('B4:D8').setNumberFormat('0.##');
  sh.getRange(spec.date.cell).setNumberFormat('yyyy-mm-dd');
  sh.freezePanes.freezeRows(3);
  wb.recalculate();
  for (const [address,expected] of Object.entries(spec.stages[0].expected)) {
    const actual=sh.getRange(address).values[0][0];
    if (actual!==expected) throw new Error(`${spec.id} ${address}: expected ${expected}, got ${actual}`);
  }
  const check=await wb.inspect({kind:'region',sheetId:spec.sheet,range:'A1:D12',maxChars:3500});
  await fs.writeFile(path.join(evidenceDir,`${spec.id}.inspect.ndjson`),check.ndjson);
  const png=await wb.render({sheetName:spec.sheet,range:'A1:D12',scale:1.5,format:'png'});
  await fs.writeFile(path.join(evidenceDir,`${spec.id}.png`),new Uint8Array(await png.arrayBuffer()));
  const file=await SpreadsheetFile.exportXlsx(wb);await file.save(path.join(evidenceDir,`${spec.id}.xlsx`));
  await fs.copyFile(path.join(evidenceDir,`${spec.id}.xlsx`),path.join(outputDir,`${spec.id}.xlsx`),1);
}
console.log('Created three synthetic inputs; Excel host NOT_RUN; verify packages and freeze hashes next.');
