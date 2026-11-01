#ifndef AppVersion
  #error AppVersion is required
#endif
[Setup]
AppId={{2AB6E03E-2E63-4C9C-9AC5-E6E357F74BB2}
AppName=OpenFader
AppVersion={#AppVersion}
DefaultDirName={localappdata}\Programs\OpenFader
DefaultGroupName=OpenFader
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\artifacts
OutputBaseFilename=OpenFader-{#AppVersion}-windows-x64-setup
LicenseFile=..\LICENSE
Compression=lzma2
SolidCompression=yes
; Never let Restart Manager terminate the recorder independently of the UI.
CloseApplications=no
RestartApplications=no
UninstallDisplayIcon={app}\openfader.exe
[Files]
Source: "..\app\build\windows\x64\runner\Release\openfader-update-helper.exe"; Flags: dontcopy noencryption
Source: "..\app\build\windows\x64\runner\Release\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
[Icons]
Name: "{group}\OpenFader"; Filename: "{app}\openfader.exe"

[Run]
Filename: "{app}\openfader.exe"; Description: "Open OpenFader"; Flags: nowait postinstall skipifsilent; Check: WasRunning

[Code]
var
  RestartOpenFader: Boolean;

function WasRunning: Boolean;
begin
  Result := RestartOpenFader;
end;

const
  GENERIC_READ = $80000000;
  OPEN_EXISTING = 3;
  INVALID_HANDLE_VALUE = -1;

function CreateFileW(lpFileName: String; dwDesiredAccess, dwShareMode: Cardinal;
  lpSecurityAttributes: Longint; dwCreationDisposition, dwFlagsAndAttributes: Cardinal;
  hTemplateFile: THandle): THandle;
  external 'CreateFileW@kernel32.dll stdcall';
function CloseHandle(hObject: THandle): Boolean;
  external 'CloseHandle@kernel32.dll stdcall';

function FileIsAvailable(const Path: String): Boolean;
var
  Handle: THandle;
begin
  if not FileExists(Path) then begin
    Result := True;
    exit;
  end;
  Handle := CreateFileW(Path, GENERIC_READ, 0, 0, OPEN_EXISTING, 0, 0);
  Result := Handle <> INVALID_HANDLE_VALUE;
  if Result then CloseHandle(Handle);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  ExitCode: Integer;
begin
  Result := '';
  RestartOpenFader := False;
  ExtractTemporaryFile('openfader-update-helper.exe');
  if not Exec(ExpandConstant('{tmp}\openfader-update-helper.exe'),
      '"' + ExpandConstant('{app}') + '"', '', SW_HIDE, ewWaitUntilTerminated, ExitCode) then begin
    Result := 'Setup could not check whether OpenFader has shut down. Please quit OpenFader and try again.';
    exit;
  end;
  if ExitCode = 10 then RestartOpenFader := True
  else if ExitCode <> 0 then begin
    Result := 'OpenFader did not complete its safe shutdown. If prompted, finish or stop the recording and confirm Quit. Older versions require you to choose Quit from the system tray yourself. Once OpenFader has closed, retry Setup.';
    exit;
  end;
  if not FileIsAvailable(ExpandConstant('{app}\openfader.exe')) or
     not FileIsAvailable(ExpandConstant('{app}\openfader-serve.exe')) then
    Result := 'OpenFader is still running. Open it from the system tray and choose Quit, then run Setup again. If a recording is active, finish or stop it when prompted.';
end;
