# Builds the Store variant itself so an NSIS binary can never be staged accidentally.
# Run with Windows PowerShell 5.1 or PowerShell 7, from any working directory.
[CmdletBinding()]
param([string]$CertificateThumbprint)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Invoke-Checked {
    param([string]$Program, [string[]]$Arguments)
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Program failed with exit code $LASTEXITCODE" }
}

$root = Split-Path $PSScriptRoot -Parent
Push-Location $root
try {
    $version = & bash scripts/app-version.sh
    if ($LASTEXITCODE -ne 0) { throw 'Cannot read Cargo workspace version; install Git for Windows (bash).' }
    if ($version -cnotmatch '^([1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$') {
        throw "Store builds require a stable X.Y.Z version with major >= 1, got $version"
    }
    foreach ($part in $version.Split('.')) {
        if ([long]$part -gt 65535) { throw 'MSIX version components must be <= 65535.' }
    }
    $packageVersion = "$version.0"
    $sdkRoot = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits/10/bin'
    $sdk = Get-ChildItem $sdkRoot -Directory | Where-Object {
        $_.Name -match '^10\.0\.\d+\.\d+$' -and
        (Test-Path (Join-Path $_.FullName 'x64/makeappx.exe')) -and
        (Test-Path (Join-Path $_.FullName 'x64/makepri.exe'))
    } | Sort-Object { [version]$_.Name } -Descending | Select-Object -First 1
    if ($null -eq $sdk) { throw 'Install Windows SDK with MakeAppx and MakePri, then retry.' }
    $tools = Join-Path $sdk.FullName 'x64'

    Invoke-Checked bash @('scripts/fetch-uv.sh', 'x86_64-pc-windows-msvc')
    Push-Location shell
    try {
        Invoke-Checked bun @('run', 'build:app', '--', '--ci', '--no-bundle', '--features', 'no-self-update', '--', '--locked')
    } finally { Pop-Location }

    $output = Join-Path $root 'target/msix'
    $stage = Join-Path $output 'stage'
    if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
    New-Item $stage -ItemType Directory -Force | Out-Null
    $release = Join-Path $root 'target/release'
    foreach ($file in @('smabar.exe', 'uv.exe')) {
        Copy-Item (Join-Path $release $file) $stage
    }
    # Resolve only configured resources. Never copy target/release recursively:
    # it contains other tools, stale bundles, and both binary variants.
    $configDir = Join-Path $root 'crates/smabar'
    $config = Get-Content (Join-Path $configDir 'tauri.conf.json') -Raw -Encoding utf8 | ConvertFrom-Json
    foreach ($resource in $config.bundle.resources.PSObject.Properties) {
        $sources = @(Get-Item (Join-Path $configDir $resource.Name))
        if ($sources.Count -eq 0) { throw "Resource pattern has no files: $($resource.Name)" }
        foreach ($source in $sources) {
            $relative = $resource.Value
            if ($relative.EndsWith('/')) { $relative += $source.Name }
            $destination = Join-Path $stage $relative
            New-Item (Split-Path $destination -Parent) -ItemType Directory -Force | Out-Null
            Copy-Item (Join-Path $release $relative) $destination
        }
    }
    Copy-Item (Join-Path $configDir 'msix/Assets') $stage -Recurse
    $manifest = Get-Content (Join-Path $configDir 'msix/AppxManifest.xml') -Raw -Encoding utf8
    $manifest.Replace('__VERSION__', $packageVersion) | Set-Content (Join-Path $stage 'AppxManifest.xml') -Encoding utf8
    $priConfig = Join-Path $output 'priconfig.xml'
    Invoke-Checked (Join-Path $tools 'makepri.exe') @('createconfig', '/cf', $priConfig, '/dq', 'en-US', '/o')
    # One MSIX carries every scale/language; do not generate separate resource packs.
    [xml]$pri = Get-Content $priConfig -Raw
    $pri.resources.RemoveChild($pri.resources.packaging) | Out-Null
    $pri.Save($priConfig)
    Invoke-Checked (Join-Path $tools 'makepri.exe') @('new', '/pr', $stage, '/cf', $priConfig, '/of', (Join-Path $stage 'resources.pri'), '/o')
    $package = Join-Path $output "smabar_${packageVersion}_x64.msix"
    Invoke-Checked (Join-Path $tools 'makeappx.exe') @('pack', '/o', '/d', $stage, '/p', $package)
    if ($CertificateThumbprint) {
        # Local test signing only; the private key stays in the user's certificate store.
        Invoke-Checked (Join-Path $tools 'signtool.exe') @('sign', '/fd', 'SHA256', '/sha1', $CertificateThumbprint, $package)
    }
    Write-Output $package
} finally { Pop-Location }
