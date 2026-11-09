
param([switch]$Build)
$ErrorActionPreference='Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)
if($Build){cargo build --release --locked --bin hashhelm;if($LASTEXITCODE -ne 0){throw 'Build failed'}}
$devRoot=Join-Path (Get-Location) 'workspace\dev-run'
$devExe=Join-Path $devRoot 'HashHelm.exe'
if(Get-Process HashHelm -ErrorAction SilentlyContinue | Where-Object {$_.Path -eq $devExe}){throw 'The development instance is already open. Close it normally before updating.'}
New-Item -ItemType Directory -Force $devRoot | Out-Null
foreach($resource in @('hashcat','assets')){
    $target=if($resource -eq 'hashcat'){Join-Path (Get-Location) 'vendor\hashcat-8.0.0'}else{Join-Path (Get-Location) 'assets'}
    $link=Join-Path $devRoot $resource
    if(!(Test-Path -LiteralPath $link)){New-Item -ItemType Junction -Path $link -Target $target | Out-Null}
}
Copy-Item -LiteralPath 'target\release\hashhelm.exe' -Destination $devExe -Force
Start-Process -FilePath $devExe -WindowStyle Normal
