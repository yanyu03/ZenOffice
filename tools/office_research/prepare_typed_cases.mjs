// Generate candidates from preregistered literals. This does NOT seal or certify inputs.
import fs from 'node:fs/promises';
import path from 'node:path';
import {createRequire} from 'node:module';
import {createHash} from 'node:crypto';
import {assessAuthoring} from './typed_candidate_preflight.mjs';
const args = process.argv.slice(2);
const assessOnly = args[0] === '--assess';
const [runtimeDir, originalPlanPath, outputDir, evidenceDir] = args;
const planPath = assessOnly ? args[1] : originalPlanPath;
if (!runtimeDir || !planPath) throw Error('usage: node prepare_typed_cases.mjs <runtime-dir> <plan|--help-settings> <new-output-dir> <new-evidence-dir>');
let plan;
if (planPath !== '--help-settings') {
  let result;
  try {
    const stat = await fs.stat(planPath);
    if (!stat.isFile() || stat.size > 256*1024) throw Error('invalid plan');
    const bytes = await fs.readFile(planPath);
    plan = JSON.parse(bytes.toString('utf8'));
    result = {...assessAuthoring(plan),plan_sha256:createHash('sha256').update(bytes).digest('hex')};
  } catch {
    result = {status:'INVALID_PLAN',stop:true,task:{status:'NOT_RUN'},automatic_actions:[]};
  }
  if (assessOnly || result.status !== 'READY') {
    console.log(JSON.stringify(result));
    process.exit(assessOnly ? 0 : 126);
  }
}
// No dependency loading, output directory, workbook, render or export before preflight.
const require = createRequire(path.resolve(runtimeDir, 'artifact-loader.cjs'));
const {Workbook, SpreadsheetFile} = require('@oai/artifact-tool');
if (planPath === '--help-settings') {
  const wb = Workbook.create();
  const help = wb.help('workbook date system and calculation settings', {
    search:'date1904|dateSystem|calcMode|calculationMode',include:'index,examples,notes',maxChars:4000});
  console.log(help.ndjson);
  process.exit(0);
}
if (!outputDir || !evidenceDir) throw Error('new candidate and evidence directories required');
await fs.mkdir(outputDir,{recursive:false});
await fs.mkdir(evidenceDir,{recursive:false});
await fs.copyFile(planPath,path.join(outputDir,'plan.json'),1);
const receipt = {plan:plan.id, author:'bundled @oai/artifact-tool',
  scope:'candidate creation only; OOXML/system/style/calc/custom-part policy must pass before sealing',cases:[]};
for (const spec of plan.cases) {
  const wb=Workbook.create();
  const summary=wb.worksheets.add('Summary');
  const sheet=wb.worksheets.add(spec.sheet);
  for (const [address,value] of Object.entries(spec.numbers)) sheet.getRange(address).values=[[value]];
  for (const [address,value] of Object.entries(spec.labels)) sheet.getRange(address).values=[[value]];
  for (const [address,formula] of Object.entries(spec.formulas)) sheet.getRange(address).formulas=[['='+formula]];
  summary.getRange('A1').values=[[spec.id+' — independent typed/date fixture']];
  summary.getRange('A2').values=[['Business result']];
  summary.getRange('A3').values=[['Date serial plus one']];
  summary.getRange('A5').values=[['Source: synthetic ZenOffice business-v2 plan']];
  summary.getRange('A6').values=[['Planned date system: '+spec.date_system]];
  for (const [address,formula] of Object.entries(spec.summary_formulas)) summary.getRange(address).formulas=[['='+formula]];
  // Public formatting surface only: inspect actual exporter XML, never patch it to pass.
  sheet.getRange('A1:F12').format.font={name:'Arial',size:11};
  sheet.getRange('A1').format.font={name:'Arial',size:14,bold:true};
  sheet.getRange('A1:A12').format.columnWidth=42;
  sheet.getRange('B1:F12').format.columnWidth=20;
  sheet.getRange('A3:F3').format.font={bold:true};
  sheet.getRange(spec.date_cell).setNumberFormat('mm-dd-yy');
  summary.getRange('A1:B6').format.font={name:'Arial',size:11};
  summary.getRange('A1:A6').format.columnWidth=56;
  summary.getRange('B1:B6').format.columnWidth=24;
  summary.getRange('A1').format.font={name:'Arial',size:14,bold:true};
  wb.recalculate();
  for (const [reference,expected] of Object.entries(spec.initial_expected)) {
    const [name,address]=reference.includes('!')?reference.split('!'):[spec.sheet,reference];
    const actual=wb.worksheets.getItem(name).getRange(address).values[0][0];
    if (actual!==expected) throw Error(spec.id+' initial literal mismatch at '+reference);
  }
  for (const [name,range] of [['Summary','A1:B6'],[spec.sheet,'A1:F12']]) {
    const check=await wb.inspect({kind:'region',sheetId:name,range,maxChars:4000});
    await fs.writeFile(path.join(evidenceDir,`${spec.id}-${name}.inspect.ndjson`),check.ndjson);
    const png=await wb.render({sheetName:name,range,scale:1.4,format:'png'});
    await fs.writeFile(path.join(evidenceDir,`${spec.id}-${name}.png`),new Uint8Array(await png.arrayBuffer()));
  }
  const errorScan=await wb.inspect({kind:'match',searchTerm:'#REF!|#DIV/0!|#VALUE!|#NAME\\?|#N/A|#NUM!|#NULL!',
    options:{useRegex:true,maxResults:100},summary:'initial fixture formula errors'});
  await fs.writeFile(path.join(evidenceDir,`${spec.id}-errors.ndjson`),errorScan.ndjson);
  const file=await SpreadsheetFile.exportXlsx(wb);
  await file.save(path.join(outputDir,spec.id+'.xlsx'));
  receipt.cases.push({id:spec.id,initial_literal_check:'PASS',rendered_sheets:2,
    expected_system:spec.date_system,actual_package_policy:'NOT_CHECKED'});
}
await fs.writeFile(path.join(evidenceDir,'generation.json'),JSON.stringify(receipt,null,2)+'\n');
console.log('Three candidate XLSX generated; package policy NOT_CHECKED; no manifest sealed; no Office or Zen run.');
