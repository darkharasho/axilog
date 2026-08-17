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

- ~~`t0` re-derived in `distance.rs`/`replay.rs`~~ — CLOSED 2026-08-16. It was
  16 call sites, not 2; all now call `RawLog::log_start_ms()`.
- ~~`axilog.pyi` beyond `PerTargetDetail` was not swept~~ — CLOSED 2026-08-16.
  BOTH SDK stubs were stale (9 Python types, 8 TypeScript types). Now guarded
  by `crates/axilog-schema/tests/v1_sdk_stubs.rs`, which fails if a field in
  the key-set golden is absent from either stub.
- `dc` is empty on all 42 fixture rows, so that leg of the SDK comparison is
  vacuous; neither SDK asserts `dc`'s presence before comparing, so a rename
  would pass silently.
- ~~A `markers.rs` tag-colour-swap can produce two overlapping segments~~ —
  CLOSED 2026-08-16 by porting EI's per-player cutoff (`break` at the first
  commander window with `EndNotSet`). The between-player overlap rule in
  `distance.rs` was already correct.
- ~~Windows CI `LNK1201`~~ — CLOSED 2026-08-16 (fix itself landed `db34709`,
  2026-08-15). The old "rename one of the two `axilog` targets" plan was both
  impossible (each name is public API) and unnecessary: the Windows leg now
  builds with no debuginfo, so no PDB is opened. See `docs/ROADMAP.md` for the
  verification. Don't reopen this on the strength of an old comment.

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

Phase D, a `--compact` CLI flag, format-level size work. (MOBJ and MPROC
were both on this list; both landed 2026-08-16.)

MPROC's leftovers:

- ~~Effect events are still not decoded~~ — CLOSED 2026-08-16.
  `crates/axilog-core/src/evtc/effect.rs` decodes all three arcdps
  generations (45, 51 + its end form, and the split 60–63) into one
  `EffectEvent`. The catalog went 429 → 565 of 649 finders, and
  `isInstantCast` on the committed fixture went 9 → 84 distinct skills.
  The decoder is general (position, orientation, scale, dynamic end
  times). The replay items themselves (dev-notes #6/#8) were TRANSFORMATION
  / GLIDER / GADGETCAPTURE statechanges, not effects, and are now **CLOSED
  2026-08-16** — see `docs/ROADMAP.md`'s parked section and
  `docs/NATIVE-FORMAT.md`'s 1.x rules. They did not build on the effect
  decoder in the end.
- ~~`rotation` is still `AnimatedCastEvent`-only~~ — CLOSED 2026-08-16.
  `analysis::rotation::build` now reproduces the whole of GW2EI's
  `SingleActor.InitCastEvents`: animated casts + `instant_cast`'s
  synthesized casts + `CBTS_WEAPSWAP` rows, with the
  `ServerDelayConstant` replace-the-trailing-swap dedup. The finder pass
  runs once in `analyze` and is shared with `skill_map`. Calibration is
  per family — animated and weapon swaps EXACT, instant casts bounded at
  93.1% recovered — and the committed ei-json golden was regenerated
  UN-filtered (1,222 → 1,732 entries) to make that possible; see
  `rotation_golden.rs`'s module doc and the golden's own `_note`.
  Fell out of the merge: an ext-healing double-count in `instant_cast`
  (the missing `HealingStatsExtensionHandler.SanitizeForSrc` port), and
  `skill_map::PSEUDO_SKILL_NAMES` so negative pseudo ids get EI's names
  rather than `"Skill 4294967294"`.
- **6 `UsingNoAnimatedCastChecker` finders** are still skipped. That
  checker needs a cast window (start + actual duration), which
  `analysis::rotation` now builds — so this is newly REACHABLE, and is
  the obvious next step on the residual 6.9% instant-cast gap. The rest
  of that gap is GW2EI cast sources outside the finder family entirely
  (`SpecialCastEventProcess`, `ProfHelper.ComputeEndWithBuffApplyCastEvents`,
  the Engineer toolbelt helpers).

---

Assembled 2026-08-16 from memory files and the Phase B session. The SDD ledger
it partly draws on was deleted at the end of Phase B per the skill;
`git log b5ba6be..521abc9` is the surviving record if a detail needs checking.
