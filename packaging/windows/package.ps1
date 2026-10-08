<#
.SYNOPSIS
  Build the Windows installer (.msi) and a portable .zip for Invoices.

.DESCRIPTION
  1. cargo build --release for x86_64-pc-windows-msvc, with the C runtime
     linked statically so the .exe files run on any Windows 10/11 PC
     without the Visual C++ redistributable. SQLite is compiled into the
     .exe (rusqlite "bundled"), so nothing else needs installing either.
  2. Stage invoices.exe, invoices-import.exe and README.md.
  3. wix build -> dist\Invoices-<version>-x64.msi
  4. Compress-Archive -> dist\Invoices-<version>-x64-portable.zip

  Needs: Rust (MSVC toolchain), and WiX v5:
    dotnet tool install --global wix --version 5.0.2

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File packaging\windows\package.ps1
#>
param(
    # Reuse the last release build instead of building again.
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$Root = Resolve-Path (Join-Path $PSScriptRoot '..\..')
$Target = 'x86_64-pc-windows-msvc'

function Invoke-Native([string]$What, [scriptblock]$Command) {
    & $Command
    if ($LASTEXITCODE -ne 0) { throw "$What failed (exit code $LASTEXITCODE)" }
}

Push-Location $Root
try {
    $Version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"' |
        Select-Object -First 1).Matches[0].Groups[1].Value
    Write-Host "Packaging Invoices $Version"

    if (-not (Get-Command wix -ErrorAction SilentlyContinue)) {
        throw "WiX was not found. Install it with: dotnet tool install --global wix --version 5.0.2"
    }

    if (-not $SkipBuild) {
        # Static CRT for the app only (not build scripts), via --target.
        $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = '-C target-feature=+crt-static'
        Invoke-Native 'cargo build' { cargo build --release --locked --target $Target }
    }

    $Bin = Join-Path $Root "target\$Target\release"
    $Dist = Join-Path $Root 'dist'
    $Stage = Join-Path $Dist 'stage'
    if (Test-Path $Stage) { Remove-Item -Recurse -Force $Stage }
    New-Item -ItemType Directory -Force $Stage | Out-Null
    Copy-Item (Join-Path $Bin 'invoices.exe'), (Join-Path $Bin 'invoices-import.exe'), 'README.md' $Stage

    $Msi = Join-Path $Dist "Invoices-$Version-x64.msi"
    Invoke-Native 'wix build' {
        wix build (Join-Path $PSScriptRoot 'invoices.wxs') -arch x64 `
            -d "Version=$Version" -d "BinDir=$Stage" -d "IconPath=$(Join-Path $Root 'assets\icon.ico')" `
            -o $Msi
    }

    $Zip = Join-Path $Dist "Invoices-$Version-x64-portable.zip"
    if (Test-Path $Zip) { Remove-Item -Force $Zip }
    Compress-Archive -Path (Join-Path $Stage '*') -DestinationPath $Zip

    Write-Host ""
    Write-Host "Installer: $Msi"
    Write-Host "Portable:  $Zip"
}
finally {
    Pop-Location
}
