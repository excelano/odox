# Install the Windows integration DESIGN.md 8 describes: the three OpenDocument
# extensions, their media types, their icons, and the entries that open them.
# Optionally the executables alongside them.
#
# Per-user, under HKCU and %LOCALAPPDATA%, which is the counterpart of the Linux
# script's default of ~/.local: no administrator, and nothing written that
# another account can see. There is no all-users variant, because the
# machine-wide half of every key here needs elevation and a packaging script that
# sometimes needs it and sometimes does not is worse than one that never does.
#
# THREE APPLICATIONS, ONE EXTENSION EACH
#
# Where the fleet's copies of this script install one executable and claim one or
# two types, this installs three and claims one type per executable. The table
# below is the only place any of it is spelled, and `uninstall.ps1` reads the
# same shape.
#
# The launcher is not installed. It exists so that `odox <file>` works from a
# shell on a machine where apt put all three on the path, and it hands off by
# replacing itself with the right viewer, which Windows has no equivalent of.
#
# **NOTHING HERE CLAIMS TO BE THE DEFAULT, AND THAT IS DELIBERATE**
#
# The extensions are added to `OpenWithProgids` and nothing else. The default
# value of `Software\Classes\.odt` is not written, and neither is `UserChoice`.
# So these appear in Open With, a person can pick one, and a double-click keeps
# going wherever it went before.
#
# Three reasons, and the first is the one that decided it. OpenDocument is an
# OASIS standard this suite reads and does not own, on a machine that may well
# have a full office suite already claiming it, and taking the default without
# being asked is a thing a person then has to undo. The macOS bundle says the
# same with `LSHandlerRank` set to `Alternate`.
#
# The second is that the extension's default ProgID beats a packaged
# association, so a script install that writes one silently shadows the Store
# package; segler measured that and had to name its Add/Remove entry around it.
# A default nobody writes cannot shadow anything.
#
# The third is that a `UserChoice` key has to be removed on the way out or the
# extension is left pointing at a program that is gone. slipcase-desktop's
# uninstaller did not, and flyleaf's had to learn to delete the key by name from
# its parent because removing the tree was not enough. A key nobody writes is a
# key nobody can strand.
#
# Author: David M. Anderson
# Built with AI assistance (Claude, Anthropic)

[CmdletBinding()]
param(
    # Where to install. The default is the per-user location Windows names for
    # applications that do not go through an installer service.
    [string] $Prefix = (Join-Path $env:LOCALAPPDATA 'Programs\Odox'),
    # Install the integration only, without copying any executable.
    [switch] $NoBinary
)

$ErrorActionPreference = 'Stop'
$here = $PSScriptRoot

# The three applications, and everything that differs between them.
#
# `Extension` and `ContentType` are OASIS's and are neither restated nor amended
# here; they are the same strings `AppxManifest.xml.in`, the `.desktop` entries
# and `odox_core::media_type` carry, so every place this suite names a type says
# the same thing.
#
# `ProgId` is chosen here. `Vendor.Product.Component` is the shape Windows
# documents, with no version suffix, because a `CurVer` indirection buys nothing
# until there is a second version to point at and costs a key that has to be got
# right.
$Applications = @(
    @{
        Exe         = 'xodt.exe'
        Product     = 'Odox Text'
        Icon        = 'xodt.ico'
        Extension   = '.odt'
        ContentType = 'application/vnd.oasis.opendocument.text'
        TypeName    = 'OpenDocument Text'
        ProgId      = 'Excelano.Odox.Text'
        Summary     = 'Read an OpenDocument text document'
    },
    @{
        Exe         = 'xods.exe'
        Product     = 'Odox Grid'
        Icon        = 'xods.ico'
        Extension   = '.ods'
        ContentType = 'application/vnd.oasis.opendocument.spreadsheet'
        TypeName    = 'OpenDocument Spreadsheet'
        ProgId      = 'Excelano.Odox.Grid'
        Summary     = 'Read an OpenDocument spreadsheet'
    },
    @{
        Exe         = 'xodp.exe'
        Product     = 'Odox Deck'
        Icon        = 'xodp.ico'
        Extension   = '.odp'
        ContentType = 'application/vnd.oasis.opendocument.presentation'
        TypeName    = 'OpenDocument Presentation'
        ProgId      = 'Excelano.Odox.Deck'
        Summary     = 'Read an OpenDocument presentation'
    }
)

# --- writing to the registry ------------------------------------------------

# The .NET API rather than PowerShell's registry provider, because a media type
# contains a forward slash and the provider reads that as a path separator: the
# MIME database key below would be created in the wrong place and the right one
# would never exist.
function Set-RegistryValue {
    param([string] $Path, [string] $Name, $Value, [string] $Kind = 'String')
    $key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey($Path)
    try {
        $key.SetValue($Name, $Value, [Microsoft.Win32.RegistryValueKind]::$Kind)
    } finally {
        $key.Close()
    }
}

# --- the executables ---------------------------------------------------------

# Cargo is asked where its target directory is rather than guessed at, because
# `[build] target-dir` in a Cargo configuration file moves it and no environment
# variable then says so. The Linux script learned this the hard way and this one
# inherits the lesson rather than repeating it.
function Find-TargetDir {
    $targetDir = $null
    if (Get-Command cargo -ErrorAction SilentlyContinue) {
        Push-Location (Join-Path $here '..\..')
        try {
            $meta = cargo metadata --format-version 1 --no-deps 2>$null | ConvertFrom-Json
            if ($meta) { $targetDir = $meta.target_directory }
        } catch { }
        finally { Pop-Location }
    }
    if (-not $targetDir) { $targetDir = Join-Path $here '..\..\target' }
    return $targetDir
}

function Find-Built([string] $name) {
    $targetDir = Find-TargetDir
    foreach ($built in 'release', 'debug') {
        $candidate = Join-Path $targetDir "$built\$name"
        if (Test-Path -LiteralPath $candidate) {
            return (Resolve-Path -LiteralPath $candidate).Path
        }
    }
    return $null
}

# --- the files ---------------------------------------------------------------

New-Item -ItemType Directory -Force -Path $Prefix | Out-Null

foreach ($app in $Applications) {
    $icon = Join-Path $here $app.Icon
    if (-not (Test-Path -LiteralPath $icon)) {
        throw "install.ps1: no $($app.Icon) - run the generator: cargo run --manifest-path packaging/make-icons/Cargo.toml"
    }
    Copy-Item -LiteralPath $icon -Destination (Join-Path $Prefix $app.Icon) -Force

    if ($NoBinary) { continue }
    $built = Find-Built $app.Exe
    if (-not $built) {
        throw "install.ps1: no $($app.Exe) built - run 'cargo build --release' first, or pass -NoBinary"
    }
    Copy-Item -LiteralPath $built -Destination (Join-Path $Prefix $app.Exe) -Force
}

# The uninstaller travels with what it removes, because Add/Remove Programs
# points at it and a person who has deleted the source tree still has to be able
# to get this off the machine.
Copy-Item -LiteralPath (Join-Path $here 'uninstall.ps1') `
          -Destination (Join-Path $Prefix 'uninstall.ps1') -Force

# --- the registry ------------------------------------------------------------

$classes = 'Software\Classes'

foreach ($app in $Applications) {
    $progId = $app.ProgId
    $installedExe = Join-Path $Prefix $app.Exe
    $installedIcon = Join-Path $Prefix $app.Icon

    # The type itself. `FriendlyTypeName` is what Explorer's Type column shows
    # and it is written as a plain string: the usual form is a reference into a
    # binary's resource table, which needs `SHLoadIndirectString` to read back,
    # and nothing this project ships would resolve one.
    Set-RegistryValue "$classes\$progId" '' $app.TypeName
    Set-RegistryValue "$classes\$progId" 'FriendlyTypeName' $app.TypeName
    Set-RegistryValue "$classes\$progId\DefaultIcon" '' "$installedIcon,0"
    Set-RegistryValue "$classes\$progId\shell\open\command" '' "`"$installedExe`" `"%1`""

    # The application behind the type. `ApplicationName` is the first place the
    # shell looks for a name a person recognises.
    Set-RegistryValue "$classes\$progId\Application" 'ApplicationName' $app.Product
    Set-RegistryValue "$classes\$progId\Application" 'ApplicationCompany' 'Excelano'
    Set-RegistryValue "$classes\$progId\Application" 'ApplicationDescription' $app.Summary
    Set-RegistryValue "$classes\$progId\Application" 'ApplicationIcon' "$installedIcon,0"

    # **The extension is added to, and not taken over.** `OpenWithProgids` is an
    # addition; the default value of the extension key, which is what a handler
    # claiming the type would write, is deliberately left alone. The header of
    # this file argues why at length.
    Set-RegistryValue "$classes\$($app.Extension)\OpenWithProgids" $progId ''

    # The media type, which is a different statement from the association and is
    # used in the other direction: name to type above, type to name here.
    Set-RegistryValue "$classes\MIME\Database\Content Type\$($app.ContentType)" `
        'Extension' $app.Extension

    # The Open With list, so the shell has a name for the executable itself and
    # a person can reach it from a file it was never registered for.
    $applications = "$classes\Applications\$($app.Exe)"
    Set-RegistryValue $applications 'FriendlyAppName' $app.Product
    Set-RegistryValue "$applications\shell\open\command" '' "`"$installedExe`" `"%1`""
    Set-RegistryValue "$applications\SupportedTypes" $app.Extension ''
}

# --- the Start menu ----------------------------------------------------------

# The counterpart of the `.desktop` entries: what puts these in front of a person
# who has not got a document to double-click yet. Three shortcuts, in a folder,
# because three loose entries named Odox something would sit apart in an
# alphabetical list with other applications between them.
$startMenu = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\Odox'
if (-not $NoBinary) {
    New-Item -ItemType Directory -Force -Path $startMenu | Out-Null
    $shell = New-Object -ComObject WScript.Shell
    foreach ($app in $Applications) {
        $installedExe = Join-Path $Prefix $app.Exe
        if (-not (Test-Path -LiteralPath $installedExe)) { continue }
        $link = $shell.CreateShortcut((Join-Path $startMenu "$($app.Product).lnk"))
        $link.TargetPath = $installedExe
        $link.WorkingDirectory = $Prefix
        $link.IconLocation = "$(Join-Path $Prefix $app.Icon),0"
        $link.Description = $app.Summary
        $link.Save()
        # Deliberately no AppUserModelID. Setting one would need the running
        # process to declare the same identity through a raw call these
        # applications cannot make under `forbid(unsafe_code)`. With neither side
        # declaring one, Windows derives both from the executable path, they
        # agree, and pinning and taskbar grouping work.
    }
}

# --- Add/Remove Programs -----------------------------------------------------

$version = '0.0.0'
$cargoToml = Join-Path $here '..\..\Cargo.toml'
if (Test-Path -LiteralPath $cargoToml) {
    $line = Select-String -LiteralPath $cargoToml -Pattern '^version = "([^"]+)"' | Select-Object -First 1
    if ($line) { $version = $line.Matches[0].Groups[1].Value }
}

$uninstallKey = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\Odox'
$uninstallCommand = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File `"$(Join-Path $Prefix 'uninstall.ps1')`""

# One entry for three applications, because one script put all three there and
# one script takes them away; three rows that each removed a third of an install
# would be three chances to leave two behind.
#
# **Not plain "Odox", and the suffix is a defect report from a sibling.** The
# Store packages name themselves Odox Text, Odox Grid and Odox Deck with a
# publisher of Excelano. segler had one row in Settings that read the same as its
# packaged one, separated by nothing but a version shape, and David removed the
# wrong one. The package's name is fixed by the manifest and the reservation;
# this one is ours, so this one is the one that changes.
Set-RegistryValue $uninstallKey 'DisplayName' 'Odox (user install)'
Set-RegistryValue $uninstallKey 'DisplayVersion' $version
Set-RegistryValue $uninstallKey 'Publisher' 'Excelano'
Set-RegistryValue $uninstallKey 'DisplayIcon' "$(Join-Path $Prefix 'xodt.ico'),0"
Set-RegistryValue $uninstallKey 'InstallLocation' $Prefix
Set-RegistryValue $uninstallKey 'UninstallString' $uninstallCommand
Set-RegistryValue $uninstallKey 'QuietUninstallString' $uninstallCommand
Set-RegistryValue $uninstallKey 'NoModify' 1 'DWord'
Set-RegistryValue $uninstallKey 'NoRepair' 1 'DWord'

# --- tell the shell ----------------------------------------------------------

# Without this the icons and the type descriptions appear at the next logon
# rather than now, which reads as the associations not having worked.
Add-Type -Namespace Odox -Name Shell -MemberDefinition @'
[DllImport("shell32.dll", CharSet=CharSet.Unicode)]
public static extern void SHChangeNotify(int eventId, uint flags, System.IntPtr item1, System.IntPtr item2);
'@
[Odox.Shell]::SHChangeNotify(0x08000000, 0x0000, [System.IntPtr]::Zero, [System.IntPtr]::Zero)

# --- what happened -----------------------------------------------------------

Write-Output "installed into $Prefix"
foreach ($app in $Applications) {
    Write-Output "  $($app.Product): $($app.Extension) added to Open With as $($app.ProgId)"
}
Write-Output ''
Write-Output 'None of these is the default for its file type, on purpose.'
Write-Output 'To make one the default, right-click a document, choose Open with,'
Write-Output 'then Choose another app, and tick Always use this app.'
