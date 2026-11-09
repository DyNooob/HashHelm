$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)
cargo build --release --locked --bin hashhelm
if ($LASTEXITCODE -ne 0) { throw 'Rust build failed' }
$releaseVersion = (Select-String -LiteralPath 'Cargo.toml' -Pattern '^version = "([^"]+)"' | Select-Object -First 1).Matches.Groups[1].Value
$destination = Join-Path (Get-Location) ('dist\HashHelm-' + $releaseVersion)
$releaseArchive = 'dist\HashHelm-' + $releaseVersion + '-windows-x64.7z'
New-Item -ItemType Directory -Path $destination -Force | Out-Null
Copy-Item -LiteralPath 'target\release\hashhelm.exe' -Destination (Join-Path $destination 'HashHelm.exe') -Force
if (!(Test-Path -LiteralPath (Join-Path $destination 'hashcat'))) {
    tar -xf downloads\hashcat-8.0.0.7z -C $destination
    if ($LASTEXITCODE -ne 0) { throw 'Hashcat extraction failed' }
    for($hcRetry=0;$hcRetry -lt 3;$hcRetry++){
        try{Rename-Item -LiteralPath (Join-Path $destination 'hashcat-8.0.0') -NewName 'hashcat';break}catch{if($hcRetry -eq 2){throw};Start-Sleep -Milliseconds 250}
    }
}
Copy-Item -LiteralPath 'README.md','GUIDE.zh-CN.md','BUILDING.md','THIRD_PARTY.md','LICENSE','CREDITS.md' -Destination $destination -Force
Copy-Item -LiteralPath 'assets' -Destination $destination -Recurse -Force
$engineCopy = (Get-ChildItem -LiteralPath (Join-Path $destination 'hashcat') -File -Recurse | Measure-Object Length -Sum).Sum
$baseline = $engineCopy
$uiBytes = (Get-Item -LiteralPath (Join-Path $destination 'HashHelm.exe')).Length
$assetBytes = (Get-ChildItem -LiteralPath (Join-Path $destination 'assets') -File -Recurse | Measure-Object Length -Sum).Sum
$docsBytes = (Get-Item -LiteralPath (Join-Path $destination 'README.md'),(Join-Path $destination 'THIRD_PARTY.md'),(Join-Path $destination 'LICENSE'),(Join-Path $destination 'GUIDE.zh-CN.md'),(Join-Path $destination 'BUILDING.md'),(Join-Path $destination 'CREDITS.md') | Measure-Object Length -Sum).Sum
$extra = $uiBytes + $assetBytes + $docsBytes
if ($extra -gt 10000000) { throw 'UI exceeds 10 MB size budget' }
$report = [ordered]@{ hashcatBytes=$baseline; uiExecutableBytes=$uiBytes; assetBytes=$assetBytes; docsBytes=$docsBytes; totalAddedBytes=$extra; uiBudgetBytes=10000000; hashcatArchiveSha256=(Get-FileHash downloads\hashcat-8.0.0.7z).Hash; bundledHashcatFiles=(Get-ChildItem (Join-Path $destination 'hashcat') -File -Recurse).Count }
$report | ConvertTo-Json | Set-Content -LiteralPath 'dist\size-report.json' -Encoding utf8
if (!(Test-Path -LiteralPath 'downloads\7zr.exe')) {
    Invoke-WebRequest -Uri 'https://www.7-zip.org/a/7zr.exe' -OutFile 'downloads\7zr.exe'
}
& '.\downloads\7zr.exe' a -t7z -mx=7 -ms=on $releaseArchive $destination '-xr!workspace' '-xr!*.WebView2'
if ($LASTEXITCODE -ne 0) { throw '7z packaging failed' }
$archive = (Get-Item -LiteralPath $releaseArchive).Length
$originalArchive = (Get-Item -LiteralPath 'downloads\hashcat-8.0.0.7z').Length
if (($archive - $originalArchive) -gt 10000000) { throw 'Archive exceeds the 10 MB overhead budget' }
$report['archiveBytes']=$archive
$report['officialArchiveBytes']=$originalArchive
$report['archiveOverheadBytes']=$archive-$originalArchive
$report | ConvertTo-Json | Set-Content -LiteralPath 'dist\size-report.json' -Encoding utf8
$report | ConvertTo-Json
Get-Item -LiteralPath $releaseArchive | Select-Object Name,Length
