# PROBE_SOLUTION: solution path relative to the staged sources root (e.g. src\App.sln).
# PROBE_PRIVATE_PREFIX: package-id prefix of the private-feed packages (used only for a count).
$ErrorActionPreference = 'Continue'
$sw = [Diagnostics.Stopwatch]::StartNew()
Set-Location C:\probe
$env:DOTNET_CLI_TELEMETRY_OPTOUT = '1'
& C:\probe\probe\LegacyProbe.exe (Join-Path 'C:\probe\sources' $env:PROBE_SOLUTION) C:\probe\report.json 2> C:\probe\probe_progress.log
"probe exit=$LASTEXITCODE in $([math]::Round($sw.Elapsed.TotalSeconds,1))s"
"report bytes: $((Get-Item C:\probe\report.json -ErrorAction SilentlyContinue).Length)"
"progress tail: " + ((Get-Content C:\probe\probe_progress.log -Tail 6) -join ' | ')
