# Refuse any import that does not ship with Windows.
#
# slipcase-desktop 0.1.1 passed everything a developer could run and failed
# Microsoft Store certification under policy 10.2.4.1, because the MSVC target
# linked VCRUNTIME140.dll: every build machine has it and a clean machine does
# not. `+crt-static` in .cargo/config.toml is the cure and this is what notices
# if it is ever dropped or if a new dependency brings its own runtime.
#
# Run it against each release binary before packaging anything:
#   .\packaging\windows\check-imports.ps1 target\release\xodt.exe

param([Parameter(Mandatory = $true)][string]$Binary)

if (-not (Test-Path $Binary)) { throw "no $Binary" }

# Everything named here is part of Windows itself, present on a clean install of
# every supported version. Adding a name to this list is a decision: it means the
# package now has to carry that library or depend on something that does.
$shipsWithWindows = @(
    'kernel32.dll', 'user32.dll', 'gdi32.dll', 'shell32.dll', 'ole32.dll',
    'oleaut32.dll', 'advapi32.dll', 'shlwapi.dll', 'comdlg32.dll', 'comctl32.dll',
    'ws2_32.dll', 'crypt32.dll', 'bcrypt.dll', 'ntdll.dll', 'msvcrt.dll',
    'opengl32.dll', 'dwmapi.dll', 'uxtheme.dll', 'imm32.dll', 'winmm.dll',
    'dxgi.dll', 'd3d11.dll', 'd3d12.dll', 'setupapi.dll', 'cfgmgr32.dll',
    'propsys.dll', 'userenv.dll', 'version.dll', 'powrprof.dll', 'combase.dll',
    'api-ms-win-core-', 'api-ms-win-crt-', 'ext-ms-win-'
)

$dumpbin = Get-Command dumpbin -ErrorAction SilentlyContinue
if (-not $dumpbin) { throw 'dumpbin is not on PATH — run this from a Developer Command Prompt' }

$imports = & dumpbin /dependents $Binary |
    Select-String -Pattern '^\s+(\S+\.dll)$' |
    ForEach-Object { $_.Matches[0].Groups[1].Value.ToLower() }

$unexpected = $imports | Where-Object {
    $import = $_
    -not ($shipsWithWindows | Where-Object { $import.StartsWith($_) })
}

$imports | ForEach-Object { Write-Host "  $_" }

if ($unexpected) {
    Write-Error "not part of Windows: $($unexpected -join ', ')"
    exit 1
}
Write-Host "$Binary imports nothing Windows does not ship" -ForegroundColor Green
