<#
.SYNOPSIS
    Builds the Windows installer into dist/.

.DESCRIPTION
    Four steps, in order, and each one checks that the last actually worked:

      1. build the game's C# assembly,
      2. import the project twice and believe the second,
      3. export a release build,
      4. run the exported build and make it say it works,
      5. compile the installer.

    **The exported build is run, not merely produced.** An export that produces
    an executable and a pack is an export that looks successful; whether the
    thing inside starts is a separate question, and it is the one that costs a
    player their evening. `--check` founds a republic and prints what it found,
    which is what step four greps for.
#>

[CmdletBinding()]
param(
    # The Godot editor. Must be the .NET build: the ordinary one cannot load a
    # C# project at all, and says so in a way that reads like a missing file.
    [string]$Godot = $env:GODOT,

    # The Inno Setup compiler.
    [string]$Iscc = $env:ISCC,

    # Skip the installer and leave the exported build in dist/staging.
    [switch]$NoInstaller
)

$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
$GameDir = Join-Path $Root 'game'
$Dist = Join-Path $Root 'dist'

function Step([string]$what) { Write-Host "==> $what" -ForegroundColor Cyan }
function Die([string]$why) { Write-Host "!!  $why" -ForegroundColor Red; exit 1 }

# ---- the tools ----------------------------------------------------------------

if (-not $Godot) {
    $Godot = (Get-Command 'godot' -ErrorAction SilentlyContinue)?.Source
}
if (-not $Godot -or -not (Test-Path $Godot)) {
    Die 'no Godot editor found. Pass -Godot or set $env:GODOT to the .NET build.'
}

if (-not $NoInstaller) {
    if (-not $Iscc) {
        $Iscc = (Get-Command 'iscc' -ErrorAction SilentlyContinue)?.Source
    }
    if (-not $Iscc -or -not (Test-Path $Iscc)) {
        Die 'no Inno Setup compiler found. Pass -Iscc, set $env:ISCC, or run with -NoInstaller.'
    }
}

# Export templates are a separate download from the editor, and their absence
# fails the export with a message about the *preset* being misconfigured — which
# sends you to the wrong file. Checked here so the error names the cause.
$version = (& $Godot --version 2>$null | Select-Object -Last 1)
if ($version -match '^(\d+\.\d+(?:\.\d+)?)\.(\w+)') {
    $templates = Join-Path $env:APPDATA "Godot\export_templates\$($Matches[1]).$($Matches[2]).mono"
    if (-not (Test-Path $templates)) {
        Die "no .NET export templates for $($Matches[1]).$($Matches[2]) at $templates"
    }
}

# ---- build --------------------------------------------------------------------

Step 'building the simulation and its suite'
dotnet test (Join-Path $Root 'src\RedRepublic.Sim.Tests\RedRepublic.Sim.Tests.csproj') -c Release --nologo
if ($LASTEXITCODE -ne 0) { Die 'the simulation suite failed — nothing is packaged from a red build' }

# **Godot does not build the C# project itself.** In headless mode it looks for a
# built assembly and, not finding one, reports `.NET: Assemblies not found` and
# carries on far enough to look like a different problem.
Step 'building the game assembly'
dotnet build (Join-Path $GameDir 'RedRepublic.csproj') -c ExportRelease --nologo
if ($LASTEXITCODE -ne 0) { Die 'dotnet build failed' }

# ---- import -------------------------------------------------------------------

# **Twice, and only the second is believed.** A pass imports assets, and a
# resource that references one can only resolve on a pass after the one that
# imported it — `ui/theme.tres` names six fonts, and the project settings point
# the default GUI theme at it, so this is not a corner anybody can avoid.
Step 'importing'
& $Godot --headless --import --path $GameDir 2>&1 | Out-Null
$importLog = (& $Godot --headless --import --path $GameDir 2>&1 | Out-String)
if ($importLog -match 'Parse Error|Failed to load|Cannot get class') {
    Write-Host $importLog
    Die 'the Godot project does not load'
}

# ---- export -------------------------------------------------------------------

$staging = Join-Path $Dist 'staging'
if (Test-Path $staging) { Remove-Item -Recurse -Force $staging }
New-Item -ItemType Directory -Force -Path $staging | Out-Null

Step 'exporting'
$exe = Join-Path $staging 'RedRepublic.exe'
$exportLog = (& $Godot --headless --path $GameDir --export-release 'Windows Desktop' $exe 2>&1 | Out-String)
if ($LASTEXITCODE -ne 0) { Write-Host $exportLog; Die "godot --export-release exited $LASTEXITCODE" }
if (-not (Test-Path $exe)) { Write-Host $exportLog; Die 'the export produced no executable' }

# ---- is this actually a build that runs? --------------------------------------

Step 'checking the exported build'
$console = Join-Path $staging 'RedRepublic.console.exe'
if (-not (Test-Path $console)) { Die 'no console wrapper was exported, so nothing can read the check output' }

$check = (& $console --headless -- --check 2>&1 | Out-String)
Write-Host $check

if ($check -match 'SCRIPT ERROR|Unhandled exception|unauthored') {
    Die 'the exported build did not load cleanly'
}
# A run that quietly does nothing prints exactly as much as one that worked,
# which is why these are grepped for rather than the exit code being trusted.
if ($check -notmatch 'tables ok') { Die 'the exported build has no balance table in it' }
if ($check -notmatch 'founded ')  { Die 'the exported build could not found a republic' }

# ---- the version ---------------------------------------------------------------

# The game project carries it, and everything else is stamped from it.
#
# It is not enough to write it once: the exported executable's version resource
# comes from `game/export_presets.cfg`, which Godot reads and .NET does not, so
# a build shipped a Windows file version of 1.0.0.0 while the project said 2.0.0
# and nothing anywhere disagreed out loud. The preset is a committed artifact
# stamped from the project, checked the same way the theme and the balance
# checksum are.
$csproj = [xml](Get-Content (Join-Path $GameDir 'RedRepublic.csproj'))
$appVersion = $csproj.Project.PropertyGroup.Version | Where-Object { $_ } | Select-Object -First 1
if (-not $appVersion) { Die 'game/RedRepublic.csproj carries no <Version>, so the installer has none' }

$fileVersion = $appVersion
while (($fileVersion -split '\.').Count -lt 4) { $fileVersion += '.0' }

$presets = Join-Path $GameDir 'export_presets.cfg'
$was = Get-Content $presets -Raw
$now = $was `
    -replace 'application/file_version="[^"]*"', "application/file_version=`"$fileVersion`"" `
    -replace 'application/product_version="[^"]*"', "application/product_version=`"$fileVersion`""

if ($now -ne $was) {
    Set-Content $presets $now -NoNewline
    Die "game/export_presets.cfg carried a different version; it has been stamped to $fileVersion — commit it and run this again"
}

# ---- installer ----------------------------------------------------------------

if ($NoInstaller) {
    Step "done — the exported build is in $staging"
    exit 0
}


Step "compiling the installer for $appVersion"
# RootDir as well, which the script has always failed to pass: installer.iss
# #errors without it, and its message tells you to run this script -- the one
# that just did not define it. Nobody could have built an installer since the
# port, and nothing said so, because packaging is outside CI.
& $Iscc "/DAppVersion=$appVersion" "/DStageDir=$staging" "/DOutDir=$Dist" "/DRootDir=$Root" (Join-Path $PSScriptRoot 'installer.iss')
if ($LASTEXITCODE -ne 0) { Die "iscc exited $LASTEXITCODE" }

Step "done — the installer is in $Dist"
