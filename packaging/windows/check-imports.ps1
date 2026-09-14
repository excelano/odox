<#
.SYNOPSIS
    Refuse a binary that imports a DLL Windows does not ship.

.DESCRIPTION
    slipcase-desktop 0.1.1 passed everything a developer could run and failed
    Microsoft Store certification under policy 10.2.4.1: the package installed
    on the tester's clean machine and would not start, because the MSVC target
    linked VCRUNTIME140.dll. That DLL ships in the Visual C++ Redistributable
    and not in Windows. `.cargo/config.toml` carries the cure, `+crt-static`,
    and this script is the guard on it.

    The defect is invisible from inside the toolchain that causes it: every
    machine that builds has Visual Studio on it. So the check is about the
    artefact and not about whether it runs here, which is the same rule as the
    `ldd` check `packaging/preflight.sh` runs on the Linux binaries.

    It parses the PE import table itself rather than shelling out to dumpbin,
    because dumpbin comes with Visual C++ and a check that needs the toolchain
    is a check that cannot run where the toolchain is absent. That is segler's
    reading, ported; the four applications here have no reason to differ.

    Both the import table and the delay-load table are walked. A delay-loaded
    DLL is just as absent on the machine that lacks it, it merely fails later.

.PARAMETER Binary
    One executable to read. Without it, every application in the suite is
    checked, which is what a release wants.

.NOTES
    Every string this script prints is ASCII. Windows PowerShell 5.1 reads a
    BOM-less file as ANSI, so a dash outside the ASCII range inside a string
    literal decodes to a character it accepts as a string delimiter, and the
    parse fails a long way from the line that caused it. Comments may carry
    whatever they like; printed text may not. slipcase-desktop's
    packaging/windows/README.md records the morning that cost.
#>
[CmdletBinding()]
param(
    [string] $Binary
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Refuse([string] $why) {
    Write-Host "check-imports: $why" -ForegroundColor Red
    exit 1
}

# Every DLL here is part of Windows itself, present on a clean install of every
# supported version. Adding a name is a decision: it means the package now has
# to carry that library or depend on something that does.
#
# This is slipcase-desktop's list, which segler measured again against a second
# application and found unchanged. Both draw through a window, and what a window
# imports is the window rather than the renderer.
$InBox = @(
    'advapi32.dll', 'bcryptprimitives.dll', 'combase.dll', 'dwmapi.dll',
    'dxgi.dll', 'gdi32.dll', 'imm32.dll', 'kernel32.dll', 'ntdll.dll',
    'ole32.dll', 'oleaut32.dll', 'opengl32.dll', 'setupapi.dll',
    'shell32.dll', 'shlwapi.dll', 'user32.dll', 'uiautomationcore.dll',
    'uxtheme.dll'
)

# API set contracts are resolved by the loader from the schema inside Windows
# itself, so there is no file to be missing. api-ms-win-crt- is the Universal C
# Runtime, a Windows component from Windows 10 onward; it is the Visual C++
# runtime beside it that is not.
$InBoxPrefixes = @('api-ms-win-', 'ext-ms-win-')

# An allowlist and never an exact list. The launcher draws nothing and links no
# toolkit, so it imports a fraction of what the three viewers do, and a check
# that compared whole lists would call the smaller one a change. The Linux side
# learned this on 2026-09-14 and the reasoning is the same here: fewer is not
# new, and what matters is that nothing outside the set appears.
$Applications = @('xodt', 'xods', 'xodp', 'odox')

function Read-Imports([string] $path) {
    $bytes = [System.IO.File]::ReadAllBytes($path)

    function U16([int] $at) { return [System.BitConverter]::ToUInt16($bytes, $at) }
    function U32([int] $at) { return [System.BitConverter]::ToUInt32($bytes, $at) }

    if ((U16 0) -ne 0x5A4D) { Refuse "$path does not start with MZ" }
    $pe = [int](U32 0x3C)
    if ((U32 $pe) -ne 0x00004550) { Refuse "$path has no PE signature at e_lfanew" }

    $sizeOfOptional = [int](U16 ($pe + 20))
    $opt = $pe + 24
    $magic = U16 $opt
    if ($magic -ne 0x20B) {
        Refuse ("$path is not PE32+ (magic 0x{0:X}) - this build targets x64" -f $magic)
    }

    # PE32+ data directories begin 112 bytes into the optional header. Entry 1
    # is the import table and entry 13 the delay-load table.
    $importRva = U32 ($opt + 112 + (1 * 8))
    $delayRva = U32 ($opt + 112 + (13 * 8))

    $sections = @()
    $sectionTable = $pe + 24 + $sizeOfOptional
    for ($i = 0; $i -lt [int](U16 ($pe + 6)); $i++) {
        $s = $sectionTable + ($i * 40)
        $sections += [pscustomobject]@{
            Virtual = U32 ($s + 12)
            Size    = [Math]::Max((U32 ($s + 8)), (U32 ($s + 16)))
            Raw     = U32 ($s + 20)
        }
    }

    function Offset([uint32] $rva) {
        foreach ($s in $sections) {
            if ($rva -ge $s.Virtual -and $rva -lt ($s.Virtual + $s.Size)) {
                return [int]($rva - $s.Virtual + $s.Raw)
            }
        }
        Refuse ("RVA 0x{0:X} falls in no section" -f $rva)
    }

    function NameAt([uint32] $rva) {
        $at = Offset $rva
        $end = $at
        while ($bytes[$end] -ne 0) { $end++ }
        return [System.Text.Encoding]::ASCII.GetString($bytes, $at, $end - $at)
    }

    $found = @()
    if ($importRva -ne 0) {
        $at = Offset $importRva
        while ((U32 ($at + 12)) -ne 0) {   # the Name RVA; a zero descriptor ends the table
            $found += NameAt (U32 ($at + 12))
            $at += 20
        }
    }
    if ($delayRva -ne 0) {
        $at = Offset $delayRva
        while ((U32 ($at + 4)) -ne 0) {    # DllNameRVA
            $found += NameAt (U32 ($at + 4))
            $at += 32
        }
    }
    return $found
}

# `[build] target-dir` moves the target directory and no environment variable
# then says so, which is why this asks cargo rather than assuming.
function Release-Directory {
    # `$PSScriptRoot` and not `$MyInvocation.MyCommand.Path`: inside a function
    # the latter describes the function, which has no Path, and under
    # `Set-StrictMode` reading it is an error rather than an empty string. The
    # fleet's copies get away with it by sitting at script scope.
    $target = Join-Path $PSScriptRoot '..\..\target'
    $meta = cargo metadata --format-version 1 --no-deps 2>$null | ConvertFrom-Json
    if ($meta) { $target = $meta.target_directory }
    return (Join-Path $target 'release')
}

$binaries = @()
if ($Binary) {
    $binaries += $Binary
} else {
    $release = Release-Directory
    foreach ($app in $Applications) { $binaries += (Join-Path $release "$app.exe") }
}

$failed = 0
foreach ($path in $binaries) {
    if (-not (Test-Path $path)) {
        Refuse "no binary at $path - run 'cargo build --release' first"
    }

    $imports = Read-Imports $path
    if ($imports.Count -eq 0) {
        Refuse "$path imports nothing, which cannot be right - the parse is wrong"
    }

    $unknown = @()
    foreach ($dll in $imports) {
        $lower = $dll.ToLowerInvariant()
        $ok = $InBox -contains $lower
        if (-not $ok) {
            foreach ($p in $InBoxPrefixes) { if ($lower.StartsWith($p)) { $ok = $true } }
        }
        if (-not $ok -and ($unknown -notcontains $dll)) { $unknown += $dll }
    }

    $distinct = $imports | Sort-Object -Unique
    Write-Host "check-imports: $path"
    Write-Host "  $($distinct.Count) distinct imports, $($unknown.Count) not known to ship with Windows"

    foreach ($dll in $unknown) {
        Write-Host "  UNKNOWN  $dll" -ForegroundColor Red
        $failed = 1
    }
}

if ($failed -ne 0) {
    Write-Host ''
    Write-Host 'A DLL that is not part of Windows has to be on the machine before' -ForegroundColor Yellow
    Write-Host 'the application will start, and a Store tester has a clean machine.' -ForegroundColor Yellow
    Write-Host 'If it is genuinely in-box, add it to $InBox above and say how that' -ForegroundColor Yellow
    Write-Host 'was confirmed. If it is not, remove the dependency.' -ForegroundColor Yellow
    exit 1
}

Write-Host '  every import ships with Windows' -ForegroundColor Green
exit 0
