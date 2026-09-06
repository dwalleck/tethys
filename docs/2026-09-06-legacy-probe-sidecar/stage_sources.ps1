$ErrorActionPreference = 'Stop'
$gw = (Get-NetRoute -DestinationPrefix '0.0.0.0/0' | Select-Object -First 1).NextHop
New-Item -ItemType Directory -Force -Path C:\probe | Out-Null
$sw = [Diagnostics.Stopwatch]::StartNew()
Invoke-WebRequest -UseBasicParsing -Uri "http://$gw`:8765/sources.zip" -OutFile C:\probe\sources.zip
"downloaded sources.zip $((Get-Item C:\probe\sources.zip).Length) bytes in $($sw.Elapsed.TotalSeconds)s"
if (Test-Path C:\probe\sources) { Remove-Item -Recurse -Force C:\probe\sources }
Expand-Archive -Path C:\probe\sources.zip -DestinationPath C:\probe\sources -Force
"expanded: $((Get-ChildItem -Recurse -File C:\probe\sources | Measure-Object).Count) files in $($sw.Elapsed.TotalSeconds)s"
if (-not (Test-Path C:\probe\nuget.exe)) {
  Invoke-WebRequest -UseBasicParsing -Uri 'https://dist.nuget.org/win-x86-commandline/latest/nuget.exe' -OutFile C:\probe\nuget.exe
}
"nuget.exe: $((Get-Item C:\probe\nuget.exe).Length) bytes; version: $((& C:\probe\nuget.exe help | Select-Object -First 1))"
