# Releasing axilog

This covers cutting a tag-triggered release: CLI binaries, npm platform packages, and
Python wheels/sdist, all attached to one GitHub Release.

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
3. **`addon-build`** / **`npm-pack-main`** / **`npm-install-shape`** — build the napi-rs
   native addon for the same 5 targets, stage each into its `crates/axilog-node/npm/<platform>`
   package, `npm pack` every platform package plus the main `@axiapps/axilog` package, and
   validate the install shape end-to-end (packs the main + linux-x64-gnu tarballs into a
   throwaway project, installs, requires, parses the committed fixture, asserts 42 players /
   squad damage total 2138414).
4. **`wheel-build`** / **`sdist-build`** — build the Python wheel (`maturin build --release`,
   abi3 — one wheel per platform covers every CPython ≥3.9) for
   `x86_64-unknown-linux-gnu`, `x86_64-pc-windows-msvc`, `x86_64-apple-darwin`,
   `aarch64-apple-darwin`, plus a platform-independent sdist (`maturin sdist`) on Linux.
5. **`pypi-validate`** — pre-release Python gate: `twine check` on every wheel + the
   sdist, then a real install-smoke (venv `pip install` of the manylinux x86_64 wheel,
   `import axilog`, end-to-end parse of the committed fixture). The `release` job
   `needs` this, so a broken wheel never becomes a release asset.
6. **`npm-publish`** — gated npm publish step (see below).
7. **`release`** — downloads every build's artifacts, re-checks the version guard,
   generates a consolidated `SHA256SUMS`, and creates the GitHub Release (`gh release
   create`) with generated release notes and every archive/tarball/wheel/sdist +
   checksum file attached.
8. **PyPI publish** happens in a SEPARATE workflow, `.github/workflows/pypi-publish.yml`,
   which fires when the GitHub Release is *published*: it downloads the release's wheel +
   sdist assets and uploads them to PyPI via **trusted publishing** (OIDC — no token
   secret). The PyPI trusted-publisher config is bound to that exact filename; do not
   rename it.

Every job above runs the same way whether the workflow was triggered by a tag push or by
`workflow_dispatch` (manual "dry run" — see the workflow file's header comment) — the only
difference is that `npm-publish` and `release` are gated to run **only**
on `github.event_name == 'push' && startsWith(github.ref, 'refs/tags/')`. A
`workflow_dispatch` run — even one manually pointed at an existing tag ref — always
dry-runs: it builds, packs, and validates everything, but never creates a Release or
publishes to npm (and since no Release is created, PyPI trusted publishing never fires
either).

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
cross-linked binary is exactly as reliable here. The same reasoning covers the
`addon-build` job's `linux-arm64-gnu` leg.

The Python wheel matrix does **not** include an aarch64-Linux leg: maturin's manylinux
compatibility tagging inspects the actual linked ELF symbol versions on the build host,
which needs to run on the target architecture to be trustworthy — there is no hosted
aarch64 Linux runner to do that natively, so this target is deferred rather than shipped
with an unverified platform tag.

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

## Version single-sourcing

`[workspace.package].version` in the root `Cargo.toml` is the single source of truth.
Every other place a version string is hand-duplicated must be bumped in lockstep and is
checked by `scripts/check-versions.sh` (wired into `ci.yml` as a cheap ubuntu-only step,
and re-runnable locally at any time):

- `crates/axilog-node/package.json` (`version`)
- `crates/axilog-node/npm/<platform>/package.json` (`version`, one per napi-rs platform
  package: `linux-x64-gnu`, `linux-arm64-gnu`, `win32-x64-msvc`, `darwin-x64`,
  `darwin-arm64`)
- `crates/axilog-node/package.json`'s `optionalDependencies` pins on those same platform
  packages (a stale pin makes `npm install` silently resolve nothing, or the wrong
  version, for a released main package)
- `crates/axilog-py/pyproject.toml` (`[project].version` — static, not
  maturin-`dynamic`, so it needs the same manual bump as the Node versions; see the
  comment at the top of `scripts/check-versions.sh` for why this crate doesn't use
  maturin's `dynamic = ["version"]` option)
- `crates/axilog-node/package-lock.json` (root `version`, `packages[""].version`, and a
  resolved entry per `optionalDependencies` pin) — **the one exception to "bump in
  lockstep": it is refreshed *after* the release publishes, not with the bump. See
  step 6 of "Cutting a release" for why it cannot be done earlier.**

`scripts/check-versions.sh` prints every mismatch it finds (not just the first) and
exits non-zero if anything is out of sync — run it after bumping, before tagging:

```sh
scripts/check-versions.sh
```

`release.yml`'s `version-guard` job additionally checks the pushed tag itself against
`Cargo.toml` (`scripts/check-tag-version.sh`) — that's a separate check (tag vs.
Cargo.toml) from `check-versions.sh` (Cargo.toml vs. every other duplicated copy); a
release needs both to agree.

## Cutting a release

1. Bump the version everywhere `scripts/check-versions.sh` checks:
   - `[workspace.package].version` in the root `Cargo.toml`
   - `crates/axilog-node/package.json` (`version`)
   - `crates/axilog-node/npm/*/package.json` (`version`, all 5 platform packages)
   - `crates/axilog-node/package.json`'s `optionalDependencies` pins (all 5, same value)
   - `crates/axilog-py/pyproject.toml` (`[project].version`)
2. Verify everything agrees before touching git:
   ```sh
   scripts/check-versions.sh
   ```
3. Commit the bump:
   ```sh
   git add Cargo.toml crates/axilog-node/package.json crates/axilog-node/npm \
     crates/axilog-py/pyproject.toml
   git commit -m "chore: bump version to X.Y.Z"
   ```
4. Tag and push:
   ```sh
   git tag vX.Y.Z
   git push origin main
   git push origin vX.Y.Z
   ```

   Note `crates/axilog-node/package-lock.json` is deliberately NOT bumped here.
   It cannot be — see step 6.
5. Watch the `Release` workflow run in GitHub Actions. On success, a GitHub Release
   named `vX.Y.Z` will exist with:
   - One `axilog-X.Y.Z-<target>.tar.gz` / `.zip` per CLI target, each with a `.sha256`
     alongside it
   - One `.tgz` per npm package (main `@axiapps/axilog` + all 5 platform packages)
   - One `.whl` per Python wheel target, plus one sdist `.tar.gz`
   - A consolidated `SHA256SUMS` covering every archive/tarball/wheel/sdist
   - Auto-generated release notes

6. **After `npm-publish` has succeeded**, refresh the Node lockfile and push it to
   `main` as a follow-up commit:
   ```sh
   cd crates/axilog-node && npm install --package-lock-only && cd -
   scripts/check-versions.sh          # must pass, including the lockfile section
   git commit -am "chore: refresh package-lock for X.Y.Z"
   git push origin main
   ```

   **This step cannot be folded into the bump, and that ordering is not a style
   preference.** `npm ci` demands a lock entry for every `optionalDependencies`
   pin, carrying a matching `version` and a real `resolved`/`integrity` — and it
   rejects the lock whether those entries are stubbed *or* absent (both were
   tested). Those fields only exist once `@axiapps/axilog-*@X.Y.Z` is on the
   registry, and publishing happens in `npm-publish`, which is gated on the tag
   push from step 4. So at bump time the packages provably do not exist yet:
   `npm install` resolves an unresolvable *optional* dependency to a bare
   `{"optional": true}` stub and **exits 0**, so a lock regenerated in step 1
   looks fine locally and only fails later, on every `npm ci` leg of CI.

   That is exactly how 0.3.1 and 0.3.2 both broke — 12c9f71 omitted the lock,
   and 08cf911 "fixed" it by regenerating at bump time, reproducing the outage
   one cycle later. `scripts/check-versions.sh` now fails on stubbed entries, so
   a skipped step 6 is caught by the gate rather than by four red CI legs.

   Consequence to accept: between the step-4 tag push and this commit, `main`'s
   `npm ci` legs are red. That window is inherent to publishing on tag — closing
   it means having CI commit the refreshed lock back to `main` automatically
   after `npm-publish`, which is a deliberate design change, not a fix.

If the tag doesn't match `Cargo.toml`'s version, the workflow fails immediately in the
`version-guard` job (and again, redundantly, right before the Release is created) with a
clear error — fix the version, delete the bad tag (`git tag -d vX.Y.Z && git push
--delete origin vX.Y.Z`), and retag.

## Publishing to npm / PyPI

- **npm** (`npm-publish` job in `release.yml`): only runs on a real tag **push** (never
  on `workflow_dispatch`, even one pointed at an existing tag — see the workflow's
  header comment) and requires the `NPM_TOKEN` repository secret (an npm
  automation/publish token with publish rights on the `axiapps` org). Publishes every
  packed tarball (`@axiapps/axilog` + the 5 platform packages) with `--access public`,
  skipping any name@version that is already on the registry (idempotent re-runs).
  Without the secret, the step logs a clear skip message and exits successfully.
- **PyPI** (`pypi-publish.yml`, separate workflow): fires on the GitHub Release
  `published` event and uses **trusted publishing** (OIDC) — no token secret at all.
  The trusted-publisher config on PyPI is bound to the repo + the exact workflow
  filename `pypi-publish.yml`. `skip-existing: true` makes re-runs idempotent; a
  `workflow_dispatch` input allows re-publishing an existing release tag after a
  partial failure.

The GitHub Release itself (and its attached npm tarballs / wheels / sdist) is created
regardless of registry publishing, so consumers can always install from the Release.

## Verifying a downloaded release archive

```sh
sha256sum -c axilog-X.Y.Z-<target>.tar.gz.sha256
# or, against the consolidated file:
sha256sum -c SHA256SUMS --ignore-missing
```
