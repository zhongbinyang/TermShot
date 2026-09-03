param(
    [switch]$FrameworkDependent
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $root

dotnet run --project $root --configuration Release -- --write-icon (Join-Path $root "Assets\app.ico")

$out = Join-Path $root "..\dist"
if (Test-Path $out) { Remove-Item $out -Recurse -Force }
New-Item -ItemType Directory -Path $out | Out-Null

$pubArgs = @(
    "publish", $root,
    "-c", "Release",
    "-r", "win-x64",
    "-o", $out,
    "-p:PublishSingleFile=true",
    "-p:IncludeNativeLibrariesForSelfExtract=true",
    "-p:EnableCompressionInSingleFile=true",
    "-p:DebugType=none",
    "-p:DebugSymbols=false"
)

if ($FrameworkDependent) {
    $pubArgs += "--self-contained"
    $pubArgs += "false"
} else {
    $pubArgs += "--self-contained"
    $pubArgs += "true"
}

dotnet @pubArgs

$app = Join-Path $out "TermShot.exe"
$setup = Join-Path $out "TermShot-Setup.exe"
Copy-Item $app $setup -Force
Copy-Item (Join-Path $root "install.ps1") $out -Force
Copy-Item (Join-Path $root "uninstall.ps1") $out -Force

Write-Host ""
Write-Host "Installer: $setup"
Write-Host "App: $app"
