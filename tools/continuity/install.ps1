param([Parameter(Mandatory=$true)][string]$PythonPath)
$ErrorActionPreference = 'Stop'
$project = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$data = Join-Path $project 'target/continuity'
New-Item -ItemType Directory -Path $data -Force | Out-Null
$name = 'ZenOffice-Continuity-Recovery'
$sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$shell = (Get-Process -Id $PID).Path
$script = Join-Path $PSScriptRoot 'recover.ps1'
if (-not (Test-Path -LiteralPath $PythonPath -PathType Leaf)) { throw 'Python runtime missing' }
$escape = {param($value) [Security.SecurityElement]::Escape($value)}
$arguments = '-NoProfile -WindowStyle Hidden -File "' + $script + '" -PythonPath "' + $PythonPath + '"'
$start = [DateTime]::Now.AddMinutes(5).ToString('yyyy-MM-ddTHH:mm:ss')
$subscription = '<QueryList><Query Id="0" Path="System"><Select Path="System">*[System[Provider[@Name="Microsoft-Windows-Power-Troubleshooter"] and EventID=1]]</Select></Query></QueryList>'
$existing = Get-ScheduledTask -TaskName $name -ErrorAction SilentlyContinue
if ($existing) {
    $current = Export-ScheduledTask -TaskName $name
    if (-not $current.Contains('ZenOffice continuity supervisor')) { throw 'Task name is owned by another definition; not overwriting' }
    $current | Set-Content -LiteralPath (Join-Path $data ('task-before-' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfff') + '.xml')) -Encoding utf8
} else {
    'ABSENT' | Set-Content -LiteralPath (Join-Path $data 'task-original-state.txt') -Encoding utf8
}
$xml = @"
<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.4" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
<RegistrationInfo><Description>ZenOffice continuity supervisor; no model dispatch or command replay.</Description></RegistrationInfo>
<Triggers>
<LogonTrigger><Enabled>true</Enabled><UserId>$sid</UserId><Delay>PT30S</Delay></LogonTrigger>
<TimeTrigger><Repetition><Interval>PT5M</Interval><StopAtDurationEnd>false</StopAtDurationEnd></Repetition><StartBoundary>$start</StartBoundary><Enabled>true</Enabled></TimeTrigger>
<EventTrigger><Enabled>true</Enabled><Subscription>$(& $escape $subscription)</Subscription><Delay>PT20S</Delay></EventTrigger>
</Triggers>
<Principals><Principal id="User"><UserId>$sid</UserId><LogonType>InteractiveToken</LogonType><RunLevel>LeastPrivilege</RunLevel></Principal></Principals>
<Settings><MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy><DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries><StopIfGoingOnBatteries>false</StopIfGoingOnBatteries><AllowHardTerminate>true</AllowHardTerminate><StartWhenAvailable>true</StartWhenAvailable><RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable><Enabled>true</Enabled><Hidden>true</Hidden><WakeToRun>false</WakeToRun><ExecutionTimeLimit>PT3M</ExecutionTimeLimit><RestartOnFailure><Interval>PT1M</Interval><Count>3</Count></RestartOnFailure></Settings>
<Actions Context="User"><Exec><Command>$(& $escape $shell)</Command><Arguments>$(& $escape $arguments)</Arguments><WorkingDirectory>$(& $escape $project)</WorkingDirectory></Exec></Actions>
</Task>
"@
Register-ScheduledTask -TaskName $name -Xml $xml -Force | Out-Null
Export-ScheduledTask -TaskName $name | Set-Content -LiteralPath (Join-Path $data 'installed-task.xml') -Encoding utf8
Start-ScheduledTask -TaskName $name
Write-Output 'Registered and started ZenOffice recovery task. No shutdown/sleep test was performed.'
