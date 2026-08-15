## Project Context

axilog is a cross-platform, CLI-first reimplementation of Elite Insights for parsing GW2 arcdps combat logs, part of the axi suite. It has a reusable Rust parsing core with planned Python/Node SDKs, matches standard Elite Insights functionality, and follows the arcdps spec more closely — notably down contribution, CCs over time, and full timeline support.

## Goals

- Cross-platform CLI parser for GW2 arcdps logs as the first-class interface
- Reusable Rust parsing core shared across CLI and SDKs
- Planned Python and Node SDKs on top of the Rust core
- Full parity with standard Elite Insights functionality
- Closer adherence to the arcdps spec: down contribution and CCs over time (full timeline support)

## Out of scope

- Not tied to a single OS / Windows-only like the original EI

## Suggested stack

- **Rust** — Fast, cross-platform parsing core reused by the CLI and all SDKs
- **Python SDK** — Bindings over the Rust core for scripting/analysis users
- **Node SDK** — Bindings over the Rust core for JS/TS integrations

## Working in this repo efficiently

This is a large repo (~134k LOC Rust, 847 tests, 24G `target/`). Context and
token budget are real constraints — work accordingly.

- **Worktrees live outside the repo**, at `../axilog-worktrees/<name>` or
  `../axilog-mN`. Never create one under the repo root: search tools then
  return every hit twice.
- **Don't read the big files whole.** `crates/axilog-ei/src/lib.rs` (~3.3k
  lines), `crates/axilog-core/src/analysis/ei_replay.rs` (~2.6k), and
  `crates/axilog-schema/src/lib.rs` (~1.9k) are each a large slice of a
  context window. Grep to a line number and read the region around it.
- **Plan docs are reference, not preamble.** `docs/superpowers/plans/*.md` run
  to 1.3k–3.6k lines. Read the section you need; don't page in a whole plan to
  re-establish context.
- **Scope test runs.** Prefer `cargo test -p <crate> -q` over a full-workspace
  run while iterating; run the whole suite once before merging. `-q` matters —
  847 passing test names is pure noise.
- **Check `docs/ROADMAP.md` first** for where a milestone left off, rather than
  re-deriving state from the code and git history.
- **Never read `fixtures/local/`** — real `.zevtc` logs with real account
  names. Use the committed fixtures under `crates/*/tests/`.
