# PROBE_SOLUTION: solution path relative to the staged sources root (e.g. src\App.sln).
# PROBE_PRIVATE_PREFIX: package-id prefix of the private-feed packages (used only for a count).
$ErrorActionPreference = 'Continue'
$gw = (Get-NetRoute -DestinationPrefix '0.0.0.0/0' | Select-Object -First 1).NextHop
Invoke-WebRequest -UseBasicParsing -Uri "http://$gw`:8765/privpkgs.zip" -OutFile C:\probe\privpkgs.zip
Expand-Archive -Path C:\probe\privpkgs.zip -DestinationPath C:\probe\privpkgs -Force
Copy-Item -Recurse -Force C:\probe\privpkgs\packages\* C:\probe\sources\packages\
"packages dirs now: $((Get-ChildItem C:\probe\sources\packages -Directory | Measure-Object).Count)"
"private dlls present: $((Get-ChildItem -Recurse -Filter '*.dll' C:\probe\sources\packages\$env:PROBE_PRIVATE_PREFIX* | Measure-Object).Count)"
Set-Location (Split-Path -Parent (Join-Path 'C:\probe\sources' $env:PROBE_SOLUTION))
& C:\probe\nuget.exe restore $env:PROBE_SOLUTION -NonInteractive -Verbosity quiet 2>&1 | Out-File C:\probe\nuget_restore2.log
"restore exit=$LASTEXITCODE; missing now: $((Select-String -Path C:\probe\nuget_restore2.log -Pattern 'Unable to find version' | Measure-Object).Count)"
$sw = [Diagnostics.Stopwatch]::StartNew()
Set-Location C:\probe
& C:\probe\probe\LegacyProbe.exe (Join-Path 'C:\probe\sources' $env:PROBE_SOLUTION) C:\probe\report3.json 2> C:\probe\probe_progress3.log
"probe exit=$LASTEXITCODE in $([math]::Round($sw.Elapsed.TotalSeconds,1))s"
"report bytes: $((Get-Item C:\probe\report3.json -ErrorAction SilentlyContinue).Length); errors: $((Get-Content C:\probe\report3.errors.jsonl -ErrorAction SilentlyContinue | Measure-Object -Line).Lines)"
