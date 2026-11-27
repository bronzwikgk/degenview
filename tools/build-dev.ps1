param(
    [switch]$Run
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$manifest = Join-Path $repoRoot 'Cargo.toml'
$manifestText = Get-Content -LiteralPath $manifest -Raw
$versionMatch = [regex]::Match($manifestText, '(?m)^version\s*=\s*"([^"]+)"')
if (-not $versionMatch.Success) { throw 'Could not read package version from Cargo.toml.' }
$version = $versionMatch.Groups[1].Value

Push-Location $repoRoot
try {
    cargo build
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    $source = Join-Path $repoRoot 'target\debug\degenview_gk.exe'
    if (-not (Test-Path -LiteralPath $source)) { throw "Build succeeded but $source was not created." }
    $outputDir = Join-Path $repoRoot 'dist\dev'
    New-Item -ItemType Directory -Force -Path $outputDir | Out-Null
    $target = Join-Path $outputDir "DegenView-GK-v$version-dev.exe"
    Copy-Item -LiteralPath $source -Destination $target -Force
    Write-Output "Versioned development build: $target"
    if ($Run) { Start-Process -FilePath $target }
} finally {
    Pop-Location
}
