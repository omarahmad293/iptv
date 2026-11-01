[CmdletBinding()]
param(
    [string]$DestinationDirectory = (Join-Path $PSScriptRoot 'libs\mpv'),
    [switch]$Force
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$release = '20261101'
$asset = 'mpv-dev-x86_64-20261101-git-48fca90e8c.7z'
$expectedSha256 = '30d3f9fa121ce24638610a9ae52f8957743f9dea416dd37dc4ff7e43589ef843'
$url = "https://github.com/shinchiro/mpv-winbuild-cmake/releases/download/$release/$asset"
$destination = Join-Path $DestinationDirectory 'libmpv-2.dll'

if ((Test-Path -LiteralPath $destination) -and -not $Force) {
    throw "A DLL already exists at '$destination'. Use -Force to replace it with the pinned version."
}

$extractor = Get-Command 7z.exe -ErrorAction SilentlyContinue
if (-not $extractor -and $env:ProgramFiles) {
    $sevenZip = Join-Path $env:ProgramFiles '7-Zip\7z.exe'
    if (Test-Path -LiteralPath $sevenZip) {
        $extractor = Get-Command $sevenZip
    }
}
if (-not $extractor) {
    $extractor = Get-Command tar.exe -ErrorAction SilentlyContinue
}
if (-not $extractor) {
    throw 'Install 7-Zip or use a Windows installation with tar.exe available on PATH.'
}

$temporaryDirectory = Join-Path ([System.IO.Path]::GetTempPath()) ([guid]::NewGuid().ToString())
$previousSecurityProtocol = [Net.ServicePointManager]::SecurityProtocol
try {
    [Net.ServicePointManager]::SecurityProtocol = $previousSecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    New-Item -ItemType Directory -Path $temporaryDirectory | Out-Null
    $archive = Join-Path $temporaryDirectory $asset
    $extractedDirectory = Join-Path $temporaryDirectory 'extracted'
    New-Item -ItemType Directory -Path $extractedDirectory | Out-Null

    Write-Host "Downloading $asset ..."
    Invoke-WebRequest -Uri $url -OutFile $archive -UseBasicParsing
    $actualSha256 = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash
    if ($actualSha256 -ne $expectedSha256) {
        throw "Archive SHA-256 mismatch: expected $expectedSha256, got $actualSha256. No DLL was installed."
    }

    if ($extractor.Name -eq '7z.exe') {
        & $extractor.Source x $archive "-o$extractedDirectory" -y | Out-Null
    } else {
        & $extractor.Source -xf $archive -C $extractedDirectory
    }
    if ($LASTEXITCODE -ne 0) {
        throw "Archive extraction failed (exit code $LASTEXITCODE). Install 7-Zip if tar cannot read this archive."
    }

    $dlls = @(Get-ChildItem -LiteralPath $extractedDirectory -Filter 'libmpv-2.dll' -File -Recurse)
    if ($dlls.Count -ne 1) {
        throw "Expected exactly one libmpv-2.dll in the archive, found $($dlls.Count)."
    }

    New-Item -ItemType Directory -Path $DestinationDirectory -Force | Out-Null
    Copy-Item -LiteralPath $dlls[0].FullName -Destination $destination -Force:$Force
    Write-Host "Installed pinned x86-64 libmpv ($release, git-48fca90e8c) to $destination"
} finally {
    [Net.ServicePointManager]::SecurityProtocol = $previousSecurityProtocol
    if (Test-Path -LiteralPath $temporaryDirectory) {
        Remove-Item -LiteralPath $temporaryDirectory -Recurse -Force
    }
}