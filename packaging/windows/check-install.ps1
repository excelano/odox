<#
.SYNOPSIS
    Install the Windows integration, take it away again, and refuse if anything
    of ours is left behind or anything of anybody else's went with it.

.DESCRIPTION
    `check-imports.ps1` beside this checks the artefact the Store distributes.
    This checks the other half a green tick invites faith in, because nothing in
    `windows.yml` otherwise reaches `install.ps1` or `uninstall.ps1` at all, and
    that is exactly where the fleet's uninstall defects have lived.

    flyleaf measured one on a real machine: its uninstaller reported success and
    left a `UserChoice` naming a ProgID it had just deleted, which is the state
    that kills an extension outright. Two things together did it. Explorer writes
    a *Deny SetValue* rule on that key so no application can quietly take an
    extension over, which makes every delete that opens it for writing fail; and
    the remover caught every exception, so a failure and a key that never existed
    looked the same.

    **Odox cannot have that defect, because it never writes that key**, and this
    script is how that claim stops being a claim. What it asserts is narrower and
    sharper than the sibling's as a result: not only that our keys go away, but
    that the extension's default was never touched in the first place.

    What is checked, in the order a person would look:

      * each ProgID, its Open With entry and its media type are written
      * the default value of each extension is NOT written, before or after
      * everything of ours is gone after the uninstall
      * a ProgID that is not ours survives both, since the extensions are shared

    This registers the real ProgIDs under HKCU and takes them away again, so it
    is for a build agent or a machine where the OpenDocument entries are not
    precious. It installs no executable: `-NoBinary` covers the registry, which
    is all that is in question here.

.NOTES
    Every string this script prints is ASCII. Windows PowerShell 5.1 reads a
    BOM-less file as ANSI, and a dash outside the ASCII range inside a string
    literal decodes to a character it accepts as a string delimiter.
#>
[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$here = $PSScriptRoot
$classes = 'Software\Classes'
$failed = 0

function Fail([string] $why) {
    Write-Host "  FAIL  $why" -ForegroundColor Red
    $script:failed = 1
}

function Pass([string] $what) {
    Write-Host "  ok    $what" -ForegroundColor Green
}

# Reading, rather than the provider, for the reason the other two scripts give:
# a media type carries a forward slash and the provider reads it as a path.
function Get-Value {
    param([string] $Path, [string] $Name)
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($Path)
    if (-not $key) { return $null }
    try { return $key.GetValue($Name) } finally { $key.Close() }
}

function Test-Key([string] $Path) {
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($Path)
    if ($key) { $key.Close(); return $true }
    return $false
}

$Applications = @(
    @{ Extension = '.odt'; ProgId = 'Excelano.Odox.Text'
       ContentType = 'application/vnd.oasis.opendocument.text'; Exe = 'xodt.exe' },
    @{ Extension = '.ods'; ProgId = 'Excelano.Odox.Grid'
       ContentType = 'application/vnd.oasis.opendocument.spreadsheet'; Exe = 'xods.exe' },
    @{ Extension = '.odp'; ProgId = 'Excelano.Odox.Deck'
       ContentType = 'application/vnd.oasis.opendocument.presentation'; Exe = 'xodp.exe' }
)

# A neighbour, to prove the uninstall is surgical. It is put into the same
# `OpenWithProgids` list our entries go into, and it has to survive both halves:
# that list belongs to the extension and to every application that has ever
# offered to open one, so removing the tree would take somebody else's offer
# with it.
$neighbour = 'SomeoneElse.Odt.Reader'
foreach ($app in $Applications) {
    $key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey("$classes\$($app.Extension)\OpenWithProgids")
    try { $key.SetValue($neighbour, '', [Microsoft.Win32.RegistryValueKind]::String) } finally { $key.Close() }
}

# What the extensions' default values were before any of this ran. Whatever they
# are, they must be the same afterwards: the point of the whole arrangement is
# that odox does not touch them.
$before = @{}
foreach ($app in $Applications) {
    $before[$app.Extension] = Get-Value "$classes\$($app.Extension)" ''
}

Write-Host 'installing the integration, without executables'
& (Join-Path $here 'install.ps1') -NoBinary | Out-Null

Write-Host 'after install:'
foreach ($app in $Applications) {
    if (Test-Key "$classes\$($app.ProgId)") {
        Pass "$($app.ProgId) is written"
    } else {
        Fail "$($app.ProgId) was not written"
    }

    $listed = Get-Value "$classes\$($app.Extension)\OpenWithProgids" $app.ProgId
    if ($null -ne $listed) {
        Pass "$($app.Extension) offers $($app.ProgId) in Open With"
    } else {
        Fail "$($app.Extension) does not offer $($app.ProgId)"
    }

    $mime = Get-Value "$classes\MIME\Database\Content Type\$($app.ContentType)" 'Extension'
    if ($mime -eq $app.Extension) {
        Pass "$($app.ContentType) maps to $($app.Extension)"
    } else {
        Fail "$($app.ContentType) maps to '$mime' and not $($app.Extension)"
    }

    # The assertion this repository exists to make about Windows.
    $now = Get-Value "$classes\$($app.Extension)" ''
    if ($now -eq $before[$app.Extension]) {
        Pass "$($app.Extension) default handler untouched, which is the whole posture"
    } else {
        Fail "$($app.Extension) default went from '$($before[$app.Extension])' to '$now' - install.ps1 claimed the type"
    }

    if (Test-Key "$classes\$($app.Extension)\UserChoice") {
        Fail "$($app.Extension) has a UserChoice, which install.ps1 must never write"
    } else {
        Pass "$($app.Extension) has no UserChoice"
    }
}

Write-Host 'uninstalling'
& (Join-Path $here 'uninstall.ps1') | Out-Null

Write-Host 'after uninstall:'
foreach ($app in $Applications) {
    if (Test-Key "$classes\$($app.ProgId)") {
        Fail "$($app.ProgId) is still there"
    } else {
        Pass "$($app.ProgId) is gone"
    }

    $listed = Get-Value "$classes\$($app.Extension)\OpenWithProgids" $app.ProgId
    if ($null -ne $listed) {
        Fail "$($app.Extension) still offers $($app.ProgId)"
    } else {
        Pass "$($app.Extension) no longer offers it"
    }

    if (Test-Key "$classes\Applications\$($app.Exe)") {
        Fail "$($app.Exe) is still in the Open With list"
    } else {
        Pass "$($app.Exe) is gone from Open With"
    }

    $survived = Get-Value "$classes\$($app.Extension)\OpenWithProgids" $neighbour
    if ($null -ne $survived) {
        Pass "$($app.Extension) kept the other application's offer"
    } else {
        Fail "$($app.Extension) lost $neighbour - the uninstall removed a key that was not ours"
    }

    $after = Get-Value "$classes\$($app.Extension)" ''
    if ($after -eq $before[$app.Extension]) {
        Pass "$($app.Extension) default still untouched"
    } else {
        Fail "$($app.Extension) default is now '$after' - the uninstall changed what it never wrote"
    }
}

if (Test-Key 'Software\Microsoft\Windows\CurrentVersion\Uninstall\Odox') {
    Fail 'the Add/Remove entry is still there'
} else {
    Pass 'the Add/Remove entry is gone'
}

# Put the neighbour back the way it was found, which is to say take it away.
foreach ($app in $Applications) {
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey("$classes\$($app.Extension)\OpenWithProgids", $true)
    if ($key) {
        try { $key.DeleteValue($neighbour, $false) } finally { $key.Close() }
    }
}

if ($failed -ne 0) {
    Write-Host ''
    Write-Host 'check-install: something above is wrong. An association that outlives' -ForegroundColor Yellow
    Write-Host 'its executable, or an uninstall that takes somebody else with it, is' -ForegroundColor Yellow
    Write-Host 'the defect this script exists to catch.' -ForegroundColor Yellow
    exit 1
}

Write-Host ''
Write-Host 'check-install: installed, removed, and nothing left behind' -ForegroundColor Green
exit 0
