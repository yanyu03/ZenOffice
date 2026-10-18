param([Parameter(Mandatory=$true)][string]$PythonPath)
$ErrorActionPreference = 'Stop'
$project = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$data = Join-Path $project 'target/continuity'
New-Item -ItemType Directory -Path $data -Force | Out-Null
$mutex = [Threading.Mutex]::new($false, 'Local\ZenOffice-Continuity-Recovery')
$held = $false
function Write-AtomicState([string]$Path, $Value) {
    $temporary = $Path + '.' + [Guid]::NewGuid().ToString('N') + '.tmp'
    try {
        $bytes = [Text.UTF8Encoding]::new($false).GetBytes(($Value | ConvertTo-Json -Depth 12) + "`n")
        $stream = [IO.File]::Open($temporary, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
        try { $stream.Write($bytes, 0, $bytes.Length); $stream.Flush($true) } finally { $stream.Dispose() }
        # This launcher uses pwsh/.NET: avoid PowerShell coercing a null backup
        # path to an empty string in File.Replace's overload binding.
        [IO.File]::Move($temporary, $Path, $true)
    } finally { if ([IO.File]::Exists($temporary)) { [IO.File]::Delete($temporary) } }
}
function Get-ContinuityBootKey([DateTime]$StartTime) {
    return 'boot:' + $StartTime.ToUniversalTime().ToFileTimeUtc().ToString()
}
try {
    $held = $mutex.WaitOne(0)
    if (-not $held) { exit 0 }
    $automation = & $PythonPath (Join-Path $PSScriptRoot 'continuity.py') automation-status
    if ($LASTEXITCODE -ne 0) { throw 'Automation state unavailable; no automatic launch' }
    $automation = $automation | ConvertFrom-Json
    if ($automation.status -ne 'ACTIVE') { exit 0 } # Never unpause user/quota stops.
    $shell = (Get-Process -Id $PID).Path
    & $shell -NoProfile -File (Join-Path $project 'tools/quota_guard/start.ps1') -PythonPath $PythonPath |
        Set-Content -LiteralPath (Join-Path $data 'monitor-start.log') -Encoding utf8
    if ($LASTEXITCODE -ne 0) { throw 'Quota monitor startup failed' }
    $inspection = & $PythonPath (Join-Path $PSScriptRoot 'continuity.py') inspect
    if ($LASTEXITCODE -ne 0) { throw 'Checkpoint inspection failed' }
    Write-AtomicState (Join-Path $data 'recovery-inspection.json') ($inspection | ConvertFrom-Json)
    # An opaque key avoids ConvertFrom-Json coercing ISO timestamps to DateTime.
    $boot = Get-ContinuityBootKey (Get-CimInstance Win32_OperatingSystem).LastBootUpTime
    $marker = Join-Path $data 'startup.json'
    $previousBoot = $null
    if (Test-Path -LiteralPath $marker) { $previousBoot = (Get-Content -LiteralPath $marker -Raw | ConvertFrom-Json).boot }
    $appAction = 'not_relaunched_same_boot'
    if ($previousBoot -ne $boot) {
        $package = Get-AppxPackage -Name 'OpenAI.Codex' | Select-Object -First 1
        if (-not $package) { throw 'Installed Codex desktop package not found' }
        $existing = @(Get-CimInstance Win32_Process -Filter "Name='ChatGPT.exe'" |
            Where-Object { $_.ExecutablePath -and $_.ExecutablePath.StartsWith($package.InstallLocation, [StringComparison]::OrdinalIgnoreCase) })
        if ($existing.Count -eq 0) {
            $appId = $package.PackageFamilyName + '!App'
            Start-Process -FilePath (Join-Path $env:WINDIR 'explorer.exe') -ArgumentList ('shell:AppsFolder\' + $appId) -WindowStyle Hidden
            $appAction = 'desktop_activation_requested_not_yet_verified'
        } else { $appAction = 'desktop_already_running' }
        Write-AtomicState $marker @{ boot=$boot; observed_at=[DateTime]::UtcNow.ToString('o'); app_action=$appAction }
    }
    Write-AtomicState (Join-Path $data 'supervisor.json') @{ checked_at=[DateTime]::UtcNow.ToString('o'); automation='ACTIVE'; app_action=$appAction; replayed_commands=$false }
} catch {
    Write-AtomicState (Join-Path $data 'supervisor-error.json') @{ checked_at=[DateTime]::UtcNow.ToString('o'); status='FAILED'; reason=$_.Exception.Message; replayed_commands=$false }
    exit 1
} finally {
    if ($held) { $mutex.ReleaseMutex() }
    $mutex.Dispose()
}
