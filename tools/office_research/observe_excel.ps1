param(
    [Parameter(Mandatory=$true)][string]$Fixtures,
    [Parameter(Mandatory=$true)][ValidateSet('budget','orders','attendance')][string]$Case,
    [Parameter(Mandatory=$true)][string]$NewRunDirectory
)
$ErrorActionPreference = 'Stop'
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class ZenExcelWindow {
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
}
'@
function Release-Com($Value) {
    if ($null -ne $Value -and [Runtime.InteropServices.Marshal]::IsComObject($Value)) {
        [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($Value)
    }
}
$fixtureRoot = (Resolve-Path -LiteralPath $Fixtures).Path
$runPath = [IO.Path]::GetFullPath($NewRunDirectory)
if (Test-Path -LiteralPath $runPath) { throw 'Run directory must be new; inspect existing receipt instead of replaying' }
[void][IO.Directory]::CreateDirectory($runPath)
$plan = Get-Content -LiteralPath (Join-Path $fixtureRoot 'plan.json') -Raw | ConvertFrom-Json
$manifest = Get-Content -LiteralPath (Join-Path $fixtureRoot 'manifest.json') -Raw | ConvertFrom-Json
foreach ($entry in $manifest.sha256.PSObject.Properties) {
    if ((Get-FileHash -LiteralPath (Join-Path $fixtureRoot $entry.Name) -Algorithm SHA256).Hash.ToLower() -ne $entry.Value) { throw 'Frozen fixture identity mismatch' }
}
$spec = @($plan.cases | Where-Object { $_.id -eq $Case })[0]
$source = Join-Path $fixtureRoot ($Case + '.xlsx')
$copy = Join-Path $runPath 'input-copy.xlsx'
$output = Join-Path $runPath 'excel-output.xlsx'
Copy-Item -LiteralPath $source -Destination $copy
$expectedExecutable = $manifest.office_file_identity.path
$beforeIds = @(Get-Process EXCEL -ErrorAction SilentlyContinue | ForEach-Object { $_.Id })
$receipt = [ordered]@{case=$Case; started_utc=[DateTime]::UtcNow.ToString('o'); status='STARTING'; before_excel_ids=$beforeIds; stages=@(); error=$null; cleanup='NOT_RUN'}
$sequence = 0
function Checkpoint {
    $script:sequence++
    $file = Join-Path $runPath ('receipt-{0:D3}.json' -f $script:sequence)
    $temporary = $file + '.partial'
    [IO.File]::WriteAllText($temporary, ($receipt | ConvertTo-Json -Depth 16), (New-Object Text.UTF8Encoding($false)))
    [IO.File]::Move($temporary, $file)
}
function Read-Stage($Worksheet, $Workbook, [string]$Name) {
    $addresses = @($spec.formulas.PSObject.Properties.Name) + @($spec.date.cell)
    for ($i=0; $i -lt $spec.rows.Count; $i++) { $addresses += @('A','B','C') | ForEach-Object { $_ + ($i+4) } }
    $cells = [ordered]@{}
    foreach ($address in ($addresses | Select-Object -Unique)) {
        $range = $null
        try {
            $range = $Worksheet.Range($address)
            $value = $range.Value2
            $typeName = if ($null -eq $value) { 'null' } else { $value.GetType().FullName }
            if ($value -is [Runtime.InteropServices.ErrorWrapper]) { $value = $value.ErrorCode }
            $cells[$address] = [ordered]@{value2=$value; value_type=$typeName; formula=$range.Formula; text=$range.Text; number_format=$range.NumberFormat; has_formula=[bool]$range.HasFormula}
        } finally { Release-Com $range }
    }
    return [ordered]@{name=$Name; observed_utc=[DateTime]::UtcNow.ToString('o'); date1904=[bool]$Workbook.Date1904; cells=$cells}
}
$app=$null; $books=$null; $book=$null; $sheets=$null; $sheet=$null
$owned=$false; $ownedProcess=$null
Checkpoint
try {
    $app = New-Object -ComObject Excel.Application
    [uint32]$excelProcessId=0
    [void][ZenExcelWindow]::GetWindowThreadProcessId([IntPtr]$app.Hwnd, [ref]$excelProcessId)
    $ownedProcess = Get-Process -Id $excelProcessId
    $receipt.process = [ordered]@{id=$ownedProcess.Id; creation_utc=$ownedProcess.StartTime.ToUniversalTime().ToString('o'); path=$ownedProcess.Path}
    if ($beforeIds -contains $ownedProcess.Id -or $ownedProcess.Path -ine $expectedExecutable) { throw 'COM instance identity not admitted; refusing to control or quit it' }
    $books=$app.Workbooks
    if ($books.Count -ne 0) { throw 'New instance has unexpected workbooks; refusing to control or quit it' }
    $owned=$true
    $receipt.executable = [ordered]@{file_version=(Get-Item -LiteralPath $ownedProcess.Path).VersionInfo.FileVersion; sha256=(Get-FileHash -LiteralPath $ownedProcess.Path -Algorithm SHA256).Hash.ToLower()}
    $receipt.application = [ordered]@{version=$app.Version; build=$app.Build; operating_system=$app.OperatingSystem; decimal_separator=$app.DecimalSeparator; thousands_separator=$app.ThousandsSeparator; use_system_separators=$app.UseSystemSeparators; calculation_before=$app.Calculation}
    $receipt.status='OWNED'; Checkpoint
    $app.Visible=$false; $app.DisplayAlerts=$false; $app.EnableEvents=$false; $app.AutomationSecurity=3
    $book=$books.Open($copy,0,$false)
    $app.Calculation=-4105
    $receipt.application.calculation_after=$app.Calculation
    $receipt.application.country_code=$app.International(1)
    $receipt.application.country_setting=$app.International(2)
    $sheets=$book.Worksheets; $sheet=$sheets.Item($spec.sheet)
    foreach ($stage in $spec.stages) {
        if ($null -ne $stage.edit) {
            $range=$null
            try { $range=$sheet.Range($stage.edit.cell); $range.Value2=[double]$stage.edit.value }
            finally { Release-Com $range }
        }
        $app.CalculateFullRebuild()
        $receipt.stages += (Read-Stage $sheet $book $stage.id)
        Checkpoint
    }
    if (Test-Path -LiteralPath $output) { throw 'Refusing to overwrite output' }
    $book.SaveAs($output,51)
    Release-Com $sheet; $sheet=$null; Release-Com $sheets; $sheets=$null
    $book.Close($false); Release-Com $book; $book=$null
    $book=$books.Open($output,0,$true)
    $sheets=$book.Worksheets; $sheet=$sheets.Item($spec.sheet)
    $receipt.stages += (Read-Stage $sheet $book 'reopened')
    $receipt.source_sha256=(Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLower()
    $receipt.copy_sha256=(Get-FileHash -LiteralPath $copy -Algorithm SHA256).Hash.ToLower()
    $receipt.output_sha256=(Get-FileHash -LiteralPath $output -Algorithm SHA256).Hash.ToLower()
    $receipt.status='OBSERVED'; Checkpoint
} catch {
    $receipt.status='FAILED'; $receipt.error=$_.Exception.ToString(); Checkpoint
} finally {
    if ($owned) {
        try {
            Release-Com $sheet; $sheet=$null; Release-Com $sheets; $sheets=$null
            if ($null -ne $book) { $book.Close($false); Release-Com $book; $book=$null }
            $app.Quit()
            $receipt.cleanup='QUIT_REQUESTED'
        } catch { $receipt.cleanup='FAILED: ' + $_.Exception.Message }
    } else { $receipt.cleanup='NOT_OWNED_NO_QUIT' }
    Release-Com $books; Release-Com $app
    [GC]::Collect(); [GC]::WaitForPendingFinalizers()
    if ($owned -and $null -ne $ownedProcess) {
        Start-Sleep -Milliseconds 700
        $alive=Get-Process -Id $ownedProcess.Id -ErrorAction SilentlyContinue
        $receipt.process_remaining = ($null -ne $alive -and $alive.StartTime.ToUniversalTime().ToString('o') -eq $receipt.process.creation_utc)
    }
    $receipt.ended_utc=[DateTime]::UtcNow.ToString('o'); Checkpoint
}
if ($receipt.status -ne 'OBSERVED' -or $receipt.cleanup -ne 'QUIT_REQUESTED' -or $receipt.process_remaining) {
    Write-Output ($receipt | ConvertTo-Json -Depth 4); exit 2
}
Write-Output ('OBSERVED ' + $Case + '; independent verification required; receipt ' + $sequence)
