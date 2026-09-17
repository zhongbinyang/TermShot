# ASCII-only so Windows PowerShell 5.1 will parse this on any locale.
$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$setup = Join-Path $here "TermShot-Setup.exe"
$app = Join-Path $here "TermShot.exe"
if (Test-Path $setup) {
    Start-Process -FilePath $setup
    return
}
if (Test-Path $app) {
    Start-Process -FilePath $app -ArgumentList "--install"
    return
}
throw "TermShot-Setup.exe not found. Run build.ps1 first."
