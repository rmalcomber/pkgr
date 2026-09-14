<#
.SYNOPSIS
  Release build for both pkgr ports.

.EXAMPLE
  ./build.ps1                    # both, into bin/
  ./build.ps1 -Port rust
  ./build.ps1 -Version 1.0.0
#>
[CmdletBinding()]
param(
    [ValidateSet("both", "go", "rust")]
    [string]$Port = "both",

    # Stamped into the binary and reported by `pkgr -version`.
    [string]$Version = "0.1.0",

    [string]$OutDir = "bin"
)

$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$results = @()

if ($Port -in @("both", "go")) {
    Write-Host "Building Go port ..."
    Push-Location go
    try {
        $env:CGO_ENABLED = "0"
        # -s -w drops symbols and DWARF; -trimpath strips local paths and makes
        # the build reproducible.
        go build -trimpath -ldflags "-s -w -X main.version=$Version" -o "../$OutDir/pkgr-go.exe" .
        if ($LASTEXITCODE -ne 0) { throw "go build failed" }
    }
    finally { Pop-Location }
    $results += [pscustomobject]@{
        Port = "Go"
        Path = "$OutDir/pkgr-go.exe"
        Bytes = (Get-Item "$OutDir/pkgr-go.exe").Length
    }
}

if ($Port -in @("both", "rust")) {
    Write-Host "Building Rust port ..."
    Push-Location rust
    try {
        # Size settings live in Cargo.toml's [profile.release].
        cargo build --release --quiet
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
        Copy-Item "target/release/pkgr.exe" "../$OutDir/pkgr-rust.exe" -Force
    }
    finally { Pop-Location }
    $results += [pscustomobject]@{
        Port = "Rust"
        Path = "$OutDir/pkgr-rust.exe"
        Bytes = (Get-Item "$OutDir/pkgr-rust.exe").Length
    }
}

Write-Host ""
$results |
    Select-Object Port,
                  Path,
                  @{ Name = "Size"; Expression = { "{0:N0} bytes ({1:N2} MB)" -f $_.Bytes, ($_.Bytes / 1MB) } } |
    Format-Table -AutoSize
