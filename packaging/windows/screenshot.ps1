# Photograph one application's own window at a size the Microsoft Store accepts.
#
# Screenshots were once listed as something no script could do, and that was an
# assumption rather than a measurement. What a script cannot do is decide which
# document to open and whether the result is a good advertisement. What it can
# do is every mechanical part: size the window so the visible frame is exactly
# the size asked for, bring it to the front, drive it into the state the shot is
# of, capture it, and refuse if what came back is the wrong size.
#
#   powershell -ExecutionPolicy Bypass -File packaging\windows\screenshot.ps1 `
#       xodt -Out C:\shots\xodt-01-page.png
#   ...\screenshot.ps1 xods -Click '90,220' -Out C:\shots\xods-02-sheet.png
#   ...\screenshot.ps1 xodp -Document C:\decks\focus.odp -Out C:\shots\xodp-01.png
#
# One `-Click` taking an array, rather than the repeatable flag the Mac script
# has: PowerShell binds a parameter once, and a second `-Click` is an error
# rather than a second value.
#
# WHAT IS BEING PHOTOGRAPHED, WHICH IS NOT SIMPLY "THE DOCUMENT'S APPLICATION"
#
# The sibling this came from opens the document and lets the shell decide, which
# works there because that application owns its format and the install makes it
# the handler. Neither is true here. `packaging/windows/README.md` has the rule:
# an install adds each application to `OpenWithProgids` and never writes
# `UserChoice`, so on a machine with a full office suite the shell opens the
# office suite, and a script that shelled out to the document would photograph
# it. So the executable is launched with the document as an argument, and the
# executable is named rather than inferred.
#
# `-Association` takes the other route for the case it is actually good for: the
# MSIX package is installed, this application has been chosen in Open With, and
# what is wanted in the frame is the packaged build rather than the release
# binary it carries. The package declares no `AppExecutionAlias` and
# `WindowsApps` is not a directory a script can launch out of, so the shell is
# the only way in to it.
#
# WHAT IT REFUSES ON, AND WHY EACH ONE IS HERE
#
# **It checks the window actually reached the foreground.** `SetForegroundWindow`
# is advisory: Windows refuses it from a process that does not own the
# foreground, and it returns false rather than raising. The capture is
# `CopyFromScreen` over the window's rectangle, so a window that stayed behind is
# photographed as whatever is on top of it. That is not a hypothetical - segler's
# `03-picture.png` of 2026-09-05 went to the store folder as a picture of a
# terminal, an Explorer ribbon and a sliver of the application, and it got there
# because that script called `SetForegroundWindow` and did not look. This one
# sends the ALT tap that releases the foreground lock, retries, and refuses if
# the window is still not in front.
#
# **And it polls the geometry until it stops moving.** A cold start is still
# positioning itself seconds in, and a rect read during that produced a window
# sitting below where it was asked to be with the desktop showing along two
# edges. Two consecutive equal reads, or a refusal.
#
# TWO THINGS MEASURED RATHER THAN ASSUMED, BOTH OF THEM PIXELS
#
# `SetWindowPos` sizes the *window rect*, which on Windows 10 carries an
# invisible resize border outside the visible frame: asking for 1366x768 gave a
# frame of 1352x761, which is under the Store's minimum. The visible frame is
# `DWMWA_EXTENDED_FRAME_BOUNDS`, and the difference measured in the fleet is 14
# by 7.
#
# And that frame's top edge is one pixel above what is actually drawn, so a
# capture at exactly the frame rect picks up a sliver of whatever is behind. It
# arrived as a strip of console text across the top of slipcase-desktop's first
# two attempts. The capture is two rows taller than needed and the top two are
# cropped.
#
# A SHOT OF A DOCUMENT SITTING STILL IS THE ONE THAT COMES BACK
#
# Apple refused segler's first Mac set under guideline 2.3.3 for four frames of a
# document at rest, and the Microsoft Store asks for the same thing in gentler
# words. These three cannot be *used* the way an editor can - there is nothing to
# type into and nothing to save - so a shot shows a person having got somewhere
# instead: a heading picked in the outline and the page moved to it, a different
# sheet open, a slide chosen out of the deck. Each application's side panel is a
# list of clickable rows down the left edge, which is what `-Click` is for.
#
# Author: David M. Anderson
# Built with AI assistance (Claude, Anthropic)

[CmdletBinding()]
param(
    # Which application to photograph. Validated here rather than in the body so
    # that a typo is answered with the list of names.
    [Parameter(Position = 0)]
    [ValidateSet('xodt', 'xods', 'xodp')]
    [string] $App,
    # The document to open. Which one is an editorial decision and not this
    # script's; the default is the corpus file `packaging/store-listing.md`
    # tells a reviewer to open, which is the document the listing's own pictures
    # should therefore be of. Defaulted in the body rather than here, because
    # $PSScriptRoot is empty while parameters are being bound in Windows
    # PowerShell 5.1 and a default built from it is a refusal before the script
    # has run a line.
    [string] $Document,
    [Parameter(Mandatory = $true)][string] $Out,
    # The executable to launch. Defaults to the release build, the way every
    # packaging script here does.
    [string] $Binary,
    # Open the document through the shell instead, which reaches the installed
    # package where this application is the chosen handler. The block above says
    # when that is the one to want.
    [switch] $Association,
    # The Store's minimum for a desktop screenshot, and the default because a
    # window this size looks like a window rather than like an advertisement.
    [int] $Width = 1366,
    [int] $Height = 768,
    # Where to put the window. Anywhere it fits entirely on screen.
    [int] $X = 200,
    [int] $Y = 100,
    # Seconds to wait after the window appears before capturing. The default is
    # enough for a document of a few pages; a deck whose slides carry pictures
    # wants more.
    [int] $Settle = 3,
    # Points to click inside the window before capturing, as "X,Y" in the
    # coordinates of the picture this writes, applied in order.
    [string[]] $Click = @()
)

$ErrorActionPreference = 'Stop'

function Refuse([string] $message) { Write-Error "screenshot.ps1: $message" }

if (-not $App) {
    Refuse 'name an application: xodt, xods or xodp'
}

# The two strings this needs per application. `build-msix.ps1` holds the full
# table and `packaging/macos/build-app.sh` holds the same one in that platform's
# spellings; only these columns are wanted here.
$APPLICATIONS = @{
    xodt = @{ Executable = 'xodt.exe'; Sample = 'corpus\libreoffice\text.odt' }
    xods = @{ Executable = 'xods.exe'; Sample = 'corpus\libreoffice\calc.ods' }
    xodp = @{ Executable = 'xodp.exe'; Sample = 'corpus\libreoffice\growing-liberty.odp' }
}
$spec = $APPLICATIONS[$App]

# $PSScriptRoot and never $MyInvocation.MyCommand.Path: read inside a function
# the second one describes the function rather than the file, and under
# Set-StrictMode it is an error rather than a wrong answer. It cost a CI run in
# this repository once already.
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)

if (-not $Document) { $Document = Join-Path $root $spec.Sample }
if (-not (Test-Path $Document)) { Refuse "no document at $Document" }
$Document = (Resolve-Path $Document).Path

# Cargo is asked where its target directory is rather than guessed at, because
# `[build] target-dir` in a Cargo configuration file moves it and no environment
# variable then says so. Every packaging script here asks.
if (-not $Association) {
    if (-not $Binary) {
        $targetDir = $null
        if (Get-Command cargo -ErrorAction SilentlyContinue) {
            Push-Location $root
            try {
                $meta = cargo metadata --format-version 1 --no-deps 2>$null | ConvertFrom-Json
                if ($meta) { $targetDir = $meta.target_directory }
            } finally { Pop-Location }
        }
        if (-not $targetDir) { $targetDir = Join-Path $root 'target' }
        $Binary = Join-Path $targetDir "release\$($spec.Executable)"
    }
    if (-not (Test-Path $Binary)) {
        Refuse "no executable at $Binary - run 'cargo build --release' first, or pass -Association to photograph the installed package"
    }
    $Binary = (Resolve-Path $Binary).Path
}

$outDir = Split-Path -Parent $Out
if ($outDir -and -not (Test-Path $outDir)) {
    New-Item -ItemType Directory -Path $outDir -Force | Out-Null
}

Add-Type -AssemblyName System.Drawing
Add-Type -Namespace Shot -Name Win -MemberDefinition @'
[DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
[DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
[DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint flags);
[DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int attr, out RECT r, int size);
[DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
[DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
[DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
public struct RECT { public int Left, Top, Right, Bottom; }
'@

# The visible frame, which is what everything below is measured against.
# DWMWA_EXTENDED_FRAME_BOUNDS is attribute 9 and the RECT it fills is 16 bytes.
function Get-Frame([IntPtr] $h) {
    $r = New-Object Shot.Win+RECT
    [void][Shot.Win]::DwmGetWindowAttribute($h, 9, [ref]$r, 16)
    return $r
}

# Anything already running is stopped, so the window being photographed is the
# one holding this document and not a previous one. By process name, which is
# this application's alone: the other two viewers can have documents open
# throughout and are left where they are.
$processName = [System.IO.Path]::GetFileNameWithoutExtension($spec.Executable)
Get-Process $processName -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 1

if ($Association) {
    Start-Process $Document
} else {
    # Quoted here rather than left to Start-Process, which joins its argument
    # list with spaces and quotes nothing: an unquoted path with a space in it
    # arrives as two arguments, and the application reports that it cannot find
    # the first half of its own corpus directory.
    Start-Process -FilePath $Binary -ArgumentList "`"$Document`""
}
Start-Sleep -Seconds 6

$app = Get-Process $processName -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $app -or $app.MainWindowHandle -eq [IntPtr]::Zero) {
    if ($Association) {
        Refuse "nothing named $processName opened $Document - the shell gave the document to another application, which is what OpenWithProgids without UserChoice means"
    }
    Refuse "$Binary did not open $Document"
}
$handle = $app.MainWindowHandle

# The border measured on this platform, and the two spare rows for the crop.
$BORDER_W = 14
$BORDER_H = 7
$TOPMOST = [IntPtr](-1)
$NOTOPMOST = [IntPtr](-2)

[void][Shot.Win]::SetWindowPos(
    $handle, $TOPMOST, $X, $Y, $Width + $BORDER_W, $Height + $BORDER_H + 2, 0)

# Bring it to the front and then look, rather than ask and assume. A tap of ALT
# releases the foreground lock that stops a background process raising a window;
# without it `SetForegroundWindow` returns false and the window stays where it
# was, which is how a picture of a terminal reached a store folder.
$front = $false
foreach ($try in 1..10) {
    [Shot.Win]::keybd_event(0x12, 0, 0, [UIntPtr]::Zero)
    [Shot.Win]::keybd_event(0x12, 0, 2, [UIntPtr]::Zero)
    [void][Shot.Win]::SetForegroundWindow($handle)
    Start-Sleep -Milliseconds 400
    if ([Shot.Win]::GetForegroundWindow() -eq $handle) { $front = $true; break }
}
if (-not $front) {
    Refuse 'the window would not come to the foreground - a capture now would photograph whatever is on top of it'
}

# And wait for it to stop moving. Two consecutive equal reads of the frame, or
# a refusal: a rect read while a cold start is still positioning itself is a
# rect the capture will not find the window at.
$previous = $null
$still = $false
foreach ($try in 1..40) {
    $now = Get-Frame $handle
    if ($null -ne $previous -and
        $now.Left -eq $previous.Left -and $now.Top -eq $previous.Top -and
        $now.Right -eq $previous.Right -and $now.Bottom -eq $previous.Bottom) {
        $still = $true
        break
    }
    $previous = $now
    Start-Sleep -Milliseconds 250
}
if (-not $still) { Refuse 'the window never stopped moving' }

# The clicks, in the coordinates of the picture this writes. The window is in
# front and settled by now, which matters: the first click on an inactive window
# activates it and is swallowed, so a click sent any earlier would land nowhere
# and leave a frame that shows none of what it was for.
if ($Click.Count -gt 0) {
    # Flattened to a list of numbers and consumed in pairs, rather than parsed
    # one "X,Y" at a time. `powershell -File` hands every argument over as a
    # string and collapses `'90,220','640,300'` into one of them, so a parser
    # that insists on two numbers per element refuses a call that looks right.
    # This way `-Click '90,220'`, `-Click '90,220','640,300'` and
    # `-Click 90,220,640,300` all mean what they appear to.
    $numbers = @($Click -join ',' -split ',' | ForEach-Object { $_.Trim() } |
        Where-Object { $_ -ne '' })
    if ($numbers.Count % 2 -ne 0) {
        Refuse "-Click wants pairs of coordinates and was given $($numbers.Count) number(s): $($numbers -join ',')"
    }
    $frame = Get-Frame $handle
    for ($i = 0; $i -lt $numbers.Count; $i += 2) {
        $cx = $frame.Left + [int] $numbers[$i]
        $cy = $frame.Top + 2 + [int] $numbers[$i + 1]
        [void][Shot.Win]::SetCursorPos($cx, $cy)
        Start-Sleep -Milliseconds 300
        [Shot.Win]::mouse_event(0x02, 0, 0, 0, [UIntPtr]::Zero)
        [Shot.Win]::mouse_event(0x04, 0, 0, 0, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 700
        Write-Host "  clicked $cx,$cy"
    }
}

# The pointer goes somewhere the window is not, because egui draws hover state
# and the capture keeps it. Measured on slipcase-desktop 2026-08-29: a retake
# landed with the mouse resting over a field, which came out highlighted and
# focus-ringed in a picture meant to show the application at rest, and with the
# scroll bar drawn because the pointer was inside the scroll area. Neither is
# wrong and both are noise a shopper reads as an interface doing something.
#
# Bottom right of the virtual screen rather than a constant: the window is
# placed near the top left, and a fixed 1900x1200 is off-screen on a smaller
# display, where Windows clamps it to an edge the window might occupy.
Add-Type -AssemblyName System.Windows.Forms
$away = [System.Windows.Forms.SystemInformation]::VirtualScreen
[System.Windows.Forms.Cursor]::Position =
    New-Object System.Drawing.Point(($away.Right - 2), ($away.Bottom - 2))

# Long enough for the window to settle and repaint at its new size. egui draws
# on demand, and a capture taken during the resize catches a half-laid-out frame.
Start-Sleep -Seconds $Settle

if ([Shot.Win]::GetForegroundWindow() -ne $handle) {
    Refuse 'the window lost the foreground between settling and the capture'
}

$rect = Get-Frame $handle
$frameW = $rect.Right - $rect.Left
$frameH = $rect.Bottom - $rect.Top
if ($frameW -lt $Width -or $frameH -lt $Height + 2) {
    Refuse "the visible frame came back ${frameW}x${frameH}, which cannot yield ${Width}x${Height} - the resize border is not what this script measured"
}

$full = New-Object System.Drawing.Bitmap($frameW, $frameH)
$graphics = [System.Drawing.Graphics]::FromImage($full)
$graphics.CopyFromScreen(
    $rect.Left, $rect.Top, 0, 0, (New-Object System.Drawing.Size($frameW, $frameH)))
$graphics.Dispose()

$shot = $full.Clone(
    (New-Object System.Drawing.Rectangle(0, 2, $Width, $Height)), $full.PixelFormat)
$shot.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$shot.Dispose()
$full.Dispose()

[void][Shot.Win]::SetWindowPos($handle, $NOTOPMOST, 0, 0, 0, 0, 0x0003)

# Read back rather than trusted. A screenshot of the wrong size is refused at
# upload, and this is the one property of it a machine can check.
$written = [System.Drawing.Image]::FromFile((Resolve-Path $Out).Path)
$got = "$($written.Width)x$($written.Height)"
$written.Dispose()
if ($got -ne "${Width}x${Height}") {
    Refuse "wrote $got and the Store was asked for ${Width}x${Height}"
}
Write-Host "wrote $Out - $got, of $App, from $Document"
Write-Host 'look at it before it goes anywhere: a correct size is not a good screenshot,'
Write-Host 'and a document sitting still is the shot that comes back'
