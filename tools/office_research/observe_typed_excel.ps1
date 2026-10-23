param(
 [Parameter(Mandatory=$true)][string]$Fixtures,
 [Parameter(Mandatory=$true)][string]$ManifestSha256,
 [Parameter(Mandatory=$true)][ValidateSet('budget','orders','attendance')][string]$Case,
 [Parameter(Mandatory=$true)][string]$ExpectedExcelExecutable,
 [Parameter(Mandatory=$true)][string]$ExpectedExcelSha256,
 [Parameter(Mandatory=$true)][string]$NewRunDirectory
)
$ErrorActionPreference='Stop'
Add-Type @'
using System; using System.Runtime.InteropServices;
public static class ZenTypedExcelWindow {
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
}
'@
function Release-Com($Value) {
 if ($null -ne $Value -and [Runtime.InteropServices.Marshal]::IsComObject($Value)) {
  [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($Value)
 }
}
$fixtureRoot=(Resolve-Path -LiteralPath $Fixtures).Path
$manifestPath=Join-Path $fixtureRoot 'manifest.json'
if ((Get-FileHash -LiteralPath $manifestPath -Algorithm SHA256).Hash.ToLower() -ne $ManifestSha256) {throw 'Pinned manifest mismatch'}
$manifest=Get-Content -LiteralPath $manifestPath -Encoding UTF8 -Raw | ConvertFrom-Json
foreach ($entry in $manifest.sha256.PSObject.Properties) {
 if ((Get-FileHash -LiteralPath (Join-Path $fixtureRoot $entry.Name) -Algorithm SHA256).Hash.ToLower() -ne $entry.Value) {throw 'Sealed file mismatch'}
}
if ((Get-FileHash -LiteralPath $ExpectedExcelExecutable -Algorithm SHA256).Hash.ToLower() -ne $ExpectedExcelSha256) {throw 'Excel executable identity changed; review before execution'}
$plan=Get-Content -LiteralPath (Join-Path $fixtureRoot 'plan.json') -Encoding UTF8 -Raw | ConvertFrom-Json
$spec=@($plan.cases | Where-Object {$_.id -eq $Case})[0]
$runPath=[IO.Path]::GetFullPath($NewRunDirectory)
if (Test-Path -LiteralPath $runPath) {throw 'NewRunDirectory must not exist; inspect receipt instead of replay'}
[void][IO.Directory]::CreateDirectory($runPath)
$source=Join-Path $fixtureRoot ($Case+'.xlsx')
$copy=Join-Path $runPath 'input-copy.xlsx'
[IO.File]::Copy($source,$copy,$false)
# File.Copy inherits readonly source attributes. Only this owned copy may be writable.
[IO.File]::SetAttributes($copy,[IO.FileAttributes]::Normal)
$beforeIds=@(Get-Process EXCEL -ErrorAction SilentlyContinue | ForEach-Object {$_.Id})
$receipt=[ordered]@{case=$Case;started_utc=[DateTime]::UtcNow.ToString('o');status='STARTING';manifest_sha256=$ManifestSha256;before_excel_ids=$beforeIds;stages=@();outputs=@();error=$null;cleanup='NOT_RUN'}
$sequence=0
function Checkpoint {
 $script:sequence++
 $file=Join-Path $runPath ('receipt-{0:D3}.json' -f $script:sequence)
 [IO.File]::WriteAllText($file+'.partial',($receipt | ConvertTo-Json -Depth 24),(New-Object Text.UTF8Encoding($false)))
 [IO.File]::Move($file+'.partial',$file)
}
$tracked=@{}
foreach ($group in @('numbers','labels','formulas')) {foreach ($p in $spec.$group.PSObject.Properties) {$tracked[$spec.sheet+'!'+$p.Name]=$true}}
foreach ($p in $spec.summary_formulas.PSObject.Properties) {$tracked['Summary!'+$p.Name]=$true}
function Read-Stage($Workbook,[string]$Name) {
 $all=[ordered]@{}; $worksheets=$null
 try {
  $worksheets=$Workbook.Worksheets
  foreach ($key in ($tracked.Keys | Sort-Object)) {
   $parts=$key.Split('!');$ws=$null;$cell=$null
   try {
    $ws=$worksheets.Item($parts[0]);$cell=$ws.Range($parts[1]);$v=$cell.Value2
    $type=if ($null -eq $v) {'null'} else {$v.GetType().FullName}
    if ($v -is [Runtime.InteropServices.ErrorWrapper]) {$v=$v.ErrorCode}
    $all[$key]=[ordered]@{value2=$v;value_type=$type;has_formula=[bool]$cell.HasFormula;formula=$cell.Formula;text=$cell.Text;number_format=$cell.NumberFormat}
   } finally {Release-Com $cell;Release-Com $ws}
  }
 } finally {Release-Com $worksheets}
 return [ordered]@{name=$Name;observed_utc=[DateTime]::UtcNow.ToString('o');date1904=[bool]$Workbook.Date1904;cells=$all}
}
$app=$null;$books=$null;$book=$null;$sheets=$null;$sheet=$null;$owned=$false;$ownedProcess=$null
Checkpoint
try {
 $app=New-Object -ComObject Excel.Application
 [uint32]$excelProcessId=0
 [void][ZenTypedExcelWindow]::GetWindowThreadProcessId([IntPtr]$app.Hwnd,[ref]$excelProcessId)
 $ownedProcess=Get-Process -Id $excelProcessId
 $receipt.process=[ordered]@{id=$ownedProcess.Id;creation_utc=$ownedProcess.StartTime.ToUniversalTime().ToString('o');path=$ownedProcess.Path}
 if ($beforeIds -contains $ownedProcess.Id -or $ownedProcess.Path -ine $ExpectedExcelExecutable) {throw 'COM instance identity not admitted; no control or quit'}
 $books=$app.Workbooks
 if ($books.Count -ne 0) {throw 'Unexpected workbooks in instance; no control or quit'}
 $owned=$true
 $actualHash=(Get-FileHash -LiteralPath $ownedProcess.Path -Algorithm SHA256).Hash.ToLower()
 if ($actualHash -ne $ExpectedExcelSha256) {throw 'Owned process executable hash mismatch'}
 $receipt.executable=[ordered]@{sha256=$actualHash;file_version=(Get-Item -LiteralPath $ownedProcess.Path).VersionInfo.FileVersion}
 $receipt.application=[ordered]@{version=$app.Version;build=$app.Build;operating_system=$app.OperatingSystem;calculation_before=$app.Calculation}
 $receipt.status='OWNED';Checkpoint
 $app.Visible=$false;$app.DisplayAlerts=$false;$app.EnableEvents=$false;$app.AutomationSecurity=3
 $book=$books.Open($copy,0,$false)
 $app.Calculation=-4135 # manual; explicit full rebuild after each operation
 $app.CalculateFullRebuild()
 $receipt.stages+=(Read-Stage $book 'initial');Checkpoint
 $index=0
 foreach ($op in $spec.operations) {
  $range=$null;$sheets=$null;$sheet=$null
  try {
   $sheets=$book.Worksheets;$sheet=$sheets.Item($spec.sheet)
   $range=$sheet.Range($op.cell)
   switch ($op.kind) {
    'text' {$range.Value2=[string]$op.value}
    'boolean' {$range.Value2=[bool]$op.value}
    'number' {$range.Value2=[double]$op.value}
    'date_serial' {$range.Value2=[double]$op.value}
    'formula' {$range.Formula='='+[string]$op.value}
    'blank' {[void]$range.ClearContents()}
    'clear_formula' {[void]$range.ClearContents()}
    default {throw 'Unrecognized operation'}
   }
  } finally {Release-Com $range;Release-Com $sheet;$sheet=$null;Release-Com $sheets;$sheets=$null}
  $app.CalculateFullRebuild();$index++
  $receipt.stages+=(Read-Stage $book ('step-'+$index));Checkpoint
  if ($index -in @(4,5,8)) {
   $output=Join-Path $runPath ('saved-'+$index+'.xlsx')
   if (Test-Path -LiteralPath $output) {throw 'Refusing output overwrite'}
   if ($index -eq 8) {$book.SaveAs($output,51)} else {$book.SaveCopyAs($output)}
   $receipt.outputs+=([ordered]@{step=$index;name=('saved-'+$index+'.xlsx')});Checkpoint
  }
 }
 Release-Com $sheet;$sheet=$null;Release-Com $sheets;$sheets=$null
 $book.Close($false);Release-Com $book;$book=$null
 foreach ($entry in $receipt.outputs) {
  $entry.sha256=(Get-FileHash -LiteralPath (Join-Path $runPath $entry.name) -Algorithm SHA256).Hash.ToLower()
  $book=$books.Open((Join-Path $runPath $entry.name),0,$true)
  # Do not recalculate reopened files; raw cache verification is separate too.
  $receipt.stages+=(Read-Stage $book ('reopened-'+$entry.step));Checkpoint
  $book.Close($false);Release-Com $book;$book=$null
 }
 $receipt.source_sha256=(Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLower()
 $receipt.copy_sha256=(Get-FileHash -LiteralPath $copy -Algorithm SHA256).Hash.ToLower()
 $receipt.status='OBSERVED';Checkpoint
} catch {$receipt.status='FAILED';$receipt.error=$_.Exception.ToString();$receipt.script_stack=$_.ScriptStackTrace;Checkpoint}
finally {
 if ($owned) {
  try {
   Release-Com $sheet;$sheet=$null;Release-Com $sheets;$sheets=$null
   if ($null -ne $book) {$book.Close($false);Release-Com $book;$book=$null}
   $app.Quit();$receipt.cleanup='QUIT_REQUESTED'
  } catch {$receipt.cleanup='FAILED: '+$_.Exception.Message}
 } else {$receipt.cleanup='NOT_OWNED_NO_QUIT'}
 Release-Com $books;Release-Com $app
 [GC]::Collect();[GC]::WaitForPendingFinalizers()
 if ($owned -and $null -ne $ownedProcess) {
  $receipt.process_remaining=$true
  for ($check=0;$check -lt 20;$check++) {
   $alive=Get-Process -Id $ownedProcess.Id -ErrorAction SilentlyContinue
   $receipt.process_remaining=($null -ne $alive -and $alive.StartTime.ToUniversalTime().ToString('o') -eq $receipt.process.creation_utc)
   if (-not $receipt.process_remaining) {break}
   Start-Sleep -Milliseconds 500
  }
 }
 $receipt.ended_utc=[DateTime]::UtcNow.ToString('o');Checkpoint
}
if ($receipt.status -ne 'OBSERVED' -or $receipt.cleanup -ne 'QUIT_REQUESTED' -or $receipt.process_remaining) {Write-Output ($receipt | ConvertTo-Json -Depth 4);exit 2}
Write-Output ('OBSERVED '+$Case+'; independent verification required; receipt '+$sequence)
