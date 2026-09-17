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
Copy-Item (Join-Path $out "TermShot.exe") (Join-Path $out "TermShot-Setup.exe") -Force
Copy-Item (Join-Path $root "install.ps1") $out -Force
Copy-Item (Join-Path $root "uninstall.ps1") $out -Force

Write-Host ""
Write-Host "Installer: $(Join-Path $out 'TermShot-Setup.exe')"
Write-Host "App: $(Join-Path $out 'TermShot.exe')"
