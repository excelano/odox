# Release: getting odox into apt and onto crates.io, repeatably

**This is the process, not the history.** What a given release did is in
`git log` and `CHANGELOG.md`; what is here is what the next one costs and in
what order. Anything a machine can do is a script under `packaging/` or a
workflow under `.github/`; where a step is prose, that is a claim it cannot be
scripted, and a later reader is invited to prove it wrong. The loop is the
hand-cut loop in the fleet's `~/notes/releasing.md`, and `ship` runs it.

| | |
|---|---|
| Loop | hand-cut |
| Version lives in | `Cargo.toml` |
| `apt-ship` argument | `odox` |
| Packages per release | 6, three applications on amd64 and arm64 |
| crates | `odox-core` `odox-ui` `xodt` `xods` `xodp`, in that order |
| Store lanes | none yet |

## The loop

    ship odox                 where it stands; reports, changes nothing
    ship odox 0.1.1           run the loop
    ship odox 0.1.1 --from apt        pick up after a step that failed
    ship odox 0.1.1 --dry-run         say what every step would do

`ship` refuses rather than repairs, so a step that stops is a thing to go and
read. Everything below is what those steps do and why, in case one of them has
to be done by hand.

## The order

**Debian first, then crates.io, then a store lane when one exists.** Linux needs
no other machine, apt is our own repository — publishing is one command and
unpublishing is a prune — and nothing sits in anybody's review queue. Neither
Windows nor macOS has a lane yet; `packaging/windows/README.md` and
`packaging/macos/README.md` say what each still needs, and apt being ahead of a
store is a stated fact rather than an exception.

## Step 1 — Verify and bump

    ./packaging/preflight.sh --ci

Green means the suite passes, clippy is silent, the Windows arm still compiles,
the catalogue template is current, nothing compiled C, the packages build, and
GitHub is green on this commit. `--ci` is the part that needs the network.

The version lives in `[workspace.package]` in the root `Cargo.toml` and nowhere
else; `packaging/version.sh` is the only thing that reads it. Bump it, run a
build so `Cargo.lock` follows, and write both the `CHANGELOG.md` entry and
`packaging/debian/changelog` in the same commit as the bump.

**Write the Debian changelog's date as `$(date -R)`.** `ship`'s version step
refuses a day of the week the date was not, which is what lintian refuses a
package for — asking here moves it from a CI failure after the tag to a refusal
before it.

## Step 2 — Tag, and publish the release

    git tag v0.1.1 && git push origin main --tags
    gh release create v0.1.1 --title 'odox 0.1.1' --notes-file <notes>

**Create the release yourself.** A release created by a workflow with the default
`GITHUB_TOKEN` fires no `release: published` event at all, which is a GitHub
safeguard with no opt-out — and that event is what starts `linux.yml`. Where a
tag was cut before the workflow existed, or the event was missed,
`gh workflow run linux.yml -f tag=v0.1.1` does the same job.

The tag alone starts `publish-crate.yml`, so the crates land before the release
page exists.

## Step 3 — The packages

`linux.yml` builds three packages on a runner of each architecture, checks each
one, and attaches all six to the release. Nothing here is built on the machine
cutting the release: a package built where it was not compiled is how an arm64
package comes to carry an x86-64 executable.

`./packaging/debian/build-deb.sh` is the same script the workflow runs, for
building them locally to look at.

## Step 4 — apt

    apt-ship odox

Downloads every `.deb` attached to the release, adds each to the pool, re-signs
the indices and deploys. **This is the step releases lose**, because nothing
downstream complains: apt goes on serving the previous version indefinitely and
everything else looks finished.

## Step 5 — crates.io

`publish-crate.yml` runs `cargo publish --workspace` from the tag, which
publishes the five crates in dependency order and waits for the index between
them. It is token-gated and skips with no failed run where no `CRATES_IO_TOKEN`
is available; the excelano organisation carries one.

To rehearse the whole chain without publishing anything:

    cargo package --workspace

which resolves the sibling crates against each other exactly as the publish
does. A version on crates.io is immutable: a bad one is yanked, never
re-published, and the fix is the next number.

**What a crate leaves out** is in `README.md` and is real: `cargo install xodt`
gives a binary with no desktop entry, no icon and no Windows application
manifest. The apt package is the integrated install.

## What a person still has to look at

`CHECKLIST.md` is the list run against the packaged applications rather than a
developer build. Nothing headless reaches a window, and the three things that
matter most about a viewer — that a document is drawn correctly, that the icon
reads at 16 pixels, that a double-clicked file opens the right application — all
need eyes and a session.
