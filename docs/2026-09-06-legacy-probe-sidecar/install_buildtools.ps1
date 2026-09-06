$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Force -Path C:\probe | Out-Null
$boot = 'C:\probe\vs_BuildTools.exe'
if (-not (Test-Path $boot)) {
  Invoke-WebRequest -UseBasicParsing -Uri 'https://aka.ms/vs/17/release/vs_BuildTools.exe' -OutFile $boot
}
$args = @(
  '--quiet','--wait','--norestart','--nocache',
  '--installPath','C:\BuildTools',
  '--add','Microsoft.VisualStudio.Workload.MSBuildTools',
  '--add','Microsoft.VisualStudio.Workload.WebBuildTools',
  '--add','Microsoft.Net.Component.4.8.TargetingPack',
  '--add','Microsoft.Net.Component.4.7.2.TargetingPack',
  '--add','Microsoft.VisualStudio.Component.NuGet.BuildTools',
  '--add','Microsoft.VisualStudio.Component.Roslyn.Compiler',
  '--includeRecommended'
)
$p = Start-Process -FilePath $boot -ArgumentList $args -PassThru -RedirectStandardOutput C:\probe\vs_install.out -RedirectStandardError C:\probe\vs_install.err
"started pid=$($p.Id) at $(Get-Date -Format o)"
