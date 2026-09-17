# ASCII-only so Windows PowerShell 5.1 will parse this on any locale.
$ErrorActionPreference = "Continue"
$app = Join-Path $env:LOCALAPPDATA "TermShot\TermShot.exe"
if (Test-Path $app) {
    Start-Process -FilePath $app -ArgumentList "--uninstall" -Wait
}
$dest = Join-Path $env:LOCALAPPDATA "TermShot"
$lnk = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\TermShot.lnk"
Remove-ItemProperty -Path "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run" -Name "TermShot" -ErrorAction SilentlyContinue
Remove-Item $lnk -Force -ErrorAction SilentlyContinue
Get-Process -Name "TermShot" -ErrorAction SilentlyContinue | ForEach-Object {
    try { $_.Kill(); $_.WaitForExit(4000) | Out-Null } catch {}
}
Start-Sleep -Milliseconds 300
if (Test-Path $dest) {
    Remove-Item $dest -Recurse -Force -ErrorAction SilentlyContinue
}
if (Test-Path $dest) {
    Write-Host "Some files remain: $dest"
} else {
    Write-Host "TermShot uninstalled."
}
