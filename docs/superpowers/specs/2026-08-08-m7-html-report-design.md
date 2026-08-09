# axilog — M7: Self-Contained HTML Report

**Status:** Approved (autonomous continuation authorized by user 2026-08-08)
**Why:** The original brainstorm deferred "HTML report or web UI after the CLI". A single-file
HTML report is the EI-familiar sharing format: one artifact, no server, no dependencies, works
from a file:// open or a Discord-shared upload.

## Scope

`axilog parse <log> --format html [-o out.html]` emits ONE self-contained HTML file:
- Native `Report` JSON embedded in a `<script type="application/json">` block; all rendering is
  inline vanilla JS + inline CSS. Zero external requests (works offline; CSP-friendly).
- **Header:** map name, duration, team colors + counts, recorded-by, commander (tag variant when
  known), tick-rate mini-line when present, era/warnings surfaced.
- **Players table** (default view): account/character, profession + elite spec, damage, DPS,
  downs/kills/deaths, down-contribution, damage taken — sortable columns, squad-total footer.
- **Support view:** cleanses/self, strips, resurrects, stunbreaks (+durations).
- **Boons view:** Might avg stacks, presence % for the remaining 11; generation self/group/squad
  toggle.
- **Timeline chart:** per-second squad damage with downs markers and CC overlay — inline SVG,
  no chart library.
- Dark theme default (WvW-night aesthetic), light toggle; readable, information-dense, not
  templated-bootstrap-looking. Deterministic output (no timestamps/randomness beyond log data).

## Gates
- Structural golden test: rendered HTML for the committed fixture contains the calibrated
  numbers (2,138,414 total, 41-42 players, support sums) and all view containers; deterministic
  byte-stable across runs.
- Report opens and renders correctly (implementers verify via headless screenshot or DOM
  assertions with a scriptable browser if available; else DOM-parse assertions).
- `cargo test --workspace` green; CLI/table/json/ei-json outputs unchanged.

## Non-goals
Web UI/server, combat replay map, comparison across logs, upload integration, the
mounts/glider/capping animations (need combat-replay positions — future milestone).
