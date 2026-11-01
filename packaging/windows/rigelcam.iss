; Built by the release workflow, with the folder from bundle.ps1 at the root of the repository and
; the version in /DVersion.
#define Root "..\.."

[Setup]
; Identifies the app across versions, so it can't change.
AppId={{6F1C2E0A-7B7E-4C53-9A2B-3E0D5C1A8F42}
AppName=Rigelcam
AppVersion={#Version}
AppPublisher=Eduardo Hillesheim
AppPublisherURL=https://github.com/eduardoghi/rigelcam
DefaultDirName={autopf}\Rigelcam
DisableProgramGroupPage=yes
; No administrator needed, which also lets an update run without a prompt.
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#Root}
OutputBaseFilename=rigelcam-{#Version}-windows-x64-setup
SetupIconFile={#Root}\assets\branding\rigelcam.ico
UninstallDisplayIcon={app}\rigelcam.ico
LicenseFile={#Root}\LICENSE
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "brazilianportuguese"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

; An update replaces the folders, so a DLL dropped from a version doesn't stay behind.
[InstallDelete]
Type: filesandordirs; Name: "{app}\bin"
Type: filesandordirs; Name: "{app}\lib"

[Files]
Source: "{#Root}\rigelcam\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "{#Root}\assets\branding\rigelcam.ico"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Rigelcam"; Filename: "{app}\bin\rigelcam.exe"; IconFilename: "{app}\rigelcam.ico"
Name: "{autodesktop}\Rigelcam"; Filename: "{app}\bin\rigelcam.exe"; IconFilename: "{app}\rigelcam.ico"; Tasks: desktopicon

[Run]
Filename: "{app}\bin\rigelcam.exe"; Description: "{cm:LaunchProgram,Rigelcam}"; Flags: nowait postinstall skipifsilent
