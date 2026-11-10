$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)
$pin = Get-Content -LiteralPath 'assets/webview2-runtime.json' -Raw | ConvertFrom-Json
$source = [uri]$pin.source
if ($source.Scheme -ne 'https' -or $source.Host -ne 'msedge.sf.dl.delivery.mp.microsoft.com' -or $pin.architecture -ne 'x64' -or $pin.version -notmatch '^\d+\.\d+\.\d+\.\d+$') { throw 'Invalid official runtime pin' }
$cab = Join-Path (Get-Location) ('downloads/Microsoft.WebView2.FixedVersionRuntime.'+$pin.version+'.x64.cab')
if (!(Test-Path -LiteralPath $cab)) { Invoke-WebRequest -Uri $pin.source -OutFile $cab }
if ((Get-FileHash -LiteralPath $cab -Algorithm SHA256).Hash -ne $pin.sha256) { throw 'WebView2 archive checksum mismatch' }
$stage = Join-Path (Get-Location) ('vendor/webview2-'+$pin.version)
$runtime = Join-Path $stage ('Microsoft.WebView2.FixedVersionRuntime.'+$pin.version+'.x64')
if (!(Test-Path -LiteralPath (Join-Path $runtime 'msedgewebview2.exe'))) {
    New-Item -ItemType Directory -Path $stage -Force | Out-Null
    & "$env:SystemRoot\System32\expand.exe" $cab '-F:*' $stage > 'downloads/webview2-expand.log'
    if ($LASTEXITCODE -ne 0) { throw 'Runtime extraction failed' }
}
foreach ($file in @('msedgewebview2.exe','msedge.dll','icudtl.dat','resources.pak')) {
    if (!(Test-Path -LiteralPath (Join-Path $runtime $file))) { throw ('Incomplete runtime: '+$file) }
}
foreach ($file in @('msedgewebview2.exe','msedge.dll')) {
    $signature = Get-AuthenticodeSignature -LiteralPath (Join-Path $runtime $file)
    if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'O=Microsoft Corporation') { throw ('Invalid Microsoft signature: '+$file) }
}
Write-Output $runtime
