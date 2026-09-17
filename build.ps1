param(
    [switch]$FrameworkDependent
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $root

cargo run --release -- --write-icon (Join-Path $root "assets\app.ico")

$out = Join-Path $root "dist"
if (Test-Path $out) { Remove-Item $out -Recurse -Force }
New-Item -ItemType Directory -Path $out | Out-Null

cargo build --release
Copy-Item (Join-Path $root "target\release\TermShot.exe") (Join-Path $out "TermShot.exe") -Force

# Locate Inno Setup Command-Line Compiler (ISCC.exe)
$iscc = Get-Command "iscc.exe" -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source -ErrorAction SilentlyContinue
if (-not $iscc) {
    $candidates = @(
        "$env:LocalAppData\Programs\Inno Setup 6\ISCC.exe",
        "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
        "$env:ProgramFiles\Inno Setup 6\ISCC.exe"
    )
    foreach ($c in $candidates) {
        if (Test-Path $c) { $iscc = $c; break }
    }
}

if ($iscc) {
    Write-Host "Compiling installer with Inno Setup: $iscc..."
    & $iscc (Join-Path $root "installer.iss")
} else {
    Write-Warning "ISCC.exe not found! Please install Inno Setup 6 to generate installer."
}

Copy-Item (Join-Path $root "install.ps1") $out -Force
Copy-Item (Join-Path $root "uninstall.ps1") $out -Force

Write-Host ""
if (Test-Path (Join-Path $out 'TermShot-Setup.exe')) {
    Write-Host "Inno Setup Installer: $(Join-Path $out 'TermShot-Setup.exe')"
}
Write-Host "Portable App: $(Join-Path $out 'TermShot.exe')"
