# ASCII-only so Windows PowerShell 5.1 will parse this on any locale.
$ErrorActionPreference = "Continue"

# 1. Prefer Inno Setup native uninstaller if present
$inno_uninst_candidates = @(
    (Join-Path $env:LOCALAPPDATA "Programs\TermShot\unins000.exe"),
    (Join-Path $env:ProgramFiles "TermShot\unins000.exe"),
    (Join-Path $env:LOCALAPPDATA "TermShot\unins000.exe")
)
$ran_inno = $false
foreach ($u in $inno_uninst_candidates) {
    if (Test-Path $u) {
        Write-Host "Running Inno Setup uninstaller: $u"
        Start-Process -FilePath $u -ArgumentList "/SILENT" -Wait
        $ran_inno = $true
        break
    }
}

# 2. Legacy / fallback cleanup
$app = Join-Path $env:LOCALAPPDATA "TermShot\TermShot.exe"
if (Test-Path $app) {
    Start-Process -FilePath $app -ArgumentList "--uninstall" -Wait
}
$dest_legacy = Join-Path $env:LOCALAPPDATA "TermShot"
$dest_programs = Join-Path $env:LOCALAPPDATA "Programs\TermShot"
$lnk_user = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\TermShot.lnk"
$lnk_desktop = Join-Path ([Environment]::GetFolderPath("Desktop")) "TermShot.lnk"

Remove-ItemProperty -Path "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run" -Name "TermShot" -ErrorAction SilentlyContinue
Remove-Item $lnk_user -Force -ErrorAction SilentlyContinue
Remove-Item $lnk_desktop -Force -ErrorAction SilentlyContinue

Get-Process -Name "TermShot" -ErrorAction SilentlyContinue | ForEach-Object {
    try { $_.Kill(); $_.WaitForExit(4000) | Out-Null } catch {}
}
Start-Sleep -Milliseconds 300

if (Test-Path $dest_legacy) {
    Remove-Item $dest_legacy -Recurse -Force -ErrorAction SilentlyContinue
}
if (Test-Path $dest_programs) {
    Remove-Item $dest_programs -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host "TermShot uninstalled."
