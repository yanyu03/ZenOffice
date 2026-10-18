$ErrorActionPreference = 'Stop'
$tokens=$null; $issues=$null
$source = [Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot 'recover.ps1'),[ref]$tokens,[ref]$issues)
if ($issues.Count) { throw 'Invalid recovery source' }
# Load only the pure helpers; do not execute scheduler/app/monitor side effects.
foreach ($name in @('Write-AtomicState','Get-ContinuityBootKey')) {
    $definition = $source.FindAll({param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst]},$true) |
        Where-Object Name -eq $name | Select-Object -First 1
    if (-not $definition) { throw "Missing helper: $name" }
    . ([ScriptBlock]::Create($definition.Extent.Text))
}
$project = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$parent = [IO.Path]::GetFullPath((Join-Path $project 'target/continuity-tests'))
$sandbox = Join-Path $parent ([Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $sandbox -Force | Out-Null
try {
    $path = Join-Path $sandbox 'state.json'
    Write-AtomicState $path @{sequence=1}
    if ((Get-Content -LiteralPath $path -Raw | ConvertFrom-Json).sequence -ne 1) { throw 'Initial atomic state failed' }
    Write-AtomicState $path @{sequence=2}
    Write-AtomicState $path @{sequence=3}
    if ((Get-Content -LiteralPath $path -Raw | ConvertFrom-Json).sequence -ne 3) { throw 'Repeated atomic replacement failed' }
    if (@(Get-ChildItem -LiteralPath $sandbox -Filter '*.tmp').Count) { throw 'Temporary files leaked' }
    $key=Get-ContinuityBootKey ([DateTime]::Parse('2026-10-18T01:00:00Z').ToUniversalTime())
    $roundtrip=(@{boot=$key} | ConvertTo-Json | ConvertFrom-Json).boot
    if ($roundtrip -isnot [string] -or $roundtrip -ne $key) { throw 'Boot identity JSON coercion regression' }
    Write-Output 'PASS: 3 PowerShell cases (create, repeated atomic overwrite, opaque boot identity); no system tasks registered'
} finally {
    $resolved=[IO.Path]::GetFullPath($sandbox)
    if (-not $resolved.StartsWith($parent + [IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)) { throw 'Unsafe cleanup target' }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
