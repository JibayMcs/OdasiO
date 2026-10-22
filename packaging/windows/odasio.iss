; Installateur Windows d'OdasiO (Inno Setup 6).
; Compilé par la CI : iscc /DAppVersion=x.y.z /DSourceDir=<dossier des .exe> odasio.iss
;
; Installation par utilisateur (sans droits administrateur), pour que les mises à jour
; intégrées puissent relancer l'installateur en silence (/SILENT) sans demande d'élévation.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceDir
  #define SourceDir "..\..\target\release"
#endif

[Setup]
AppId={{6F1C2B4E-8D3A-4E5F-9B7C-0D1A2E3F4B5C}
AppName=OdasiO
AppVersion={#AppVersion}
AppVerName=OdasiO {#AppVersion}
AppPublisher=JibayMcs
AppPublisherURL=https://github.com/JibayMcs/OdasiO
AppSupportURL=https://github.com/JibayMcs/OdasiO/issues
AppUpdatesURL=https://github.com/JibayMcs/OdasiO/releases
DefaultDirName={localappdata}\Programs\OdasiO
DefaultGroupName=OdasiO
DisableProgramGroupPage=yes
DisableDirPage=auto
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\..\dist
OutputBaseFilename=OdasiO-{#AppVersion}-windows-x64-setup
SetupIconFile=..\icons\windows\odasio.ico
UninstallDisplayIcon={app}\odasio-gui.exe
LicenseFile=..\..\LICENSE-MIT
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
; Ferme OdasiO pendant une mise à jour ; [Run] le relance ensuite.
CloseApplications=yes
ChangesEnvironment=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "french"; MessagesFile: "compiler:Languages\French.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "addtopath"; Description: "{cm:AddToPath}"; GroupDescription: "{cm:CommandLine}"

[CustomMessages]
english.AddToPath=Add the odasio command to the PATH
french.AddToPath=Ajouter la commande odasio au PATH
english.CommandLine=Command line:
french.CommandLine=Ligne de commande :

[Files]
Source: "{#SourceDir}\odasio-gui.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\odasio.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE-MIT"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE-APACHE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\OdasiO"; Filename: "{app}\odasio-gui.exe"
Name: "{group}\{cm:UninstallProgram,OdasiO}"; Filename: "{uninstallexe}"
Name: "{userdesktop}\OdasiO"; Filename: "{app}\odasio-gui.exe"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Environment"; ValueType: expandsz; ValueName: "Path"; ValueData: "{olddata};{app}"; Tasks: addtopath; Check: NeedsAddPath(ExpandConstant('{app}'))

[Run]
; Lancé aussi en mode silencieux : c'est ce qui relance OdasiO après une mise à jour.
Filename: "{app}\odasio-gui.exe"; Description: "{cm:LaunchProgram,OdasiO}"; Flags: nowait postinstall

[Code]
function NeedsAddPath(Dir: string): Boolean;
var
  Path: string;
begin
  if not RegQueryStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', Path) then
  begin
    Result := True;
    exit;
  end;
  Result := Pos(';' + Uppercase(Dir) + ';', ';' + Uppercase(Path) + ';') = 0;
end;
