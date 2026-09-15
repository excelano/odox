# Remove what install.ps1 put in place, and tell the shell it is gone.
#
# The whole of it, because a file association that outlives its executable is
# worse than none: Explorer keeps offering the type and a double-click fails with
# a message about a missing file rather than the dialog that would have let a
# person pick something else.
#
# **This one has less to undo than the fleet's, and that is by design.**
# `install.ps1` never writes the default value of an extension key and never
# writes `UserChoice`, so there is no default handler here to strand. The
# failure those keys cause is well recorded: slipcase-desktop's uninstaller left
# `UserChoice` behind and an extension pointed at a program that was gone, and
# flyleaf's had to learn to delete that key by name from its parent because
# removing the tree was not enough. Neither applies to a key nobody wrote. The
# `OpenWithProgids` value is removed by name below, which is the one thing here
# that does have to be surgical: the key belongs to the extension and to every
# other application that has ever offered to open one, so the tree must not go.
#
# Author: David M. Anderson
# Built with AI assistance (Claude, Anthropic)

[CmdletBinding()]
param(
    [string] $Prefix = (Join-Path $env:LOCALAPPDATA 'Programs\Odox'),
    # Leave the installed executables and icons where they are.
    [switch] $KeepFiles
)

$ErrorActionPreference = 'Stop'

# The same table install.ps1 writes from, and it has to stay the same table: an
# application added there and not here is exactly the dead association this
# script exists to prevent.
$Applications = @(
    @{ Exe = 'xodt.exe'; Icon = 'xodt.ico'; Product = 'Odox Text'
       Extension = '.odt'; ContentType = 'application/vnd.oasis.opendocument.text'
       ProgId = 'Excelano.Odox.Text' },
    @{ Exe = 'xods.exe'; Icon = 'xods.ico'; Product = 'Odox Grid'
       Extension = '.ods'; ContentType = 'application/vnd.oasis.opendocument.spreadsheet'
       ProgId = 'Excelano.Odox.Grid' },
    @{ Exe = 'xodp.exe'; Icon = 'xodp.ico'; Product = 'Odox Deck'
       Extension = '.odp'; ContentType = 'application/vnd.oasis.opendocument.presentation'
       ProgId = 'Excelano.Odox.Deck' }
)

# The .NET API for the same reason install.ps1 uses it: PowerShell's registry
# provider reads the forward slash in a media type as a path separator, so it
# would look for the wrong key here and leave the right one behind.
function Remove-Key {
    param([string] $Path)
    try {
        [Microsoft.Win32.Registry]::CurrentUser.DeleteSubKeyTree($Path, $false)
    } catch {
        Write-Verbose "nothing at $Path"
    }
}

# One value out of a key that is not ours to delete. The extension's
# `OpenWithProgids` lists every application that has ever offered to open that
# kind of file, so removing the tree would take the other offers with it.
function Remove-Value {
    param([string] $Path, [string] $Name)
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($Path, $true)
    if (-not $key) { return }
    try {
        $key.DeleteValue($Name, $false)
    } finally {
        $key.Close()
    }
}

$classes = 'Software\Classes'

foreach ($app in $Applications) {
    Remove-Key "$classes\$($app.ProgId)"
    Remove-Key "$classes\Applications\$($app.Exe)"
    Remove-Key "$classes\MIME\Database\Content Type\$($app.ContentType)"
    Remove-Value "$classes\$($app.Extension)\OpenWithProgids" $app.ProgId
}

Remove-Key 'Software\Microsoft\Windows\CurrentVersion\Uninstall\Odox'

# The Start menu folder, and only if this is all that is in it. A folder holding
# something a person put there themselves is not this script's to remove.
$startMenu = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\Odox'
if (Test-Path -LiteralPath $startMenu) {
    foreach ($app in $Applications) {
        $link = Join-Path $startMenu "$($app.Product).lnk"
        if (Test-Path -LiteralPath $link) { Remove-Item -LiteralPath $link -Force }
    }
    if (-not (Get-ChildItem -LiteralPath $startMenu -Force)) {
        Remove-Item -LiteralPath $startMenu -Force
    }
}

if (-not $KeepFiles -and (Test-Path -LiteralPath $Prefix)) {
    foreach ($app in $Applications) {
        foreach ($file in $app.Exe, $app.Icon) {
            $path = Join-Path $Prefix $file
            if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Force }
        }
    }
    # This script is running from inside the directory it is removing, which
    # Windows allows: a running PowerShell script is read once and not held open
    # the way a loaded executable is. The file goes, and the directory with it if
    # nothing else a person put there is left.
    $self = Join-Path $Prefix 'uninstall.ps1'
    if (Test-Path -LiteralPath $self) { Remove-Item -LiteralPath $self -Force }
    if (-not (Get-ChildItem -LiteralPath $Prefix -Force)) {
        Remove-Item -LiteralPath $Prefix -Force
    }
}

# Without this the types linger in Explorer until the next logon, which reads as
# the uninstall not having worked.
Add-Type -Namespace OdoxUninstall -Name Shell -MemberDefinition @'
[DllImport("shell32.dll", CharSet=CharSet.Unicode)]
public static extern void SHChangeNotify(int eventId, uint flags, System.IntPtr item1, System.IntPtr item2);
'@
[OdoxUninstall.Shell]::SHChangeNotify(0x08000000, 0x0000, [System.IntPtr]::Zero, [System.IntPtr]::Zero)

Write-Output 'removed the Odox user install'
if ($KeepFiles) { Write-Output "left the files in $Prefix" }
