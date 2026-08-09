# axilog M8 — Release & Packaging Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development.

**Goal:** Tag-triggered release pipeline (CLI binaries all targets + npm platform packages +
Python wheels attached to a GitHub Release), version single-sourcing, publish steps gated on
absent secrets, docs.

## Global Constraints

- Existing `ci.yml` untouched (except nothing); new `release.yml` triggered on `v*` tags only.
- No actual publishing: npm/PyPI steps must be conditional on secrets (`NPM_TOKEN`,
  `PYPI_TOKEN`) being present, with a clear log message when skipped. NEVER embed or require
  credentials.
- Everything testable locally gets a test: version-check script, archive naming, checksum
  generation, npm package shape. Workflow YAML python-validated.
- Version single source: workspace `Cargo.toml` [workspace.package].version. package.json +
  pyproject stay in sync via check (fail CI on mismatch) — pick implementation (script in
  `scripts/` run by ci.yml AND release.yml).
- `cargo test --workspace` (205) + node + python suites stay green. MIT, warning-free.

---

### Task 1: release.yml — CLI binaries + GitHub Release

**Files:** Create `.github/workflows/release.yml`, `scripts/package-release.sh` (archive+checksum helper, unit-testable), `RELEASING.md`.

**Requirements:** on push of tag `v*`: matrix build `axilog` (release profile) for
x86_64-unknown-linux-gnu, aarch64-unknown-linux-gnu (cross via cross-rs or gcc-aarch64 —
research the simplest reliable approach on ubuntu runners; build-only, no tests), 
x86_64-pc-windows-msvc, x86_64-apple-darwin, aarch64-apple-darwin. Strip where applicable,
package as `axilog-<version>-<target>.tar.gz` (zip on windows) via `scripts/package-release.sh`,
generate SHA256SUMS. A final job creates the GitHub Release (gh or softprops action) attaching
all artifacts, with a version==tag==Cargo-version guard step (fails loudly on mismatch).
`RELEASING.md`: bump → tag → push flow. Local tests: run package-release.sh against the debug
binary, assert archive name/contents/checksum format; validate YAML parses.

### Task 2: npm platform packages + version sync

**Files:** `crates/axilog-node/npm/**` (platform package dirs), `crates/axilog-node/package.json`
(optionalDependencies), `scripts/check-versions.sh`, `.github/workflows/ci.yml` (add the check
step only), release.yml (addon build matrix + npm-pack artifacts + gated publish).

**Requirements:** create per-platform npm package dirs (linux-x64-gnu, linux-arm64-gnu,
win32-x64-msvc, darwin-x64, darwin-arm64) per napi-rs convention (`napi create-npm-dirs` or
hand-authored to the same shape — cite which); main package gains optionalDependencies on them
(same version) and keeps the local-dev fallback (require ./axilog.*.node first). Release workflow:
build the .node per target (same matrix legs as Task 1 where toolchains allow; document any
target dropped and why), `npm pack` each platform package + main, attach tarballs to the Release;
publish steps `if: secrets.NPM_TOKEN` present → else log-skip. `scripts/check-versions.sh`
asserts Cargo workspace version == package.json == platform packages == pyproject; wire into
ci.yml as one cheap step; test the script locally (both pass and injected-mismatch fail).
Install-shape test: in a temp dir, `npm install <packed main tarball> <packed platform tarball>`
and require it, parse the fixture, assert 2138414 (runs on ubuntu; add to release.yml as a
validation job and run it locally now).

### Task 3: Python wheels + docs

**Files:** release.yml (maturin wheel jobs), README install section, RELEASING.md completion.

**Requirements:** maturin build --release wheels on linux(x64)/windows(x64)/macos(x64+arm64)
(abi3 → one wheel per platform), plus sdist on ubuntu; attach to Release; gated PyPI publish
(`PYPI_TOKEN`). Local validation: build the linux wheel now, `pip install` it into a fresh venv,
run the import + parse smoke (42 players / 2138414). README "Install" section: GitHub Release
binaries (with checksum verify example), npm tarballs, wheels; note registries pending. Version
sync includes pyproject (script from Task 2 already covers — verify). Final: all suites green
locally; YAML validated; M8 milestone entry in README.

## Self-Review
Three tasks: binaries, npm, wheels — each with local validation despite the workflow itself
being tag-triggered. Publish gating explicit. Version discipline enforced by CI check. No
placeholders.
