param([string]$PythonPath, [string]$CodexPath)
$ErrorActionPreference = 'Stop'
$guardProject = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$guardData = Join-Path $guardProject 'target/quota_guard'
$guardScript = Join-Path $PSScriptRoot 'guard.py'
New-Item -ItemType Directory -Path $guardData -Force | Out-Null
$guardPidFile = Join-Path $guardData 'daemon.json'
if (Test-Path -LiteralPath $guardPidFile) {
    try {
        $guardExisting = Get-Content -LiteralPath $guardPidFile -Raw | ConvertFrom-Json
        $guardProcess = Get-Process -Id $guardExisting.pid -ErrorAction Stop
        $guardProcessInfo = Get-CimInstance Win32_Process -Filter "ProcessId=$($guardExisting.pid)"
        $guardActualUnix = ([DateTimeOffset]$guardProcess.StartTime.ToUniversalTime()).ToUnixTimeSeconds()
        if ([Math]::Abs($guardActualUnix - $guardExisting.started_unix) -lt 3 -and $guardProcessInfo.CommandLine.Contains($guardScript) -and $guardProcessInfo.CommandLine.Contains('--daemon')) {
            Write-Output "Quota monitor already running: PID $($guardExisting.pid)"
            Write-Output (Join-Path $guardData 'STATUS.md')
            exit 0
        }
    } catch { }
}
if (-not $PythonPath) {
    $PythonPath = Join-Path $env:USERPROFILE '.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe'
    if (-not (Test-Path -LiteralPath $PythonPath)) { $PythonPath = (Get-Command python.exe -ErrorAction Stop).Source }
}
if (-not $CodexPath) {
    $guardCommand = Get-Command codex.exe -ErrorAction SilentlyContinue
    if ($guardCommand) { $CodexPath = $guardCommand.Source }
    else {
        $CodexPath = Get-ChildItem (Join-Path $env:LOCALAPPDATA 'OpenAI/Codex/bin') -Recurse -Filter codex.exe -File |
            Sort-Object LastWriteTime -Descending | Select-Object -First 1 -ExpandProperty FullName
    }
}
if (-not $CodexPath -or -not (Test-Path -LiteralPath $CodexPath)) { throw 'Installed Codex executable not found; supply -CodexPath' }
$guardArgs = @(('"{0}"' -f $guardScript), '--daemon', '--codex', ('"{0}"' -f $CodexPath), '--state-dir', ('"{0}"' -f $guardData))
$guardNew = Start-Process -FilePath $PythonPath -ArgumentList $guardArgs -WindowStyle Hidden -WorkingDirectory $guardProject -PassThru -RedirectStandardOutput (Join-Path $guardData 'daemon.stdout.log') -RedirectStandardError (Join-Path $guardData 'daemon.stderr.log')
Write-Output "Quota monitor launched: PID $($guardNew.Id)"
Write-Output (Join-Path $guardData 'STATUS.md')
