#define MyAppName "TermShot"
#define MyAppVersion "1.0.0"
#define MyAppPublisher "TermShot"
#define MyAppURL "https://github.com/TermShot/TermShot"
#define MyAppExeName "TermShot.exe"

[Setup]
; AppId uniquely identifies this application. Do not use the same AppId in other installers.
AppId={{F92E5E4E-34C1-4D02-861F-CA6A76E84C7B}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
OutputDir=dist
OutputBaseFilename=TermShot-Setup
SetupIconFile=assets\app.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
WizardSizePercent=100
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
CloseApplications=force
RestartApplications=no
ArchitecturesInstallIn64BitMode=x64compatible

[Languages]
Name: "chinesesimp"; MessagesFile: "compiler:Languages\ChineseSimplified.isl"
Name: "en"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "autostart"; Description: "开机自动启动 TermShot"; GroupDescription: "系统集成:"; Flags: checkedonce

[Files]
Source: "target\release\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "assets\app.ico"; DestDir: "{app}\assets"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Registry]
; If autostart task is checked, write HKCU Run key
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "{#MyAppName}"; ValueData: """{app}\{#MyAppExeName}"""; Flags: uninsdeletevalue; Tasks: autostart
; Always remove Run entry on uninstall even if enabled in-app later
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueName: "{#MyAppName}"; Flags: dontcreatekey uninsdeletevalue

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent

[Code]
function InitializeSetup(): Boolean;
var
  ErrorCode: Integer;
begin
  // Terminate any currently running instance of TermShot before installing or upgrading
  ShellExec('open', 'taskkill.exe', '/F /IM TermShot.exe', '', SW_HIDE, ewWaitUntilTerminated, ErrorCode);
  Result := True;
end;

function InitializeUninstall(): Boolean;
var
  ErrorCode: Integer;
begin
  // Terminate any currently running instance of TermShot before uninstalling
  ShellExec('open', 'taskkill.exe', '/F /IM TermShot.exe', '', SW_HIDE, ewWaitUntilTerminated, ErrorCode);
  Result := True;
end;
