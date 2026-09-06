# PROBE_SOLUTION: solution path relative to the staged sources root (e.g. src\App.sln).
# PROBE_PRIVATE_PREFIX: package-id prefix of the private-feed packages (used only for a count).
$ErrorActionPreference = 'Continue'
$sw = [Diagnostics.Stopwatch]::StartNew()
Set-Location (Split-Path -Parent (Join-Path 'C:\probe\sources' $env:PROBE_SOLUTION))
& C:\probe\nuget.exe restore $env:PROBE_SOLUTION -NonInteractive -Verbosity quiet 2>&1 | Out-File C:\probe\nuget_restore.log
"restore exit=$LASTEXITCODE in $([math]::Round($sw.Elapsed.TotalSeconds,1))s"
"packages dirs: $((Get-ChildItem C:\probe\sources\packages -Directory -ErrorAction SilentlyContinue | Measure-Object).Count)"
"log tail: " + ((Get-Content C:\probe\nuget_restore.log -Tail 8) -join ' | ')
"errors in log: $((Select-String -Path C:\probe\nuget_restore.log -Pattern 'error|Unable to find|NU1' -SimpleMatch:$false | Measure-Object).Count)"
