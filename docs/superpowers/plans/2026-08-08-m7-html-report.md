# axilog M7 — HTML Report Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development.

**Goal:** `--format html` emits a single self-contained dark-theme report (header, sortable
players/support/boons views, SVG timeline) with a structural golden test.

## Global Constraints

- Zero external requests in the emitted HTML (inline CSS/JS/data only); deterministic output.
- New crate `axilog-html` (report builder: `render(&Report) -> String`) so the CLI stays thin;
  template assets as `include_str!` files under the crate (`assets/report.css`, `report.js`,
  skeleton) — keep JS/CSS in their own files for readability, inlined at build time.
- All numbers rendered come from the embedded Report JSON at runtime (JS renders; Rust only
  embeds + skeleton) — one source of truth, no server-side duplication of formatting logic.
- Design: dark default, WvW-appropriate, information-dense; typography/spacing per
  frontend-design principles (distinctive, not bootstrap-default); team colors used semantically
  (red/blue/green accents). Avoid heavy frameworks — vanilla JS, <25KB combined CSS+JS.
- `cargo test --workspace` (188) + node 6 + python 9 stay green. MIT, warning-free.

---

### Task 1: axilog-html crate + skeleton + CLI wiring

**Files:** Create `crates/axilog-html/` (Cargo.toml, src/lib.rs, assets/{skeleton.html,report.css,report.js}); modify `crates/axilog-cli/src/main.rs` (Format::Html + `-o/--output` arg), workspace Cargo.toml.

**Requirements:** `render(&Report) -> String`: skeleton with `<script id="axilog-data" type="application/json">` embedding serde_json (HTML-escape `</script`-safe via `<` escaping — use serde_json's safe raw string or manual replace; verify XSS-safety for player names: names go through JSON-in-script only, and JS must use textContent, never innerHTML, for log-derived strings — state this contract in report.js comments). Header section rendered by JS from the data: map, duration mm:ss, teams (colored chips + player counts), recorded_by, commander (variant name when present), warnings banner when non-empty, tick-rate inline stat when present. CLI: `--format html` prints to stdout by default, `-o file` writes (any format). Structural test: output contains the data block, parses as valid JSON when extracted, contains header container ids; deterministic (two renders byte-equal). Verify by extracting and JSON-parsing in the test.

### Task 2: players/support/boons views (sortable)

**Files:** `crates/axilog-html/assets/report.js`, `report.css`; test additions in `crates/axilog-html/src/lib.rs` or tests/.

**Requirements:** Tab bar (Damage | Support | Boons). Damage: columns account, profession(+elite), damage, DPS, downs, kills, deaths, down-contrib, dmg taken; sortable (click header, asc/desc), default damage desc; squad-totals footer row; profession shown with elite-spec name. Support: cleanses/self/strips/res/stunbreaks(+s). Boons: Might avg, presence % others (one decimal), generation toggle (self/group/squad %). Number formatting: thousands separators; percentages 1dp. Non-squad players (subgroup 0) visually distinct (muted). Structural tests: the JS is static so tests assert the asset contains the column definitions and the HTML contains view containers; PLUS a DOM-level smoke: use Node (available) with a minimal DOM shim or regex-extract the rendered-data path — simplest honest check: run `node -e` executing report.js's pure functions (sorting/formatting) if they're factored as testable pure functions — REQUIRE that factoring (render logic pure functions + thin DOM glue) and test the pure functions via node.

### Task 3: SVG timeline + polish + goldens + docs

**Files:** assets, `crates/axilog-html/tests/golden_html.rs`, README, `.github/workflows/ci.yml` (only if a node step for JS-function tests is added — reuse existing node setup).

**Requirements:** Inline SVG chart from timeline.per_second: squad damage area/line, downs as markers, cc_applied as a secondary translucent overlay; axis labels (time mm:ss, damage k-format); responsive width; pure-function data→path generation tested in node. Golden structural test: render fixture report → assert calibrated values present in the embedded JSON (2138414, support sums), all containers, determinism, size budget (<250KB total for the fixture). README: html format usage + screenshot-free description; milestone entry. Final: run every suite (cargo 188+new, node, python) green.

## Self-Review
Three tasks; crate isolation keeps CLI thin; XSS contract stated; pure-function JS factoring
makes logic testable without a browser; determinism + size budgets are gates. No placeholders.
