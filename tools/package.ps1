<#
.SYNOPSIS
    Build the Windows installer for Red Republic.

.DESCRIPTION
    The whole artifact, from a clean checkout to a setup executable:

      1. build the release cdylib
      2. generate godot/export_presets.cfg, with the version stamped in
      3. import the Godot project and export it
      4. run the exported build's own --check, and assert it is the shipped one
      5. wrap the result in an Inno Setup installer

    Step 4 is the reason this is a script rather than a documented sequence of
    commands. Every difference between a development build and a shipped one --
    optimisation, vsync, the absence of debug prints -- is a cargo profile or a
    project setting that can silently fail to apply, and an export that produced
    a working *development* build would look exactly like success. So the export
    is run and interrogated about which artifact it is, and a mismatch fails the
    packaging rather than shipping.

    There is one version number in this repository: workspace.package.version in
    the root Cargo.toml. It reaches the menu through the loaded binary
    (crates/red-republic-shell/src/build_info.rs), and it reaches the executable's
    version resource and the installer from `cargo metadata` here. The export
    preset is generated rather than committed for exactly that reason -- see the
    note in .gitignore.

.PARAMETER Godot
    The Godot executable. Defaults to $env:GODOT, then `godot` on PATH, then the
    Steam install. CI passes its own pinned download.

.PARAMETER Iscc
    Inno Setup's compiler. Defaults to $env:ISCC, then PATH, then the usual
    install locations.

.PARAMETER SkipBuild
    Skip `cargo build --release`. For iterating on the export or the installer
    when the cdylib has not changed.

.PARAMETER NoInstaller
    Export and smoke-test, but stop before Inno Setup. For checking the export on
    a machine with no Inno Setup installed.

.EXAMPLE
    pwsh tools/package.ps1
#>
[CmdletBinding()]
param(
    [string]$Godot,
    [string]$Iscc,
    [switch]$SkipBuild,
    [switch]$NoInstaller
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$GodotDir = Join-Path $Root 'godot'
$Dist = Join-Path $Root 'dist'

function Step($text) { Write-Host "==> $text" -ForegroundColor Cyan }
function Die($text) { Write-Host "packaging failed: $text" -ForegroundColor Red; exit 1 }

# ---- what version is this -----------------------------------------------------

Step 'reading the version from cargo'
$meta = cargo metadata --format-version 1 --no-deps --manifest-path (Join-Path $Root 'Cargo.toml') | ConvertFrom-Json
$pkg = $meta.packages | Where-Object { $_.name -eq 'red-republic-shell' }
if (-not $pkg) { Die 'red-republic-shell is not in the workspace metadata' }
$Version = $pkg.version
# A Windows version resource is four comma-separated integers and the CalVer is
# three. `the_version_is_three_plain_numbers` in build_info.rs is what stops a
# pre-release suffix reaching this line and failing the export with a message
# about the preset rather than about the version. A zero-padded month never gets
# this far either, but for a different reason: Cargo refuses the manifest.
$Version4 = "$Version.0"
Write-Host "    version $Version"

# ---- locate the tools ---------------------------------------------------------

if (-not $Godot) {
    $Godot = if ($env:GODOT) { $env:GODOT }
    elseif (Get-Command godot -ErrorAction SilentlyContinue) { (Get-Command godot).Source }
    else { 'C:\Program Files (x86)\Steam\steamapps\common\Godot Engine\godot.windows.opt.tools.64.exe' }
}
if (-not (Test-Path $Godot)) { Die "no Godot at '$Godot'. Pass -Godot or set `$env:GODOT." }

if (-not $NoInstaller) {
    if (-not $Iscc) {
        $candidates = @(
            $env:ISCC,
            'D:\Tools\InnoSetup6\ISCC.exe',
            'C:\Program Files (x86)\Inno Setup 6\ISCC.exe',
            'C:\Program Files\Inno Setup 6\ISCC.exe'
        ) | Where-Object { $_ }
        $Iscc = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
        if (-not $Iscc -and (Get-Command ISCC -ErrorAction SilentlyContinue)) { $Iscc = (Get-Command ISCC).Source }
    }
    if (-not $Iscc -or -not (Test-Path $Iscc)) {
        Die "no Inno Setup compiler found. Pass -Iscc, set `$env:ISCC, or run with -NoInstaller."
    }
}

# Export templates are a separate ~1.2 GB download from the editor and their
# absence fails the export with a message about the *preset* being misconfigured,
# which sends you to the wrong file. Checked here so the error names the cause.
$templateVersion = (& $Godot --version 2>$null | Select-Object -Last 1)
if ($templateVersion -match '^(\d+\.\d+(?:\.\d+)?)\.(\w+)') {
    $templateDir = Join-Path $env:APPDATA "Godot\export_templates\$($Matches[1]).$($Matches[2])"
    if (-not (Test-Path (Join-Path $templateDir 'windows_release_x86_64.exe'))) {
        Die @"
no Windows export template for $($Matches[1]).$($Matches[2]).
Expected: $templateDir\windows_release_x86_64.exe
Get it from https://github.com/godotengine/godot/releases (the
Godot_v<version>_export_templates.tpz asset), and extract the contents of its
templates/ folder into the directory above.
"@
    }
}

# ---- build --------------------------------------------------------------------

# BOTH profiles, and the debug one is not an oversight to remove.
#
# `red_republic.gdextension` maps `windows.debug.x86_64` to target/debug and
# `windows.release.x86_64` to target/release. The Godot *editor* is what runs the
# import and the export, and it loads the debug entry -- so with only a release
# build present the editor cannot resolve a single GDExtension class, every
# script referencing one fails to parse, and the export ships a project whose
# scenes are placeholder nodes.
#
# This was invisible on the development desktop, where target/debug is always
# populated, and failed on the first CI run that had never built debug. The
# release build is what the player gets; the debug build is what the toolchain
# needs to produce it.
if (-not $SkipBuild) {
    Step 'building the cdylib (debug for the editor, release for the game)'
    cargo build -p red-republic-shell
    if ($LASTEXITCODE -ne 0) { Die 'cargo build (debug) failed' }
    cargo build --release -p red-republic-shell
    if ($LASTEXITCODE -ne 0) { Die 'cargo build (release) failed' }
} else {
    Step 'skipping cargo build (-SkipBuild)'
}

$dll = Join-Path $Root 'target\release\red_republic_shell.dll'
if (-not (Test-Path $dll)) { Die "no release cdylib at $dll" }

# Checked separately so the failure names the cause. Without it the symptom is a
# wall of `Cannot get class` and `Identifier not declared` from the import pass,
# which reads as a GDScript problem and sends you to the wrong file.
$editorDll = Join-Path $Root 'target\debug\red_republic_shell.dll'
if (-not (Test-Path $editorDll)) {
    Die @"
no debug cdylib at $editorDll.
The Godot editor loads the debug entry of red_republic.gdextension to run the
import and the export, so without it no GDExtension class resolves and every
script that uses one fails to parse. Run without -SkipBuild, or:
    cargo build -p red-republic-shell
"@
}

# ---- the export preset --------------------------------------------------------

Step 'generating the export preset'
# Only the options this project has an opinion about. Godot fills the rest from
# its own defaults, which is why this is short and why it does not need editing
# when the engine adds an option.
#
# `exclude_filter` drops icon.ico from the pack: the executable's icon is read
# from disk at export time, so packing it as well would ship the same 19 kB twice.
# It also drops `tools/`, which holds the generator that writes `ui/theme.tres`.
# The generated resource ships; the generator is a development tool and has no
# business in a player's install.
$preset = @"
[preset.0]

name="Windows Desktop"
platform="Windows Desktop"
runnable=true
advanced_options=false
dedicated_server=false
custom_features=""
export_filter="all_resources"
include_filter=""
exclude_filter="icon.ico,tools/*"
export_path=""
encryption_include_filters=""
encryption_exclude_filters=""
seed=0
encrypt_pck=false
encrypt_directory=false
script_export_mode=2

[preset.0.options]

custom_template/debug=""
custom_template/release=""
debug/export_console_wrapper=2
binary_format/embed_pck=false
texture_format/s3tc_bptc=true
texture_format/etc2_astc=false
binary_format/architecture="x86_64"
application/modify_resources=true
application/icon="res://icon.ico"
application/console_wrapper_icon=""
application/icon_interpolation=4
application/file_version="$Version4"
application/product_version="$Version4"
application/company_name="Noah Sabaj"
application/product_name="Red Republic"
application/file_description="Red Republic"
application/copyright="Copyright (C) $((Get-Date).Year) Noah Sabaj"
application/trademarks=""
"@
Set-Content -Path (Join-Path $GodotDir 'export_presets.cfg') -Value $preset -Encoding utf8NoBOM

# ---- export -------------------------------------------------------------------

$staging = Join-Path $Dist "RedRepublic-$Version"
if (Test-Path $staging) { Remove-Item $staging -Recurse -Force }
New-Item -ItemType Directory -Force -Path $staging | Out-Null

# A virgin checkout has no godot/.godot/ and without one no GDExtension class
# resolves, so the export would produce a game whose every scene is a placeholder
# node. The import pass is not optional even though it usually looks redundant.
Step 'importing the Godot project'
& $Godot --headless --import --path $GodotDir 2>&1 | Out-String -OutVariable importLog | Out-Null
if ($importLog -match 'Parse Error|Failed to load script|Cannot get class') {
    Write-Host $importLog
    Die 'the Godot project does not load'
}

Step 'exporting'
$exe = Join-Path $staging 'RedRepublic.exe'
& $Godot --headless --path $GodotDir --export-release 'Windows Desktop' $exe 2>&1 |
    Out-String -OutVariable exportLog | Out-Null
if ($LASTEXITCODE -ne 0) { Write-Host $exportLog; Die "godot --export-release exited $LASTEXITCODE" }
if (-not (Test-Path $exe)) { Write-Host $exportLog; Die 'the export produced no executable' }

# The cdylib is not in res://, so it is not in the pack -- Godot's GDExtension
# export plugin copies it next to the executable instead. Measured rather than
# assumed, and asserted here because a missing DLL is a game that starts and then
# has no simulation in it, which is the one failure that looks like a working
# build right up until the first frame.
$stagedDll = Join-Path $staging 'red_republic_shell.dll'
if (-not (Test-Path $stagedDll)) {
    Die 'the export did not carry red_republic_shell.dll — the game would start with no simulation'
}

# ---- is this actually the shipped build? --------------------------------------

Step 'checking the exported build'
$console = Join-Path $staging 'RedRepublic.console.exe'
if (-not (Test-Path $console)) { Die 'no console wrapper was exported, so nothing can read the check output' }
$check = & $console --headless -- --check 2>&1 | Out-String
Write-Host ($check -split "`n" |
    Where-Object { $_ -match 'Initialize godot-rust|^build |^save check|^settings check|^build check|SCRIPT ERROR' } |
    Out-String)

if ($check -match 'SCRIPT ERROR|Assertion failed|unauthored') { Die 'the exported build did not load cleanly' }
if ($check -notmatch 'save check ok') { Die 'the exported build cannot round-trip its own save' }
if ($check -notmatch 'settings check ok') { Die 'the exported build cannot round-trip its own settings' }
if ($check -notmatch 'build check ok') { Die 'the exported build cannot put up a building' }
# `release` is what makes this the shipped artifact rather than a development
# build that happens to live in dist/. A debug export reports `development`,
# which is the negative control this assertion was calibrated against.
if ($check -notmatch "build $([regex]::Escape($Version)): release") {
    Write-Host $check
    Die "the exported build does not identify as a shipped $Version build (expected 'build $Version`: release')"
}

if ($NoInstaller) {
    Step "done, without an installer: $staging"
    exit 0
}

# ---- installer ----------------------------------------------------------------

Step 'building the installer'
$iss = Join-Path $Root 'tools\installer.iss'
& $Iscc "/DAppVersion=$Version" "/DStageDir=$staging" "/DOutDir=$Dist" "/DRootDir=$Root" $iss 2>&1 |
    Out-String -OutVariable issLog | Out-Null
if ($LASTEXITCODE -ne 0) { Write-Host $issLog; Die "ISCC exited $LASTEXITCODE" }

$setup = Join-Path $Dist "RedRepublic-$Version-setup.exe"
if (-not (Test-Path $setup)) { Write-Host $issLog; Die 'ISCC reported success and produced no installer' }

$mb = [math]::Round((Get-Item $setup).Length / 1MB, 1)
Step "done: $setup ($mb MB)"
