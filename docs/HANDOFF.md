# Handoff — axilog, 2026-08-16

## Where things stand

Phase B of the native-format program is **merged**. PR #5 landed on `main` as
`521abc9` (16 commits, 39 files, +3601/−151). CI was green on all four targets.
The remote branch is deleted; the local branch `feat/phase-b-native-gaps` and
its worktree at `../axilog-worktrees/spec2-absorption` still exist and are safe
to remove whenever.

Phase B closed the five gaps native could close that ei-json can't:
per-target split widened 7 → 23 fields, replay `dc` (despawn) intervals,
commander-tag segments, engine-side `distToCom`/`stackDist`
(`crates/axilog-core/src/analysis/distance.rs`, new), and
`encounter.started_at_unix`.

## The program this belongs to

Goal: **axibridge runs entirely off axilog's native format, no ei-json shim.**
Two standing rulings from the owner:

1. The ei-json translation layer is **permanent** — it stays as a thin
   translation over the native document. Do not propose sunsetting it.
2. Native 1.0 is **malleable** — breaking changes land without a major bump
   while the in-tree adapter is 1.0's only reader. Record each in
   `docs/NATIVE-FORMAT.md` §"1.x compatibility rules". Don't stop to ask.

Phases: container DONE → A (side-channel absorption) DONE → **B DONE** →
**C DONE** (icons; proc flags split out as MPROC) → D (axibridge-side
reader rewrite — the owner's, not ours).

## Facts that cost real time to learn

- `CommanderTag::segments` and `markers[].time_ms` are in **arcdps session
  time**, not log-relative. A doc claiming otherwise was Phase B's worst
  defect. Fixture proof: `duration_ms=49285` with segments at `33847418`.
- A replay `dc` window still open at log end is **dropped entirely**; a
  commander segment still open **is closed** at the last event time. This
  divergence is deliberate, mirrors EI's own two rules, and must not be
  "harmonized."
- On the distance scalars: `None` = the position pass never ran;
  `-1.0` = it ran and nothing qualified. Never collapse these.
- Task 7's residuals, for anyone re-measuring: 0.0104 in against GW2EI's own
  exported positions; 1.6944% end-to-end (this project's resampler sits in
  between, which is why the plan's 1e-9 gate was split).

## Known debt, deliberately parked

- `t0` is re-derived in `distance.rs:142`, duplicating `replay.rs:172`.
- `axilog.pyi` beyond `PerTargetDetail` was not swept for pre-1.0 staleness.
- `dc` is empty on all 42 fixture rows, so that leg of the SDK comparison is
  vacuous; neither SDK asserts `dc`'s presence before comparing, so a rename
  would pass silently.
- A `markers.rs` tag-colour-swap can produce two overlapping segments.
- Windows CI `LNK1201`: the CLI bin and the Python cdylib are both named
  `axilog`, so their PDBs collide. Root-caused, not fixed; a rerun usually goes
  green. Real fix is renaming one. **Not a Phase B regression.**

## Working here

- Big files — read by region, never whole: `crates/axilog-ei/src/lib.rs` (~3.3k),
  `analysis/ei_replay.rs` (~2.6k), `crates/axilog-schema/src/lib.rs` (~1.9k).
- `cargo fmt --all` must NOT be run — this repo is hand-formatted.
- ei-json goldens must not move.
- **Never read `fixtures/local/`** — real logs with real account names.
- Scope tests: `cargo test -p <crate> -q`. Full workspace is 941 tests.
- Python SDK: there is no repo venv. Use
  `VIRTUAL_ENV=/var/tmp/mstephens/axilog-py-venv /var/tmp/mstephens/axilog-py-venv/bin/maturin develop --release`
  from `crates/axilog-py`, then that venv's `python -m unittest discover -s tests`.
  Any `.venv/bin/...` path in the plan docs is stale.
- Commits must be signed: prefix with
  `SSH_AUTH_SOCK="$HOME/.1password/agent.sock"`. `git log --format=%G?` returns
  `N` here even for signed commits (no allowedSignersFile) — check with
  `git cat-file -p <sha> | grep -c gpgsig` instead. Never use `--no-gpg-sign`.
- Worktrees go at `../axilog-worktrees/<name>`, never under the repo root.
- Use `/var/tmp`, not `/tmp` (quota wedges the shell).
- No unattended autonomous loops — that mandate was revoked 2026-08-14 for cost.

## Do not start without asking

MPROC (skill proc/instant-cast flags — a port of GW2EI's ~616-finder
instant-cast subsystem, NOT the catalog the old "Phase C proc flags" wording
implied; scoped in `docs/ROADMAP.md`), Phase D, a `--compact` CLI flag,
format-level size work, MOBJ (`wvWMapData` objectives), pre-existing clippy
warnings.

---

Assembled 2026-08-16 from memory files and the Phase B session. The SDD ledger
it partly draws on was deleted at the end of Phase B per the skill;
`git log b5ba6be..521abc9` is the surviving record if a detail needs checking.
