# Releasing

How papers-write is built, released and updated, as Papers Review and Papers
Desktop are.

## The cycle

1. The version in `package.json`, `package-lock.json`, `src-tauri/Cargo.toml`,
   `src-tauri/tauri.conf.json` and `Cargo.lock`, committed as "Version x.y.z".
2. An annotated tag `vx.y.z`, pushed. The tag starts `.github/workflows/release.yml`:
   a draft release, then one build per system (tauri-action), each uploading
   its files to the draft:
   - macOS (Apple silicon, `macos-15`): `Papers-Write-macos-aarch64.dmg`, the
     app as `.app.tar.gz` and its signature;
   - Linux (x86-64, `ubuntu-22.04`): `Papers-Write-linux-amd64.deb` and
     `Papers-Write-linux-amd64.AppImage`, with their signatures;
   - `latest.json`, which the apps read for updates.
   File names carry no version, so `releases/latest/download/<name>` always
   gives the latest one (the website links there).
3. A person reads the draft, writes its notes and publishes it. Only then do
   the apps see the update.

The builds are not code-signed or notarized: on macOS the first open is
right-click ▸ Open (or `xattr -dr com.apple.quarantine "Papers Write.app"`).

## Updates in the app

The updater plugin reads
`https://github.com/mathesislab/papers-write/releases/latest/download/latest.json`
at start and twice a day (Settings ▸ Updates, which can turn it off), and
installs only when asked. The updates are signed with a minisign key: its
public key is in `tauri.conf.json` (`plugins.updater.pubkey`), its private key
is the repository secret `TAURI_SIGNING_PRIVATE_KEY` (no password), and a copy
is kept outside the repository (`~/.tauri/papers-write.key`). A lost private
key means the installed apps cannot be updated: they need the new version
installed by hand once.

On Linux the AppImage updates itself; a `.deb` is updated by installing the
new one.

## Checks

`.github/workflows/ci.yml` on each push to main and each pull request, on
macOS and Linux: `npm run check`, `npm test`, the frontend built, `cargo
clippy -D warnings`, `cargo test`, and on Linux a smoke test: the app started
under xvfb must stay up without a panic (`scripts/ci/smoke-linux.sh`).

## Linux

The same app, but what is macOS's is not there: the PDF tab is pdf.js (not
PDFKit), the spell checker and the windows' native tabs are macOS's only. The
tools a build runs (texmark, Quarto, latexmk, pandoc) are found on PATH.

## The website

`site/` (one page, `index.html` and `style.css`, the screenshots in
`site/images/`) is published to <https://mathesislab.github.io/papers-write/>
by `.github/workflows/pages.yml` when it changes on main.

## Set up in November 2026

From v0.5.4, the first release (v0.5.0 to v0.5.3 were tags only):

1. This file.
2. CI on macOS and Linux, with the Linux smoke test.
3. Linux: the PDF tab by pdf.js where PDFKit is not.
4. Updates: `tauri-plugin-updater`, Settings ▸ Updates and Papers Write ▸
   Check for Updates…, the key pair, the secret.
5. The release workflow (macOS and Linux).
6. The website, with screenshots of a test project, and GitHub Pages turned on.
7. v0.5.4 tagged, built by the workflow, published.
