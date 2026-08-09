# axilog — M15: Combat-replay positions in EI shape

**Status:** Approved (autonomous per docs/ROADMAP.md / [[axilog-autonomous-mandate]])
**Why:** Unblocks axibridge's replay map / heatmap / positioning features. M9 shipped native
replay tracks (calibrated 99.77% within 1 map pixel); M11 shipped EI `combatReplayData`
dead/down intervals (byte-exact) + activeTimes. What's missing is the rest of EI's replay
surface: fixed-rate `positions`/`orientations`, `dc` intervals, `start`/`end`/`iconURL`, and the
top-level `combatReplayMetaData` (inchToPixel / pollingRate / sizes / maps).

## Reference shape (verified against the local post-era EI JSON)

- Top-level `combatReplayMetaData`: `{ inchToPixel: 0.009, pollingRate: 300, sizes: [523, 750],
  maps: [{ url: "<imgur png>", interval: [0, <fight end>], position: [0, 0] }] }`.
- Per-player `combatReplayData`: `start`, `end` (ms awareness window), `iconURL` (profession
  icon), `positions` (list of `[x, y]` map-pixel floats on the fixed `pollingRate` grid — 1,159
  samples on the 348s reference log at 300ms), `orientations` (degrees, same length as
  positions), `dead`/`down` (SHIPPED in M11, byte-exact — must not change), `dc` (disconnect
  intervals bracketed by i64::MIN/i64::MAX sentinels around the awareness window).

## Scope

1. **EI fixed-rate positions + orientations + dc** — resample position (sc 19) and facing
   (sc 21) events onto EI's fixed `pollingRate` grid exactly the way GW2EI does (GW2EI source is
   the algorithm arbiter: verify its polling/interpolation — `GetCombatReplayPolledPositions`/
   interpolation semantics — and cite). `start`/`end` from the awareness window (same values
   activeTimes already uses). `dc` sentinel intervals matching EI's shape. Map-pixel transform
   reuses M9's proven per-map geometry.
2. **combatReplayMetaData + per-map table** — static per-WvW-map table (map id → inchToPixel,
   sizes, image url, world-rect for the transform) sourced from GW2EI's own map definitions
   (cite file/values). Covers at least: Eternal Battlegrounds, Green/Blue Alpine Borderlands,
   Red Desert Borderland (+ any map GW2EI defines for WvW, incl. Edge of the Mists if defined).
   `iconURL`: static per-profession/elite-spec icon URL table matching EI's (cite source).
3. **ei-json wiring + gating + docs** — decide the gate with measured numbers: `positions`/
   `orientations` are heavy (per-player arrays at 300ms) → expected to ride the existing
   `--replay` opt-in via Option-presence (precedent: rotation/timeseries); M11's always-on
   dead/down/activeTimes must remain always-on and unchanged. `combatReplayMetaData` is tiny —
   emit whenever the map is known and replay data is requested (match EI presence semantics;
   document). README parity rows; goldens extended.

## Calibration

Extend `fixtures/wvw-small.ei.json` (committed, from the local EI reference via the established
M13 route — axibridge's cached EI CLI, never uploads) with combatReplayData positions +
combatReplayMetaData; post-era spot-check vs `fixtures/local/wvw-postrework.ei.json` (READ-only).
Gates: metaData EXACT (inchToPixel/pollingRate/sizes/urls); `start`/`end`/`dc` EXACT; positions —
≥99% of samples within 1 map pixel of EI's, with outliers counted + documented (M9 precedent:
99.77%); orientations within a documented tolerance (EI rounds; verify its rounding). M11
dead/down stay byte-exact. All existing calibration exact; both eras sanity-checked.

## Non-goals

`wvWMapData` (shard/team ids + objectiveData capture timelines — needs GADGETCAPTURE tracking,
parked eye-candy territory; note as follow-up), HTML replay tab changes (native replay already
has its own), PvE/instance map tables (WvW-first), `orientations`-driven rendering.
