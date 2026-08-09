# axilog — M8: Release & Packaging Infrastructure

**Status:** Approved (autonomous continuation authorized by user 2026-08-08)
**Why:** Seven milestones of functionality exist only as a source checkout. A tag-triggered
release pipeline makes axilog installable: CLI binaries for every target, npm platform packages
ready for the `@axi/axilog` publish, Python wheels. Registry publishing itself stays deferred
(credentials + scope naming are the user's call); everything up to that point becomes automatic.

## Scope

1. **Release workflow** (`release.yml`, on `v*` tags): build `axilog` CLI for
   x86_64-linux-gnu, aarch64-linux-gnu (cross), x86_64-windows-msvc, x86_64/aarch64-macos;
   strip, archive (tar.gz/zip), SHA-256 checksums; create a GitHub Release with artifacts and
   auto-generated notes. Version single-sourced from the workspace Cargo version; tag must match.
2. **npm packaging completion:** `napi create-npm-dirs`-style per-platform packages
   (`@axi/axilog-<platform>`) with `optionalDependencies` wiring in the main package; release
   workflow builds the .node per target and validates `npm pack` output loads (install-shape
   test); publish step present but gated behind a repository secret that does not exist yet
   (documented no-op until user configures).
3. **Python wheels:** maturin wheel matrix (same targets where supported) + sdist, attached to
   the GitHub Release; `pip install <wheel-url>` documented. Publish-to-PyPI step gated the same
   way.
4. **Version discipline:** one version (workspace) flows to npm package.json + pyproject via a
   release-prep script with a check step in CI (mismatch fails).
5. **Docs:** README install section (release binaries, npm tarball, wheel), RELEASING.md
   (how to cut a release: bump, tag, push).

## Gates
- A dry-run of the release pipeline logic locally where possible (archive naming, checksum
  script, version-check script) with tests; workflow YAML validated; a `v0.1.0-rc1`-style
  test of the full workflow is deferred until the user wants a public release (documented).
- Existing CI untouched and green.

## Non-goals
Actual registry publishing (gated), code signing/notarization, auto-update, changelog automation.
