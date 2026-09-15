<#
.SYNOPSIS
  Size-optimised release build of pkgr.

.DESCRIPTION
  Runs formatting, lint and test gates, then builds the release binary with
  the size profile from Cargo.toml and reports its size.

  The optimisation settings live in Cargo.toml under [profile.release], so a
  plain `cargo build --release` produces the same binary. This script refuses
  to build if any of them have gone missing, so a stray edit cannot silently
  produce a bloated release.

.EXAMPLE
  ./build.ps1
  ./build.ps1 -SkipChecks
  ./build.ps1 -InstallDir C:\Users\Roy\tools
#>
[CmdletBinding()]
param(
    # Skip the fmt, clippy and test gates and build straight away.
    [switch]$SkipChecks,

    # Copy the finished binary here, e.g. a directory on your PATH.
    [string]$InstallDir
)

$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

function Invoke-Cargo {
    # Cargo writes progress ("Compiling", "Finished") to stderr. When the
    # script's output is redirected, Windows PowerShell 5.1 turns each stderr
    # line into an error record, which "Stop" would make fatal. Success is
    # judged by the exit code instead.
    $ErrorActionPreference = 'Continue'
    & cargo @args
    if ($LASTEXITCODE -ne 0) { throw "cargo $($args -join ' ') failed" }
}

# The release profile is what makes the binary small. Every one of these is
# required; together they take it from 437 KB to 233 KB.
$required = [ordered]@{
    'opt-level'     = '"z"'   # optimise for size rather than speed
    'lto'           = '"fat"' # whole-program optimisation across crates
    'codegen-units' = '1'     # no parallel codegen, so more can be merged away
    'panic'         = '"abort"' # no unwind tables or landing pads
    'strip'         = 'true'  # no symbols, no debug info
}

$cargoToml = Get-Content Cargo.toml -Raw
$releaseProfile = [regex]::Match($cargoToml, '(?ms)^\[profile\.release\]\s*$(.*?)(?=^\[|\z)').Groups[1].Value
$version = [regex]::Match($cargoToml, '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value

$missing = foreach ($key in $required.Keys) {
    $pattern = '(?m)^\s*' + [regex]::Escape($key) + '\s*=\s*' + [regex]::Escape($required[$key]) + '\s*(#.*)?$'
    if ($releaseProfile -notmatch $pattern) { "$key = $($required[$key])" }
}
if ($missing) {
    throw "Cargo.toml [profile.release] is missing required size settings:`n  $($missing -join "`n  ")"
}

Write-Host "pkgr $version - release profile:" -ForegroundColor Cyan
foreach ($key in $required.Keys) { Write-Host ("  {0,-14} {1}" -f $key, $required[$key]) }
Write-Host ""

if (-not $SkipChecks) {
    Write-Host "Checking formatting ..." -ForegroundColor Cyan
    Invoke-Cargo fmt --check

    Write-Host "Linting ..." -ForegroundColor Cyan
    # '--' is quoted because PowerShell otherwise consumes a bare -- as its own
    # end-of-parameters marker, and -D warnings would reach cargo, not clippy.
    Invoke-Cargo clippy --release --all-targets --quiet '--' -D warnings

    Write-Host "Testing ..." -ForegroundColor Cyan
    Invoke-Cargo test --quiet

    # The other platform's picker is behind a #[cfg], so nothing above compiles
    # it. Without this a change here can break Linux silently, and only a build
    # on the other machine would find out.
    $crossTarget = 'x86_64-unknown-linux-gnu'
    if ((rustup target list --installed) -contains $crossTarget) {
        Write-Host "Linting $crossTarget ..." -ForegroundColor Cyan
        Invoke-Cargo clippy --target $crossTarget --all-targets --quiet '--' -D warnings
    }
    else {
        Write-Host "  skipping $crossTarget check - run: rustup target add $crossTarget" -ForegroundColor Cyan
    }
    Write-Host ""
}

Write-Host "Building release ..." -ForegroundColor Cyan
# --locked builds exactly the dependency versions in Cargo.lock, so a release
# never quietly picks up a newer crate.
Invoke-Cargo build --release --locked

$name = if ($env:OS -eq 'Windows_NT') { 'pkgr.exe' } else { 'pkgr' }

# Stable cargo cannot redirect only the final artifact (--artifact-dir is
# nightly), so the release binary is copied out of target/ into bin/.
$binDir = Join-Path $PSScriptRoot 'bin'
New-Item -ItemType Directory -Force -Path $binDir | Out-Null

function Copy-Binary([string]$from, [string]$to) {
    try {
        Copy-Item $from $to -Force
    }
    catch {
        throw "Could not copy to $to - is pkgr currently running? $_"
    }
}

$output = Join-Path $binDir $name
Copy-Binary (Join-Path 'target/release' $name) $output
$binary = Get-Item $output

Write-Host ""
Write-Host ("Built {0}  {1:N0} bytes ({2:N0} KB)" -f $binary.FullName, $binary.Length, ($binary.Length / 1000)) -ForegroundColor Green

if ($InstallDir) {
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    $destination = Join-Path $InstallDir $name
    Copy-Binary $binary.FullName $destination
    Write-Host "Installed to $destination" -ForegroundColor Green
}
