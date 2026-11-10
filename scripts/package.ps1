param([switch]$Lite, [switch]$SkipBuild)
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)
$packageBuild = if ($env:HASHHELM_BUILD_NUMBER) { [int]$env:HASHHELM_BUILD_NUMBER } else { 1 }
if (!$SkipBuild) {
    cargo build --release --locked --bin hashhelm
    if ($LASTEXITCODE -ne 0) { throw 'Rust build failed' }
}
python scripts/rust-notices.py
if ($LASTEXITCODE -ne 0) { throw 'Rust notice collection failed' }
$releaseVersion = (Select-String -LiteralPath 'Cargo.toml' -Pattern '^version = "([^"]+)"' | Select-Object -First 1).Matches.Groups[1].Value
$nativeInfo = & '.\target\release\hashhelm.exe' --hashhelm-platform-info | ConvertFrom-Json
if ($LASTEXITCODE -ne 0 -or $nativeInfo.version -ne $releaseVersion -or $nativeInfo.build -ne $packageBuild -or $nativeInfo.platform -ne 'windows' -or $nativeInfo.arch -ne 'x86_64') { throw 'Prebuilt GUI metadata does not match this package' }
$buildId = (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [guid]::NewGuid().ToString('N').Substring(0,8)
$buildRoot = Join-Path (Get-Location) ('dist\packages\' + $buildId)
$destination = Join-Path $buildRoot ('HashHelm-' + $releaseVersion)
$flavor = if ($Lite) { 'lite' } else { 'offline' }
$releaseArchive = 'dist\HashHelm-' + $releaseVersion + '-build' + $packageBuild + '-windows-x64-' + $flavor + '.7z'
# Assemble each release from an empty folder; keep existing portable data intact.
$pendingArchive = Join-Path $buildRoot 'release.7z'
New-Item -ItemType Directory -Path $destination -Force | Out-Null
Copy-Item -LiteralPath 'target\release\hashhelm.exe' -Destination (Join-Path $destination 'HashHelm.exe') -Force
if (!(Test-Path -LiteralPath (Join-Path $destination 'hashcat'))) {
    tar -xf downloads\hashcat-8.0.0.7z -C $destination
    if ($LASTEXITCODE -ne 0) { throw 'Hashcat extraction failed' }
    for($hcRetry=0;$hcRetry -lt 3;$hcRetry++){
        try{Rename-Item -LiteralPath (Join-Path $destination 'hashcat-8.0.0') -NewName 'hashcat';break}catch{if($hcRetry -eq 2){throw};Start-Sleep -Milliseconds 250}
    }
}
Copy-Item -LiteralPath 'README.md','GUIDE.zh-CN.md','BUILDING.md','THIRD_PARTY.md','FORMAT-SUPPORT.md','SECURITY-NOTES.md','DESIGN.md','LICENSE','CREDITS.md' -Destination $destination -Force
Copy-Item -LiteralPath 'assets' -Destination $destination -Recurse -Force
Copy-Item -LiteralPath 'scripts/Diagnose.cmd' -Destination (Join-Path $destination 'Diagnose.cmd')
Copy-Item -LiteralPath 'scripts/Start-HashHelm.cmd' -Destination (Join-Path $destination 'Start-HashHelm.cmd')
$runtimeBytes = 0
if (!$Lite) {
    $runtimeSource = & powershell -NoProfile -File scripts/prepare-webview2.ps1
    if ($LASTEXITCODE -ne 0) { throw 'Official WebView2 preparation failed' }
    $runtimeSource = $runtimeSource | Select-Object -Last 1
    $runtimeDestination = Join-Path $destination 'runtime/webview2'
    New-Item -ItemType Directory -Path (Split-Path $runtimeDestination -Parent) -Force | Out-Null
    Copy-Item -LiteralPath $runtimeSource -Destination $runtimeDestination -Recurse
    Copy-Item -LiteralPath 'assets/webview2-runtime.json' -Destination (Join-Path $destination 'runtime/runtime-manifest.json')
    $runtimeBytes = (Get-ChildItem -LiteralPath (Join-Path $destination 'runtime') -Recurse -File | Measure-Object Length -Sum).Sum
}
$engineCopy = (Get-ChildItem -LiteralPath (Join-Path $destination 'hashcat') -File -Recurse | Measure-Object Length -Sum).Sum
$baseline = $engineCopy
$uiBytes = (Get-Item -LiteralPath (Join-Path $destination 'HashHelm.exe')).Length
$assetBytes = (Get-ChildItem -LiteralPath (Join-Path $destination 'assets') -File -Recurse | Measure-Object Length -Sum).Sum
$docsBytes = (Get-Item -LiteralPath (Join-Path $destination 'README.md'),(Join-Path $destination 'THIRD_PARTY.md'),(Join-Path $destination 'LICENSE'),(Join-Path $destination 'GUIDE.zh-CN.md'),(Join-Path $destination 'BUILDING.md'),(Join-Path $destination 'CREDITS.md'),(Join-Path $destination 'FORMAT-SUPPORT.md'),(Join-Path $destination 'SECURITY-NOTES.md'),(Join-Path $destination 'DESIGN.md') | Measure-Object Length -Sum).Sum
$extra = $uiBytes + $assetBytes + $docsBytes
if ($extra -gt 10000000) { throw 'UI exceeds 10 MB size budget' }
$report = [ordered]@{ packageDirectory=$destination; flavor=$flavor; bundledRuntimeBytes=$runtimeBytes; hashcatBytes=$baseline; uiExecutableBytes=$uiBytes; assetBytes=$assetBytes; docsBytes=$docsBytes; totalAddedBytes=$extra; uiBudgetBytes=10000000; hashcatArchiveSha256=(Get-FileHash downloads\hashcat-8.0.0.7z).Hash; bundledHashcatFiles=(Get-ChildItem (Join-Path $destination 'hashcat') -File -Recurse).Count }
if (!(Test-Path -LiteralPath 'downloads\7zr.exe')) {
    Invoke-WebRequest -Uri 'https://www.7-zip.org/a/7zr.exe' -OutFile 'downloads\7zr.exe'
}
& '.\downloads\7zr.exe' a -t7z -mx=7 -ms=on $pendingArchive $destination '-xr!workspace' '-xr!*.WebView2'
if ($LASTEXITCODE -ne 0) { throw '7z packaging failed' }
& '.\downloads\7zr.exe' t $pendingArchive
if ($LASTEXITCODE -ne 0) { throw '7z integrity check failed; previous release was preserved' }
$archive = (Get-Item -LiteralPath $pendingArchive).Length
$originalArchive = (Get-Item -LiteralPath 'downloads\hashcat-8.0.0.7z').Length
if ($Lite -and ($archive - $originalArchive) -gt 10000000) { throw 'Lite archive exceeds the 10 MB overhead budget' }
Move-Item -LiteralPath $pendingArchive -Destination $releaseArchive -Force
$report['archiveBytes']=$archive
$report['version']=$releaseVersion
$report['build']=$packageBuild
$report['sha256']=(Get-FileHash -LiteralPath $releaseArchive -Algorithm SHA256).Hash.ToLowerInvariant()
$report['officialArchiveBytes']=$originalArchive
$report['archiveOverheadBytes']=$archive-$originalArchive
$report['archivePath']=$releaseArchive
$report | ConvertTo-Json | Set-Content -LiteralPath 'dist\size-report.json' -Encoding utf8
$report | ConvertTo-Json | Set-Content -LiteralPath ('dist\size-report-'+$flavor+'.json') -Encoding utf8
$report | ConvertTo-Json
Get-Item -LiteralPath $releaseArchive | Select-Object Name,Length
