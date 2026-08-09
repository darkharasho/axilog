# Releasing axilog

This covers cutting a tag-triggered CLI binary release (M8 Task 1). npm
platform packages and Python wheels attach to the same GitHub Release in
later M8 tasks; this document will grow to cover their version-sync and
publish steps when those land.

## How it works

Pushing a tag matching `v*` triggers `.github/workflows/release.yml`, which:

1. **`version-guard`** — checks that the tag equals `v` + `[workspace.package].version`
   in the root `Cargo.toml` (via `scripts/check-tag-version.sh`). Fails loudly and stops
   the whole run if they don't match — nothing gets released under the wrong version.
2. **`build`** — for each target below, builds `axilog` in release profile, strips the
   binary, and packages it via `scripts/package-release.sh` into
   `axilog-<version>-<target>.tar.gz` (`.zip` on Windows) plus a `.sha256` checksum file:
   - `x86_64-unknown-linux-gnu`
   - `aarch64-unknown-linux-gnu` (cross-compiled — see below; build-only, no tests run
     for this leg)
   - `x86_64-pc-windows-msvc`
   - `x86_64-apple-darwin`
   - `aarch64-apple-darwin`
3. **`release`** — downloads every build's artifacts, re-checks the version guard,
   generates a consolidated `SHA256SUMS`, and creates the GitHub Release (`gh release
   create`) with generated release notes and every archive + checksum attached.

### aarch64-unknown-linux-gnu cross-compilation

There's no hosted GitHub Actions runner with native ARM Linux, so this target is
cross-compiled on the `ubuntu-latest` (x86_64) runner. We install
`gcc-aarch64-linux-gnu` via `apt` and point Cargo at it
(`CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER`), rather than using the `cross` tool
(cross-rs). `cross` runs builds inside Docker, which costs real minutes pulling a
multi-hundred-MB image on every release for a target that is build-only here (no tests
execute under emulation, which is `cross`'s main advantage over a bare linker). A plain
`apt install` + linker env var is faster and has no extra moving parts; axilog-cli and
its workspace dependencies are pure Rust with no aarch64-specific native deps, so a bare
cross-linked binary is exactly as reliable here.

### Packaging script

`scripts/package-release.sh <binary-path> <version> <target> <outdir>` stages the binary
under the name `axilog` (or `axilog.exe` for a target containing `windows`), archives it
(`tar.gz` normally, `.zip` for Windows — using `zip` if present, falling back to `7z a
-tzip` since that's what Windows runners have on `PATH`), and writes a
`sha256sum`-compatible `<archive>.sha256` checksum file next to it (falls back to
`shasum -a 256` on macOS runners, which don't ship `sha256sum`; the output format is
identical either way).

Validate it locally at any time (no release build required — it exercises the debug
binary):

```sh
scripts/test-package-release.sh
```

This asserts archive naming, that the binary is present and executable inside the
archive, and that the checksum file verifies with `sha256sum -c` — for both the tar.gz
and zip code paths, plus a missing-binary failure case. Not wired into `ci.yml` (it's
release-path-only tooling); run it manually before cutting a release if you've touched
the packaging script.

## Cutting a release

1. Bump the version:
   - `[workspace.package].version` in the root `Cargo.toml`
   - `crates/axilog-node/package.json` (`version` field)
   - `crates/axilog-py/pyproject.toml` (`[project].version`)

   (Automated version-sync checking across all three — `scripts/check-versions.sh`,
   wired into `ci.yml` — lands in M8 Task 2. For now, bump all three by hand and
   double-check they match before tagging.)
2. Commit the bump:
   ```sh
   git add Cargo.toml crates/axilog-node/package.json crates/axilog-py/pyproject.toml
   git commit -m "chore: bump version to X.Y.Z"
   ```
3. Tag and push:
   ```sh
   git tag vX.Y.Z
   git push origin main
   git push origin vX.Y.Z
   ```
4. Watch the `Release` workflow run in GitHub Actions. On success, a GitHub Release
   named `vX.Y.Z` will exist with:
   - One `axilog-X.Y.Z-<target>.tar.gz` / `.zip` per target, each with a `.sha256`
     alongside it
   - A consolidated `SHA256SUMS` covering every archive
   - Auto-generated release notes

If the tag doesn't match `Cargo.toml`'s version, the workflow fails immediately in the
`version-guard` job (and again, redundantly, right before the Release is created) with a
clear error — fix the version, delete the bad tag (`git tag -d vX.Y.Z && git push
--delete origin vX.Y.Z`), and retag.

## Verifying a downloaded release archive

```sh
sha256sum -c axilog-X.Y.Z-<target>.tar.gz.sha256
# or, against the consolidated file:
sha256sum -c SHA256SUMS --ignore-missing
```
