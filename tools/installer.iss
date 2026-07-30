; The Windows installer for Red Republic.
;
; Not run directly -- `tools/package.ps1` compiles this, passing the version and
; the staged export in. The three defines below are required, and the fallbacks
; exist only so that a bare `ISCC tools/installer.iss` fails with a message about
; what is missing rather than a parse error twenty lines down.
;
; # Why a per-user install
;
; PrivilegesRequired=lowest, into {localappdata}\Programs. A game is not system
; software: installing it needs no administrator, and asking for elevation is
; both a worse first thirty seconds for a stranger and a UAC prompt that cannot
; be answered by the CI job that checks a clean-machine install actually works.
; The cost is that it installs for the current user only, which for a game is the
; behaviour anybody would have expected anyway.
;
; # What uninstalling does not remove
;
; Saved republics and settings live under the user's own application data and are
; deliberately left behind. Uninstalling a game and losing a decade-long republic
; is not a trade anybody agreed to, and reinstalling should find it still there.

#ifndef AppVersion
  #error AppVersion is not defined -- run tools/package.ps1 rather than ISCC directly
#endif
#ifndef StageDir
  #error StageDir is not defined -- run tools/package.ps1 rather than ISCC directly
#endif
#ifndef OutDir
  #error OutDir is not defined -- run tools/package.ps1 rather than ISCC directly
#endif
#ifndef RootDir
  #error RootDir is not defined -- run tools/package.ps1 rather than ISCC directly
#endif

#define AppName "Red Republic"
#define Publisher "Noah Sabaj"
#define ExeName "RedRepublic.exe"

[Setup]
; Fixed for the life of the application. This is what tells Windows that
; 2026.8.0 is an upgrade of 2026.7.0 rather than a second program, so it must
; never be regenerated -- changing it would leave a player with two entries in
; Apps and Features and two copies on disk.
AppId={{6765B1FA-C9BB-404B-9BF9-7DD8EE6649BB}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#Publisher}
VersionInfoVersion={#AppVersion}

DefaultDirName={localappdata}\Programs\{#AppName}
DefaultGroupName={#AppName}
; There is nothing to configure: one program, one folder, no components. Every
; page this skips is a page a stranger would have clicked Next on without
; reading.
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible

LicenseFile={#RootDir}\LICENSE
SetupIconFile={#RootDir}\godot\icon.ico
UninstallDisplayIcon={app}\{#ExeName}
UninstallDisplayName={#AppName} {#AppVersion}

OutputDir={#OutDir}
OutputBaseFilename=RedRepublic-{#AppVersion}-setup
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Shortcuts:"

[Files]
; The whole staged export. `package.ps1` has already asserted that the executable,
; the console wrapper and red_republic_shell.dll are all in there -- a pack
; missing the cdylib is a game that starts and has no simulation behind it.
Source: "{#StageDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
; The windowed executable, not the console wrapper. The wrapper is installed
; beside it on purpose -- it is how a player produces a log for a bug report --
; but a shortcut to it would open a console window nobody asked for.
Name: "{group}\{#AppName}"; Filename: "{app}\{#ExeName}"
Name: "{group}\Uninstall {#AppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#ExeName}"; Tasks: desktopicon

[Run]
Description: "Launch {#AppName}"; Filename: "{app}\{#ExeName}"; Flags: nowait postinstall skipifsilent
