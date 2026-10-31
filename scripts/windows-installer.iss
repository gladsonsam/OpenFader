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
UninstallDisplayIcon={app}\openfader.exe
[Files]
Source: "..\app\build\windows\x64\runner\Release\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
[Icons]
Name: "{group}\OpenFader"; Filename: "{app}\openfader.exe"
