# Assemble the MSIX packages the Microsoft Store distributes: a release
# executable, the manifest with the identity and the version substituted into it,
# and the five images the manifest names.
#
# The rule this works to is the one that matters: refuse loudly rather than
# produce something subtly wrong. A package quietly built from a debug binary
# says nothing at the moment it is made and everything days later. Every check
# below exists because the thing it checks cannot be seen by looking at the
# finished package.
#
#   powershell -ExecutionPolicy Bypass -File packaging\windows\build-msix.ps1 xodt
#   ...\build-msix.ps1 -All                 # three packages, which is a release
#   ...\build-msix.ps1 xodt -SelfSign       # sign it, so it can be installed here
#   ...\build-msix.ps1 xodt -SelfSign -Certify   # and run the certification kit
#
# THREE PACKAGES, ONE SCRIPT
#
# odox ships three store packages rather than one, because a person opens a
# spreadsheet and a presentation through different doors and each door is its own
# listing. They differ in a handful of strings, so the table below holds those and
# everything else is done once per name: one manifest template, one asset set per
# application under `assets\`, and one identity per application in
# `identity.psd1`. That is the shape that differs from the rest of the fleet,
# where this script builds a single package and reads a single identity.
#
# The launcher is not here. It has no window, no file association and no store
# package, and it ships only on apt.
#
# WHAT THIS DOES NOT DO
#
# It does not submit, and it does not produce a signature that goes anywhere near
# a submission. The Store signs what it distributes, so `-SelfSign` exists only so
# that a package can be installed on this machine and looked at. The certificate
# it makes is a throwaway.
#
# This is segler's script, and slipcase-desktop's before that. Every measurement
# in the comments below was taken in one of those unless it says otherwise.
#
# Author: David M. Anderson
# Built with AI assistance (Claude, Anthropic)

[CmdletBinding()]
param(
    # Which application to package. One name, or -All for the three a release
    # needs. Validated here rather than in the body so that a typo is answered
    # with the list of names.
    [Parameter(Position = 0)]
    [ValidateSet('xodt', 'xods', 'xodp')]
    [string] $App,
    # Build all three packages, in the order the table holds them.
    [switch] $All,
    # The executable to package. With neither this nor a release build present
    # this refuses rather than building one: `cargo build --release` is the
    # caller's to run, the way it is for every other packaging script here. It
    # names one file, so it cannot be used with -All.
    [string] $Binary,
    # Where to write the packages and their staging trees. Defaulted in the body
    # rather than here: $PSScriptRoot is empty while parameters are being bound
    # in Windows PowerShell 5.1, so a default built from it is a refusal before
    # the script has run a line.
    [string] $OutDir,
    # Sign with a throwaway certificate whose subject is the manifest's
    # Publisher, so that the package can be installed here. Not for submission.
    [switch] $SelfSign,
    # Run the Windows App Certification Kit and fail on it. Needs elevation, and
    # needs the package to be installable, so it needs -SelfSign as well.
    [switch] $Certify,
    # Apply the -Certify gate to a report that already exists and do nothing
    # else. Needs no elevation and builds nothing, which is what makes the gate
    # checkable: breaking KNOWN_FINDINGS deliberately and watching this refuse is
    # the only way to know it still bites, and a kit run costs an elevated
    # session and several minutes.
    [string] $ReadReport
)

$ErrorActionPreference = 'Stop'

# What the Windows App Certification Kit says about these applications every
# time, so that `-Certify` can be quiet about those and loud about anything else.
#
# **It is empty, and it stays empty until a kit run fills it.** A baseline copied
# from another repository is a list of things somebody else measured, and it would
# silence a finding here that nobody has ever looked at. slipcase-desktop's
# `Blocked executables` entry is the likely first arrival, since it comes from the
# Rust standard library's process-spawning path reached through `webbrowser` under
# `egui-winit`, which these three link as well; likely is not measured.
#
# **A record of what is known is not a claim that it is acceptable.** Whether to
# submit with a test failing is a decision and it is David's. Recording a
# finding here does not take it.
#
# Shrink this list when a finding goes away; the run says so when one does.
$KNOWN_FINDINGS = @{
}

$here = $PSScriptRoot
$root = Split-Path -Parent (Split-Path -Parent $here)
if (-not $OutDir) { $OutDir = Join-Path $root 'dist' }

function Refuse([string] $message) {
    Write-Error "build-msix.ps1: $message"
}

# Read a certification report and apply the gate. A function so that it can be
# run against a report on its own, which is the only way to check that the gate
# bites without an elevated session and a fresh kit run: `-ReadReport` takes that
# path.
function Test-CertificationReport([string] $report) {
    # The verdict is read out of the report rather than out of an exit code. A
    # kit that ran and failed and a kit that never ran are different things, and
    # this must never call the second one a pass: a missing verdict is a refusal
    # too.
    [xml] $xml = Get-Content $report
    $overall = $xml.REPORT.OVERALL_RESULT
    if (-not $overall) {
        Refuse "the certification kit's report at $report has no OVERALL_RESULT - read it rather than trusting this script"
    }
    # Read out of `<TEST><RESULT>` and not out of an `OVERALL_RESULT` attribute.
    # Only the report element carries that attribute; every individual test states
    # its verdict in a child element, so the first version of this printed nothing
    # at all while a test was failing, and said only "WARNING". Worse than
    # useless: the kit's own overall verdict does not escalate a failing test, so
    # a test can read FAIL under an overall of WARNING.
    # Whatever this refuses on, it now says what.
    $unexpected = @()
    $seen = @{}
    foreach ($test in $xml.SelectNodes('//TEST')) {
        $node = $test.SelectSingleNode('RESULT')
        if (-not $node) { continue }
        $verdict = $node.InnerText.Trim()
        if ($verdict -eq 'PASS') { continue }
        $name = $test.GetAttribute('NAME')
        $seen[$name] = $verdict
        $expected = $KNOWN_FINDINGS[$name]
        if ($expected -eq $verdict) {
            Write-Host "$verdict  $name  (known - baselined)"
        } else {
            $unexpected += "$verdict $name"
            Write-Host "$verdict  $name  ** NOT IN THE KNOWN LIST **"
        }
        foreach ($message in $test.SelectNodes('.//MESSAGE')) {
            $text = $message.GetAttribute('TEXT')
            if ($text) { Write-Host "        $text" }
        }
    }
    # A known finding that stopped being reported is good news and not a refusal,
    # but it is said out loud, because a baseline nobody ever shrinks becomes a
    # list of things that used to be true.
    foreach ($name in $KNOWN_FINDINGS.Keys) {
        if (-not $seen.ContainsKey($name)) {
            Write-Host "gone   $name is no longer reported - take it out of KNOWN_FINDINGS"
        }
    }
    Write-Host "certification kit: $overall  ($report)"

    # The gate is the comparison against the list, not the count of things that
    # are not PASS. A finding that will be reported on every run this project ever
    # does would, if refused on, make `-Certify` refuse always - and CLAUDE.md has
    # the name for that, about the check for compiled C: a check whose red is the
    # normal state announces nothing. This one is quiet when the kit says what it
    # said last time and loud when it says anything else.
    #
    # An overall of FAIL is still a refusal on its own. The kit does not escalate
    # a failing test into it, so if it does say FAIL it has decided something the
    # per-test list does not cover.
    if ($unexpected) {
        Refuse "the certification kit reported $($unexpected.Count) finding(s) not in the known list: $($unexpected -join '; ') - certification runs it too, so this comes back"
    }
    if ($overall -eq 'FAIL') {
        Refuse 'the Windows App Certification Kit says FAIL overall'
    }
}


# Nothing above this line has run yet, which is the point: a report is read on
# its own, without building or signing anything.
if ($ReadReport) {
    if (-not (Test-Path $ReadReport)) { Refuse "no report at $ReadReport" }
    Test-CertificationReport (Resolve-Path $ReadReport).Path
    exit 0
}


# --- what the three differ in -----------------------------------------------

# The same table `packaging/macos/build-app.sh` holds, in this platform's
# spellings. Two entries need saying:
#
# `TypeName` is the shell's key for the file-type registration and is lowercase
# with no dot, which the manifest's own comment requires and which the schema
# enforces. The human-readable name the shell shows is `TypeDisplayName`, and
# that is the string macOS puts in `CFBundleTypeName`. One placeholder name,
# `@TYPE_NAME@`, therefore means different things in the two templates, which is
# worth knowing before copying a value from one to the other.
#
# `Extension` carries the dot here and does not on macOS, where a
# `public.filename-extension` tag is written without one.
#
# The descriptions are the Debian package's, because a person reading a store
# page and a person reading `apt show` are being told about the same application
# and there is no reason for two answers. `build-deb.sh` holds the originals.
$APPLICATIONS = [ordered] @{
    xodt = @{
        Product = 'Odox Text'
        ApplicationId = 'OdoxText'
        Executable = 'xodt.exe'
        Extension = '.odt'
        TypeName = 'odt'
        TypeDisplayName = 'OpenDocument Text'
        ContentType = 'application/vnd.oasis.opendocument.text'
        TileDescription = 'OpenDocument text document viewer'
        Description = "Open a .odt document and read it: headings, lists, tables, pictures and the document's own fonts, with an outline beside the page. The document is drawn from the file and nothing is sent anywhere. Nothing is written either: this release reads OpenDocument and does not save it."
    }
    xods = @{
        Product = 'Odox Grid'
        ApplicationId = 'OdoxGrid'
        Executable = 'xods.exe'
        Extension = '.ods'
        TypeName = 'ods'
        TypeDisplayName = 'OpenDocument Spreadsheet'
        ContentType = 'application/vnd.oasis.opendocument.spreadsheet'
        TileDescription = 'OpenDocument spreadsheet viewer'
        Description = 'Open a .ods workbook and read it: every sheet, the values the document holds and the text it displays, with the formula behind the cell you pick. The workbook is drawn from the file and nothing is sent anywhere. Nothing is written either: this release reads OpenDocument and does not save it.'
    }
    xodp = @{
        Product = 'Odox Deck'
        ApplicationId = 'OdoxDeck'
        Executable = 'xodp.exe'
        Extension = '.odp'
        TypeName = 'odp'
        TypeDisplayName = 'OpenDocument Presentation'
        ContentType = 'application/vnd.oasis.opendocument.presentation'
        TileDescription = 'OpenDocument presentation viewer'
        Description = "Open a .odp deck and read it: every slide at the size the document sets, its text where the document puts it, and the speaker's notes. The deck is drawn from the file and nothing is sent anywhere. Nothing is written either: this release reads OpenDocument and does not save it."
    }
}

if ($All -and $App) {
    Refuse 'pass a name or -All, not both'
}
if (-not $All -and -not $App) {
    Refuse 'name an application (xodt, xods or xodp), or pass -All to build the three'
}
$names = if ($All) { @($APPLICATIONS.Keys) } else { @($App) }
if ($All -and $Binary) {
    Refuse '-Binary names one executable, so it cannot be used with -All'
}

# --- the identity, from one place -------------------------------------------

# `identity.psd1` beside this holds what is public: the reserved name, the
# publisher display name, and the calculated forms. `Publisher` is the X.500
# string Partner Center assigns per account, the same for every Excelano
# product, and it comes from the environment so that a public repository does
# not carry an account identifier: `windows.yml` passes the organisation
# variable STORE_PUBLISHER, and a Windows machine sets STORE_PUBLISHER in its
# own environment before running this.
$identityFile = Join-Path $here 'identity.psd1'
if (-not (Test-Path $identityFile)) {
    Refuse "no identity at $identityFile - it is committed beside this script and should not be missing"
}
$identity = Import-PowerShellDataFile $identityFile
foreach ($field in 'PublisherDisplayName') {
    if (-not $identity.$field) { Refuse "identity.psd1 has no $field" }
}
if (-not $identity.Applications) {
    Refuse "identity.psd1 has no Applications table - it holds one identity per application, keyed by binary"
}
# Only the names about to be used are required, so that a half-filled file can
# still build the application whose name has been reserved. Three reservations
# arrive one at a time.
foreach ($name in $names) {
    if (-not $identity.Applications.$name) {
        Refuse "identity.psd1's Applications table has no $name"
    }
    if (-not $identity.Applications.$name.Name) {
        Refuse "identity.psd1's $name entry has no Name, which is what Partner Center calls Package/Identity/Name"
    }
}
$publisher = $env:STORE_PUBLISHER
if (-not $publisher) {
    Refuse 'no STORE_PUBLISHER in the environment - it is the X.500 string Partner Center shows under Product management, Product identity, as Package/Identity/Publisher, and it is the excelano organisation variable of that name'
}
# The one value with a shape worth checking. `Publisher` is an X.500 string and
# the display name is what gets put there by mistake; a package whose Publisher
# does not match the reservation is rejected at upload, which is the most
# expensive place to find out.
if ($publisher -notmatch '^CN=') {
    Refuse "STORE_PUBLISHER is '$publisher', which is not an X.500 string - Partner Center's Package/Identity/Publisher begins CN="
}

# --- the version, from the one parser ---------------------------------------

# `packaging/version.sh` is the only thing that reads Cargo.toml's version, and it
# is asked here rather than copied, which is the whole reason it takes an
# argument. It is POSIX sh, so it needs a shell, and Git for Windows ships one.
#
# Not `bash` off PATH. On a machine with WSL that name resolves to
# C:\Windows\System32\bash.exe, which runs inside a Linux distribution where this
# checkout is at a different path, so version.sh would read a Cargo.toml that is
# not this one - or nothing at all.
$git = Get-Command git -ErrorAction SilentlyContinue
if (-not $git) { Refuse 'git is not on PATH, and version.sh needs the shell Git for Windows ships' }
$gitRoot = Split-Path -Parent (Split-Path -Parent $git.Source)
$sh = Join-Path $gitRoot 'bin\bash.exe'
if (-not (Test-Path $sh)) { $sh = Join-Path $gitRoot 'usr\bin\sh.exe' }
if (-not (Test-Path $sh)) {
    Refuse "no shell found beside $($git.Source) - version.sh is POSIX sh and needs the one Git for Windows installs"
}
$versionScript = (Join-Path $here '..\version.sh').Replace('\', '/')
$version = & $sh $versionScript --appx
if ($LASTEXITCODE -ne 0 -or -not $version) {
    Refuse 'version.sh --appx would not answer'
}
$version = ($version | Select-Object -First 1).Trim()
# The Store requires four parts with the fourth 0, and version.sh says so too.
# This is the check that shelling out produced what was asked for rather than a
# message on standard output.
if ($version -notmatch '^\d+\.\d+\.\d+\.0$') {
    Refuse "version.sh --appx said '$version', which is not four parts ending in 0"
}

# --- the tools --------------------------------------------------------------

# None of these is on PATH on a stock machine and all are stock in the SDK. The
# newest SDK is taken, and the x64 build of the tool because that is the
# architecture everything else here is. Found once, before the first package, so
# that a missing SDK is not discovered two builds into a `-All` run.
function Find-SdkTool([string] $name) {
    $kits = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
    if (-not (Test-Path $kits)) { return $null }
    Get-ChildItem $kits -Directory |
        Where-Object { $_.Name -match '^10\.' } |
        Sort-Object { [version] $_.Name } -Descending |
        ForEach-Object { Join-Path $_.FullName "x64\$name" } |
        Where-Object { Test-Path $_ } |
        Select-Object -First 1
}
$makeappx = Find-SdkTool 'makeappx.exe'
if (-not $makeappx) { Refuse 'no makeappx.exe in any Windows SDK - install the Windows 10/11 SDK' }
$makepri = Find-SdkTool 'makepri.exe'
if (-not $makepri) { Refuse 'no makepri.exe in any Windows SDK' }

if (-not (Test-Path $OutDir)) { New-Item -ItemType Directory -Path $OutDir -Force | Out-Null }
$OutDir = (Resolve-Path $OutDir).Path

# Cargo is asked where its target directory is rather than guessed at, because
# `[build] target-dir` in a Cargo configuration file moves it and no environment
# variable then says so. Every packaging script here asks, and it is asked once
# for the three.
$targetDir = $null
if (Get-Command cargo -ErrorAction SilentlyContinue) {
    Push-Location $root
    try {
        $meta = cargo metadata --format-version 1 --no-deps 2>$null | ConvertFrom-Json
        if ($meta) { $targetDir = $meta.target_directory }
    } finally { Pop-Location }
}
if (-not $targetDir) { $targetDir = Join-Path $root 'target' }


# Everything one application needs, from the executable to the signed package and
# the certification run. Called once per name, so a `-All` release is three passes
# through this and not three scripts that might disagree.
function Build-Package([string] $name) {
    $spec = $APPLICATIONS[$name]
    $identityName = $identity.Applications.$name.Name

    # --- the executable ------------------------------------------------------

    $exe = $Binary
    if (-not $exe) { $exe = Join-Path $targetDir "release\$($spec.Executable)" }
    if (-not (Test-Path $exe)) {
        Refuse "no executable at $exe - run 'cargo build --release' first"
    }
    $exe = (Resolve-Path $exe).Path

    # Two things read straight out of the PE header, because neither is visible in
    # a finished package and both are shipping defects.
    #
    # The architecture, because the manifest declares x64, and a package whose
    # declaration disagrees with its executable installs and then fails to launch.
    #
    # The subsystem, because each `main.rs` carries `windows_subsystem = "windows"`
    # only when `debug_assertions` is off - so a debug binary packaged by mistake
    # is a console subsystem one, and a console window behind the application is a
    # defect slipcase-desktop found by eye once already. It is the cheapest check
    # in this file and it guards the one thing here that cost an eye to notice.
    $pe = [System.IO.File]::ReadAllBytes($exe)
    $peOffset = [BitConverter]::ToInt32($pe, 0x3C)
    $machine = [BitConverter]::ToUInt16($pe, $peOffset + 4)
    $subsystem = [BitConverter]::ToUInt16($pe, $peOffset + 92)
    if ($machine -ne 0x8664) {
        Refuse ("$exe is machine 0x{0:X4}, and AppxManifest declares x64" -f $machine)
    }
    if ($subsystem -ne 2) {
        Refuse "$exe is not a Windows GUI subsystem executable (subsystem $subsystem) - a debug build is a console one, and packaging that puts a console window behind the application"
    }

    # The imports, because a DLL that is not part of Windows has to already be on
    # the machine before the application will start, and the machine that matters
    # is a certification tester's rather than this one. slipcase-desktop 0.1.1 was
    # packaged, certified, submitted and failed on exactly that: it imported
    # VCRUNTIME140.dll from the Visual C++ Redistributable, which every machine
    # here has and a clean Windows does not. The check is its own script because
    # CI runs it too, and it is here because this is the last place a bad binary
    # can still be stopped.
    & (Join-Path $here 'check-imports.ps1') -Binary $exe
    if ($LASTEXITCODE -ne 0) {
        Refuse "$exe imports a DLL that does not ship with Windows - see above"
    }

    # --- the staging tree ----------------------------------------------------

    # One staging tree per application, named after it. A shared one would be
    # emptied and refilled three times in a `-All` run, and the first thing to go
    # wrong in that arrangement is the one nobody sees: a package built from
    # another application's assets is a correct-looking package with the wrong
    # picture on it.
    $stage = Join-Path $OutDir "msix-stage-$name"
    if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
    New-Item -ItemType Directory -Path (Join-Path $stage 'Assets') -Force | Out-Null

    Copy-Item $exe (Join-Path $stage $spec.Executable)

    # Each application has its own asset directory and it is staged as `Assets`,
    # which is the path the manifest names. `make-icons` writes one per
    # application because the three ship as three packages: an asset set cannot be
    # shared by packages that each carry their own copy of it.
    #
    # The launcher is not packaged, so nothing here stages a second executable.
    # A packaged application's executables live under `WindowsApps` behind an
    # app-execution alias, declaring one is a manifest extension nobody has asked
    # for, and `odox` is an apt command rather than a store product.
    $assets = Join-Path $here "assets\$name"
    if (-not (Test-Path $assets)) {
        Refuse "no assets at $assets - run 'cargo run --manifest-path packaging/make-icons/Cargo.toml'"
    }
    Copy-Item (Join-Path $assets '*.png') (Join-Path $stage 'Assets')
    # The whole directory is copied and then the five the manifest names are
    # checked, rather than the five being copied by name. The qualified variants
    # beside them are resolved by `resources.pri` and never named anywhere, so a
    # copy-by-name list would silently stop shipping them the day one was added.
    foreach ($image in 'StoreLogo.png', 'Square150x150Logo.png', 'Square44x44Logo.png',
                       'Wide310x150Logo.png', 'FileTypeLogo.png') {
        if (-not (Test-Path (Join-Path $stage "Assets\$image"))) {
            Refuse "no $image in $assets - run 'cargo run --manifest-path packaging/make-icons/Cargo.toml'"
        }
    }

    # --- the manifest --------------------------------------------------------

    $manifest = Get-Content (Join-Path $here 'AppxManifest.xml.in') -Raw
    $manifest = $manifest.
        Replace('@IDENTITY_NAME@', $identityName).
        Replace('@PUBLISHER@', $publisher).
        Replace('@PUBLISHER_DISPLAY_NAME@', $identity.PublisherDisplayName).
        Replace('@VERSION_APPX@', $version).
        Replace('@PRODUCT@', $spec.Product).
        Replace('@APPLICATION_ID@', $spec.ApplicationId).
        Replace('@EXECUTABLE@', $spec.Executable).
        Replace('@TYPE_NAME@', $spec.TypeName).
        Replace('@TYPE_DISPLAY_NAME@', $spec.TypeDisplayName).
        Replace('@CONTENT_TYPE@', $spec.ContentType).
        Replace('@EXTENSION@', $spec.Extension).
        Replace('@TILE_DESCRIPTION@', $spec.TileDescription).
        Replace('@DESCRIPTION@', $spec.Description)

    # A placeholder that survived substitution is a package that installs and is
    # wrong, so it is looked for rather than assumed away. This catches a
    # placeholder added to the template and not to this script, which is the
    # realistic way the two part company.
    $left = [regex]::Matches($manifest, '@[A-Z_]+@') |
        ForEach-Object { $_.Value } | Sort-Object -Unique
    if ($left) {
        Refuse "AppxManifest.xml.in has placeholders this script does not substitute: $($left -join ', ')"
    }

    # UTF-8 with no byte order mark. `Out-File -Encoding utf8` in Windows
    # PowerShell 5.1 writes one, and a manifest beginning with a BOM is malformed
    # XML as far as makeappx is concerned.
    [System.IO.File]::WriteAllText(
        (Join-Path $stage 'AppxManifest.xml'),
        $manifest,
        (New-Object System.Text.UTF8Encoding $false))

    # --- the resource index --------------------------------------------------

    # Without this the package ships the images and the shell reads only the five
    # the manifest names by literal path: every `scale-` and `altform-` qualifier
    # beside them is inert, because a qualifier is resolved through the resource
    # index and nowhere else.
    #
    # The visible cost of not having one was the taskbar. `BackgroundColor` is
    # `transparent`, so Windows plates the icon in the user's accent colour, and on
    # slipcase-desktop that drew the application on a purple square while the
    # side-loaded install drew the same icon unplated. The `altform-unplated` asset
    # is what stops it, and it was in that package and doing nothing until this
    # step existed.
    #
    # The configuration is written outside the staging tree on purpose. `makepri`
    # indexes the directory it is given, so a configuration file left inside it
    # becomes a resource of the package. It is named after the application so that
    # a `-All` run does not have three passes writing one path.
    $priConfig = Join-Path $OutDir "priconfig-$name.xml"
    # `en-US_de-DE` matches the `<Resource Language="en-us" />` and
    # `<Resource Language="de-de" />` the manifest declares - makepri joins the
    # tags with an underscore, and `createconfig` writes them as the Language
    # qualifier `en-US;de-DE`. If the two sides disagree the index has no default
    # language and the shell falls back to the literal paths, which is the failure
    # this whole step exists to remove - and it fails silently, so it is spelled
    # once here from the manifest's values.
    & $makepri createconfig /cf $priConfig /dq en-US_de-DE /o | Out-Null
    if ($LASTEXITCODE -ne 0) { Refuse "makepri createconfig failed ($LASTEXITCODE)" }

    # The default configuration splits qualified resources into *resource
    # packages*, which is right for a bundle and wrong for one monolithic package.
    # Left alone, `makepri` wrote `resources.scale-125.pri` and four siblings and
    # left the scale variants out of the main index entirely: `makepri dump` of the
    # installed package found no `scale-125` anywhere in it, so every one of those
    # images shipped and resolved to nothing. This is a package, not a bundle, so
    # the splitting is turned off and everything lands in one index.
    [xml] $priXml = Get-Content $priConfig
    foreach ($split in @($priXml.SelectNodes('//autoResourcePackage'))) {
        $split.ParentNode.RemoveChild($split) | Out-Null
    }
    $priXml.Save($priConfig)

    & $makepri new /pr $stage /cf $priConfig /of (Join-Path $stage 'resources.pri') /o | Out-Null
    if ($LASTEXITCODE -ne 0) { Refuse "makepri new failed ($LASTEXITCODE)" }
    Remove-Item $priConfig -Force
    if (-not (Test-Path (Join-Path $stage 'resources.pri'))) {
        Refuse 'makepri reported success and wrote no resources.pri'
    }
    # Nothing should have been split out. If a `resources.<qualifier>.pri` appears
    # beside the main one, the configuration edit above stopped working and the
    # variants are silently unresolvable again - which is a thing that looks like a
    # working package right up until somebody photographs a taskbar.
    $split = Get-ChildItem (Join-Path $stage 'resources.*.pri') -ErrorAction SilentlyContinue
    if ($split) {
        Refuse "makepri split resources into $($split.Name -join ', ') - those belong to a bundle, and this is one package"
    }

    # --- the package ---------------------------------------------------------

    # Named after the Application Id rather than the product, because the product
    # names carry a space and a package a person types the name of should not.
    $package = Join-Path $OutDir "$($spec.ApplicationId)-$version-x64.msix"
    & $makeappx pack /d $stage /p $package /o
    if ($LASTEXITCODE -ne 0) { Refuse "makeappx pack failed ($LASTEXITCODE)" }

    Write-Host ''
    Write-Host "built $package"
    Write-Host "  identity  $identityName"
    Write-Host "  publisher $($publisher)"
    Write-Host "  version   $version"
    Write-Host "  from      $exe"
    # Said out loud because the package name is deterministic, so a plain run
    # overwrites a signed package of the same version with an unsigned one and
    # says nothing about it. Deployment then fails 0x800B0100, "no signature was
    # present", which reads like a signing problem rather than like the last build
    # having been a different build.
    if (-not $SelfSign) {
        Write-Host '  unsigned  - pass -SelfSign to install it here'
    }

    # --- signing, for a local install and nothing else -----------------------

    if ($SelfSign) {
        $signtool = Find-SdkTool 'signtool.exe'
        if (-not $signtool) { Refuse 'no signtool.exe in any Windows SDK' }

        # signtool refuses a package whose manifest Publisher and whose
        # certificate subject differ, so the subject is built from the identity
        # rather than typed a second time.
        #
        # The Publisher is the Excelano account's, so one throwaway certificate
        # signs all three of these packages and any the other applications in the
        # fleet leave behind. That is correct rather than a coincidence to guard
        # against: the subject is what signtool checks, the packages are all this
        # account's, and the certificate is a throwaway either way.
        $cert = Get-ChildItem Cert:\CurrentUser\My |
            Where-Object { $_.Subject -eq $publisher -and $_.HasPrivateKey } |
            Sort-Object NotAfter -Descending |
            Select-Object -First 1
        if (-not $cert) {
            Write-Host "making a throwaway signing certificate for $($publisher)"
            $cert = New-SelfSignedCertificate -Type CodeSigningCert `
                -Subject $publisher `
                -KeyUsage DigitalSignature `
                -FriendlyName 'Excelano MSIX test signing (throwaway)' `
                -CertStoreLocation Cert:\CurrentUser\My `
                -TextExtension @('2.5.29.37={text}1.3.6.1.5.5.7.3.3')
        }
        & $signtool sign /fd SHA256 /sha1 $cert.Thumbprint $package
        if ($LASTEXITCODE -ne 0) { Refuse "signtool failed ($LASTEXITCODE)" }
        Write-Host "signed with $($cert.Thumbprint) - a throwaway, and not what the Store distributes"

        # Deployment reads LocalMachine\TrustedPeople and not the per-user store:
        # importing into CurrentUser\TrustedPeople leaves Add-AppxPackage failing
        # 0x800B0109 just the same. That import is the one administrator action in
        # this whole path, so it is printed rather than attempted.
        $trusted = Get-ChildItem Cert:\LocalMachine\TrustedPeople -ErrorAction SilentlyContinue |
            Where-Object { $_.Thumbprint -eq $cert.Thumbprint }
        Write-Host ''
        if ($trusted) {
            Write-Host 'install it:'
            Write-Host "  Add-AppxPackage $package"
            # Deployment refuses 0x80073CFB for a package whose identity and
            # version match one already installed but whose contents differ, which
            # is every rebuild during a day's work. The version is not bumped for
            # that - it is one number with three spellings and a release decision -
            # so the old one comes off first.
            Write-Host '  # rebuilding the same version? remove the installed one first:'
            Write-Host "  Get-AppxPackage $identityName | Remove-AppxPackage"
        } else {
            Write-Host 'to install it, this certificate has to be trusted, which needs administrator once:'
            Write-Host "  Export-Certificate -Cert Cert:\CurrentUser\My\$($cert.Thumbprint) -FilePath `$env:TEMP\excelano-test.cer"
            Write-Host '  # then, from an elevated prompt:'
            Write-Host "  Import-Certificate -FilePath `$env:TEMP\excelano-test.cer -CertStoreLocation Cert:\LocalMachine\TrustedPeople"
            Write-Host "  Add-AppxPackage $package"
        }
    }

    # --- the certification kit -----------------------------------------------

    if ($Certify) {
        if (-not $SelfSign) {
            Refuse '-Certify needs -SelfSign: the kit installs the package it tests, and an unsigned one will not install'
        }
        $elevated = ([Security.Principal.WindowsPrincipal] `
                [Security.Principal.WindowsIdentity]::GetCurrent()
            ).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
        if (-not $elevated) {
            Refuse 'the Windows App Certification Kit needs an elevated session - rerun this from an administrator prompt'
        }
        $appcert = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\App Certification Kit\appcert.exe'
        if (-not (Test-Path $appcert)) {
            Refuse "no appcert.exe at $appcert - the App Certification Kit is a separate SDK feature"
        }

        # Named after the application as well as the version, because three
        # packages of one version would otherwise write one report and the last
        # one would be read as though it were all three.
        $report = Join-Path $OutDir "wack-$($spec.ApplicationId)-$version.xml"

        # The report is removed first, and then the one that appears is checked for
        # being newer than this run. Both, because the first version of this did
        # neither and reported a previous run's verdict as though it were this
        # one's.
        #
        # `appcert` refuses to overwrite a report: given a path that exists it
        # prints "Please specify a unique report file name" and stops before
        # running a single test. The file was still there, `Test-Path` was
        # satisfied, and the findings printed were the previous package's - on a
        # run whose whole purpose was to test a different package under the same
        # version number.
        #
        # This is the failure the comment above already claimed to guard against,
        # which is worth reading twice: a kit that ran and failed and a kit that
        # never ran must not come out the same, and *stale* is a third state
        # neither of those words covers.
        if (Test-Path $report) { Remove-Item $report -Force }
        $startedAt = Get-Date
        & $appcert reset | Out-Null
        & $appcert test -appxpackagepath $package -reportoutputpath $report
        if (-not (Test-Path $report)) {
            Refuse "the certification kit wrote no report to $report"
        }
        if ((Get-Item $report).LastWriteTime -lt $startedAt) {
            Refuse "the report at $report is older than this run - the kit did not write it, so nothing below would be about this package"
        }

        Test-CertificationReport $report
    }
}

foreach ($name in $names) {
    Build-Package $name
}
