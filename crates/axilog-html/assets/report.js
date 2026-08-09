/*
 * axilog WvW report — client-side renderer.
 *
 * XSS CONTRACT: every log-derived string (names, marker ids, team colors,
 * map name, warnings, profession/elite-spec, ...) MUST reach the DOM via
 * `Node.textContent` only — never `innerHTML`/`insertAdjacentHTML`/markup
 * concatenation. textContent can't be parsed as HTML, so it's safe even
 * for values containing a literal script-close tag string (never write
 * that sequence in *this file*, including comments — see the note at the
 * bottom). Rust independently makes the embedded JSON itself injection-
 * safe (axilog-html/src/lib.rs); this is the second, independent line of
 * defense.
 *
 * Structure: PURE functions (data in, plain view-model out, no DOM) plus
 * thin DOM glue that assigns textContent/classList from that view model —
 * keeps formatting/derivation/sorting testable from Node without a
 * browser (see tests/js_units.rs). Exported via a UMD wrapper below.
 */
(function (root, factory) {
  var axilogReport = factory();
  if (typeof module === "object" && module.exports) {
    module.exports = axilogReport;
  }
  if (root) {
    root.AxilogReport = axilogReport;
  }
})(typeof self !== "undefined" ? self : this, function () {
  "use strict";

  // ---- formatting (pure) ----------------------------------------------

  /** Group a non-negative digit string into "1,234,567"-style triples. */
  function groupDigits(digits) {
    var out = "";
    var count = 0;
    for (var i = digits.length - 1; i >= 0; i--) {
      out = digits.charAt(i) + out;
      count++;
      if (count % 3 === 0 && i !== 0) {
        out = "," + out;
      }
    }
    return out;
  }

  /** Format a number as an integer with thousands separators, e.g.
   * 1234567 -> "1,234,567". Rounds to the nearest integer first. */
  function formatThousands(n) {
    var num = Math.round(Number(n) || 0);
    var neg = num < 0;
    return (neg ? "-" : "") + groupDigits(String(Math.abs(num)));
  }

  /** Format a number to a fixed number of decimals, with thousands
   * separators on the integer part, e.g. formatFixed(1234.5, 1) ->
   * "1,234.5". */
  function formatFixed(n, decimals) {
    var num = Number(n) || 0;
    var neg = num < 0;
    var fixed = Math.abs(num).toFixed(decimals);
    var dot = fixed.indexOf(".");
    var intPart = dot === -1 ? fixed : fixed.slice(0, dot);
    var fracPart = dot === -1 ? "" : fixed.slice(dot);
    return (neg ? "-" : "") + groupDigits(intPart) + fracPart;
  }

  /** One-decimal percentage, e.g. formatPercent(33.333) -> "33.3%". */
  function formatPercent(n) {
    return formatFixed(n, 1) + "%";
  }

  /** Two-decimal average-stack count, e.g. formatStacks(12.3456) ->
   * "12.35". */
  function formatStacks(n) {
    return formatFixed(n, 2);
  }

  /** One-decimal seconds from a millisecond duration, e.g.
   * formatSecondsFromMs(4500) -> "4.5". */
  function formatSecondsFromMs(ms) {
    return formatFixed((Number(ms) || 0) / 1000, 1);
  }

  /** Format a millisecond duration as "mm:ss" (seconds floored). */
  function formatDuration(ms) {
    var totalSeconds = Math.floor((Number(ms) || 0) / 1000);
    var mm = Math.floor(totalSeconds / 60);
    var ss = totalSeconds % 60;
    return pad2(mm) + ":" + pad2(ss);
  }

  function pad2(n) {
    return n < 10 ? "0" + n : String(n);
  }

  /** k-format a number for compact axis labels, e.g. formatK(45000) ->
   * "45k", formatK(45231) -> "45.2k" (one decimal only when the value
   * isn't a whole number of thousands), formatK(999) -> "999". */
  function formatK(n) {
    var num = Math.round(Number(n) || 0);
    var neg = num < 0;
    var abs = Math.abs(num);
    var out;
    if (abs >= 1000) {
      var rounded = Math.round((abs / 1000) * 10) / 10;
      out = (Number.isInteger(rounded) ? String(rounded) : rounded.toFixed(1)) + "k";
    } else {
      out = String(abs);
    }
    return (neg ? "-" : "") + out;
  }

  // ---- sorting (pure) ---------------------------------------------------

  /**
   * Stable sort `rows` by the value of `key` on each row. Direction is
   * "asc" or "desc" (defaults to "asc" for anything else). Stability is
   * explicit (an index tie-break), not relied upon from the host engine,
   * so behavior is identical everywhere and is what the node tests pin.
   * Pure: returns a new array, never mutates `rows`.
   */
  function sortRows(rows, key, direction) {
    var dir = direction === "desc" ? -1 : 1;
    var withIndex = (rows || []).map(function (r, i) {
      return { r: r, i: i };
    });
    withIndex.sort(function (a, b) {
      var av = a.r[key];
      var bv = b.r[key];
      if (av < bv) return -1 * dir;
      if (av > bv) return 1 * dir;
      return a.i - b.i;
    });
    return withIndex.map(function (x) {
      return x.r;
    });
  }

  // ---- team chips (pure) -------------------------------------------------

  /** Known team colors get their own CSS class; anything else falls back
   * to "unknown" so future/uncommon team colors still render sanely. */
  var KNOWN_TEAM_COLORS = { red: true, blue: true, green: true };
  function teamCssClass(color) {
    return KNOWN_TEAM_COLORS.hasOwnProperty(color) ? color : "unknown";
  }

  /**
   * One chip per `encounter.teams`: squad players (`players[].team`) take
   * priority, falling back to counting `enemies[]` of that color when a
   * team had no squad players — e.g. "green · 42 squad" (the recording
   * squad's own team) vs "blue · 18 enemies". Teams with zero on both
   * sides are dropped rather than shown as "· 0". Pure — no DOM.
   */
  function buildTeamChips(encounter, players, enemies) {
    var squadCounts = {};
    (players || []).forEach(function (p) {
      squadCounts[p.team] = (squadCounts[p.team] || 0) + 1;
    });
    var enemyCounts = {};
    (enemies || []).forEach(function (e) {
      enemyCounts[e.team] = (enemyCounts[e.team] || 0) + 1;
    });
    return (encounter.teams || [])
      .map(function (t) {
        var squad = squadCounts[t.color] || 0;
        var enemy = enemyCounts[t.color] || 0;
        return {
          color: t.color,
          cssClass: teamCssClass(t.color),
          count: squad > 0 ? squad : enemy,
          kind: squad > 0 ? "squad" : "enemies",
          total: squad + enemy,
        };
      })
      .filter(function (t) {
        return t.total > 0;
      });
  }

  /** Find the (first) commander among `players`, if any. */
  function findCommander(players) {
    return (players || []).filter(function (p) {
      return p.commander;
    })[0] || null;
  }

  /**
   * Derive the full header view model from a parsed Report. Pure — every
   * field here is a plain string/number/array/null ready for thin DOM
   * glue to assign via textContent.
   */
  function buildHeaderModel(report) {
    var encounter = report.encounter;
    var commander = findCommander(report.players);
    var tickRate = encounter.tick_rate;
    return {
      map: encounter.map,
      duration: formatDuration(encounter.duration_ms),
      teams: buildTeamChips(encounter, report.players, report.enemies),
      recordedBy: encounter.recorded_by || null,
      commander: commander
        ? {
            account: commander.account,
            variant: commander.commander_tag ? commander.commander_tag.variant : null,
          }
        : null,
      warnings: report.warnings || [],
      tickRateAvg: tickRate ? tickRate.avg : null,
    };
  }

  // ---- damage view (pure) ------------------------------------------------

  /** Profession display: elite-spec name when the player has one (e.g.
   * "Firebrand"), otherwise the base profession (e.g. "Guardian"). */
  function professionDisplay(p) {
    return p.elite_spec ? p.elite_spec : p.profession;
  }

  var DAMAGE_COLUMNS = [
    { key: "account", label: "Account" },
    { key: "character", label: "Character" },
    { key: "professionDisplay", label: "Profession" },
    { key: "damage", label: "Damage" },
    { key: "dps", label: "DPS" },
    { key: "downs", label: "Downs" },
    { key: "kills", label: "Kills" },
    { key: "deaths", label: "Deaths" },
    { key: "downContribution", label: "Down Contrib." },
    { key: "damageTaken", label: "Dmg Taken" },
  ];

  var DAMAGE_DEFAULT_SORT = { key: "damage", direction: "desc" };

  /** Build one damage-view row per player. Pure — no DOM. */
  function buildDamageRows(players) {
    return (players || []).map(function (p) {
      return {
        account: p.account,
        character: p.character,
        profession: p.profession,
        eliteSpec: p.elite_spec || null,
        professionDisplay: professionDisplay(p),
        nonSquad: p.subgroup === 0,
        damage: p.damage.total,
        dps: p.damage.dps,
        downs: p.downs_dealt,
        kills: p.kills_dealt,
        deaths: p.deaths,
        downContribution: p.down_contribution,
        damageTaken: p.damage_taken,
      };
    });
  }

  /** Squad-totals footer row: sums every numeric damage-view column
   * across every row (DPS is summed too — squad DPS, not an average).
   * Pure — no DOM. */
  function buildDamageTotals(rows) {
    var totals = {
      damage: 0,
      dps: 0,
      downs: 0,
      kills: 0,
      deaths: 0,
      downContribution: 0,
      damageTaken: 0,
    };
    (rows || []).forEach(function (r) {
      totals.damage += r.damage;
      totals.dps += r.dps;
      totals.downs += r.downs;
      totals.kills += r.kills;
      totals.deaths += r.deaths;
      totals.downContribution += r.downContribution;
      totals.damageTaken += r.damageTaken;
    });
    return totals;
  }

  var DAMAGE_FORMATTERS = {
    account: function (v) { return v; },
    character: function (v) { return v; },
    professionDisplay: function (v) { return v; },
    damage: formatThousands,
    dps: function (v) { return formatThousands(v); },
    downs: formatThousands,
    kills: formatThousands,
    deaths: formatThousands,
    downContribution: formatThousands,
    damageTaken: formatThousands,
  };

  // ---- support view (pure) -----------------------------------------------

  var SUPPORT_COLUMNS = [
    { key: "account", label: "Account" },
    { key: "professionDisplay", label: "Profession" },
    { key: "cleanses", label: "Cleanses" },
    { key: "cleansesSelf", label: "Self-Cleanses" },
    { key: "strips", label: "Strips" },
    { key: "resurrects", label: "Resurrects" },
    { key: "stunBreaks", label: "Stunbreaks" },
    { key: "removedStunSeconds", label: "Removed Stun (s)" },
  ];

  var SUPPORT_DEFAULT_SORT = { key: "cleanses", direction: "desc" };

  /** Build one support-view row per player. Pure — no DOM. */
  function buildSupportRows(players) {
    return (players || []).map(function (p) {
      return {
        account: p.account,
        profession: p.profession,
        eliteSpec: p.elite_spec || null,
        professionDisplay: professionDisplay(p),
        nonSquad: p.subgroup === 0,
        cleanses: p.support.cleanses,
        cleansesSelf: p.support.cleanses_self,
        strips: p.support.strips,
        resurrects: p.support.resurrects,
        stunBreaks: p.cc.stun_breaks,
        removedStunSeconds: (Number(p.cc.removed_stun_duration_ms) || 0) / 1000,
      };
    });
  }

  var SUPPORT_FORMATTERS = {
    account: function (v) { return v; },
    professionDisplay: function (v) { return v; },
    cleanses: formatThousands,
    cleansesSelf: formatThousands,
    strips: formatThousands,
    resurrects: formatThousands,
    stunBreaks: formatThousands,
    removedStunSeconds: function (v) { return formatFixed(v, 1); },
  };

  // ---- boons view (pure) --------------------------------------------------

  /** Boons shown as a plain whole-fight presence %, one decimal. */
  var PRESENCE_BOON_NAMES = [
    "Quickness",
    "Alacrity",
    "Stability",
    "Protection",
    "Fury",
    "Resistance",
  ];

  /** Boons whose *generation* (self/group/squad %) column swaps with the
   * generation-mode segmented control. */
  var GENERATION_BOON_NAMES = ["Might", "Quickness", "Alacrity", "Stability"];

  var GENERATION_MODES = ["self", "group", "squad"];
  var GENERATION_DEFAULT_MODE = "group";

  function findBoon(boons, name) {
    return (
      (boons || []).filter(function (b) {
        return b.name === name;
      })[0] || null
    );
  }

  /** Pull the generation percentage matching `mode` ("self"/"group"/
   * "squad") off a single boon entry; 0 when the boon/mode is missing. */
  function generationValue(boon, mode) {
    if (!boon || !boon.generation) return 0;
    if (mode === "self") return boon.generation.self_pct;
    if (mode === "squad") return boon.generation.squad_pct;
    return boon.generation.group_pct;
  }

  function boonColumnKey(name) {
    return "presence" + name;
  }
  function generationColumnKey(name) {
    return "gen" + name;
  }

  var BOON_COLUMNS = [
    { key: "account", label: "Account" },
    { key: "professionDisplay", label: "Profession" },
    { key: "mightAvg", label: "Might (avg)" },
  ]
    .concat(
      PRESENCE_BOON_NAMES.map(function (name) {
        return { key: boonColumnKey(name), label: name };
      })
    )
    .concat(
      GENERATION_BOON_NAMES.map(function (name) {
        return { key: generationColumnKey(name), label: name + " Gen." };
      })
    );

  var BOON_DEFAULT_SORT = { key: "account", direction: "asc" };

  /**
   * Build one boons-view row per player for the given generation `mode`
   * ("self"/"group"/"squad", defaults to "group"). Pure — no DOM. Call
   * again (with a different mode) whenever the generation-mode toggle
   * changes; there is no separate "update in place" path.
   */
  function buildBoonRows(players, mode) {
    var m = GENERATION_MODES.indexOf(mode) === -1 ? GENERATION_DEFAULT_MODE : mode;
    return (players || []).map(function (p) {
      var might = findBoon(p.boons, "Might");
      var row = {
        account: p.account,
        profession: p.profession,
        eliteSpec: p.elite_spec || null,
        professionDisplay: professionDisplay(p),
        nonSquad: p.subgroup === 0,
        mightAvg: (might && might.avg_stacks) || 0,
      };
      PRESENCE_BOON_NAMES.forEach(function (name) {
        var b = findBoon(p.boons, name);
        row[boonColumnKey(name)] = b ? b.presence_pct : 0;
      });
      GENERATION_BOON_NAMES.forEach(function (name) {
        row[generationColumnKey(name)] = generationValue(findBoon(p.boons, name), m);
      });
      return row;
    });
  }

  var BOON_FORMATTERS = (function () {
    var f = {
      account: function (v) { return v; },
      professionDisplay: function (v) { return v; },
      mightAvg: formatStacks,
    };
    PRESENCE_BOON_NAMES.forEach(function (name) {
      f[boonColumnKey(name)] = formatPercent;
    });
    GENERATION_BOON_NAMES.forEach(function (name) {
      f[generationColumnKey(name)] = formatPercent;
    });
    return f;
  })();

  // ---- timeline (pure) ----------------------------------------------------

  /** Logical SVG canvas size (viewBox units). The element itself is
   * responsive (`width: 100%` in CSS over a fixed viewBox, see
   * `renderTimeline`), so these are just the coordinate space `
   * buildTimelinePaths` computes pixel positions in -- not real pixels. */
  var TIMELINE_WIDTH = 960;
  var TIMELINE_HEIGHT = 260;
  var TIMELINE_MARGIN = { top: 12, right: 16, bottom: 28, left: 48 };

  /**
   * Derive every plottable coordinate for the damage timeline from
   * `timeline.per_second` (`{squad_damage[], cc_applied[], downs[]}`, one
   * entry per second, all three arrays the same length) plus a target
   * `w`/`h` (SVG viewBox size). Pure -- no DOM, so it's directly node-
   * testable (see tests/js/pure_fn_tests.mjs).
   *
   * Returns `{areaPath, linePath, downMarkers, ccBars, xTicks, yTicks}`:
   * - `areaPath`/`linePath`: SVG `<path d>` strings for the squad-damage
   *   area (filled, primary series) and its outline, one point per second,
   *   x evenly spaced across the plot width, y linear-scaled 0..max(squad
   *   damage) against the plot height.
   * - `downMarkers`: `{x, y, second, count}` per second with >=1 down --
   *   placed ON the damage line (y = that second's damage y-position, not
   *   a separate scale), per the design brief ("downs as circular markers
   *   on the line at their second").
   * - `ccBars`: `{x, y, width, height, second, value}` per second with
   *   cc_applied > 0. DESIGN CHOICE: normalized to cc_applied's OWN max
   *   (not a shared/secondary y-axis against damage) -- CC-applied and
   *   damage are different units on very different scales, so sharing one
   *   linear axis would flatten the (usually much smaller) CC series to
   *   near-invisible slivers. Rendered as a translucent bar overlay behind
   *   the damage area so both series stay legible at a glance without a
   *   second axis to read.
   * - `xTicks`: `{x, second, label}`, label is `mm:ss` (one index == one
   *   second, matching the `per_second` field name), a sensible fixed
   *   count (<=6, less if there are fewer seconds than that).
   * - `yTicks`: `{y, value, label}`, 5 evenly-spaced ticks from 0 to
   *   max(squad damage), label k-formatted (see `formatK`).
   *
   * Degenerate inputs are handled explicitly rather than left to produce
   * NaN/Infinity: zero seconds -> every array empty, both path strings
   * empty. Exactly one second -> the single point is centered in the plot
   * width (an x-scale needs >=2 points to have a "span"); the line path
   * is a lone `M` moveto (no `L` segment -- there's nothing to connect).
   */
  function buildTimelinePaths(perSecond, w, h) {
    var squad = (perSecond && perSecond.squad_damage) || [];
    var cc = (perSecond && perSecond.cc_applied) || [];
    var downs = (perSecond && perSecond.downs) || [];
    var n = squad.length;

    var m = TIMELINE_MARGIN;
    var plotW = Math.max(0, w - m.left - m.right);
    var plotH = Math.max(0, h - m.top - m.bottom);
    var baseline = m.top + plotH;

    if (n === 0) {
      return { areaPath: "", linePath: "", downMarkers: [], ccBars: [], xTicks: [], yTicks: [] };
    }

    var yMax = 0;
    for (var i = 0; i < n; i++) {
      if (squad[i] > yMax) yMax = squad[i];
    }
    if (yMax <= 0) yMax = 1; // avoid /0 -- an all-zero fight still renders a flat baseline.

    var ccMax = 0;
    for (var j = 0; j < cc.length; j++) {
      if (cc[j] > ccMax) ccMax = cc[j];
    }
    if (ccMax <= 0) ccMax = 1;

    function xAt(idx) {
      return n === 1 ? m.left + plotW / 2 : m.left + (idx / (n - 1)) * plotW;
    }
    function yAt(value) {
      return baseline - (value / yMax) * plotH;
    }

    var linePoints = [];
    for (var k = 0; k < n; k++) {
      linePoints.push(xAt(k) + "," + yAt(squad[k]));
    }
    var linePath = "M" + linePoints.join("L");
    var areaPath = linePath + "L" + xAt(n - 1) + "," + baseline + "L" + xAt(0) + "," + baseline + "Z";

    var downMarkers = [];
    for (var d = 0; d < n; d++) {
      if (downs[d] > 0) {
        downMarkers.push({ x: xAt(d), y: yAt(squad[d]), second: d, count: downs[d] });
      }
    }

    var barW = n === 1 ? Math.min(24, plotW) : ((plotW / (n - 1)) * 0.6) || 0;
    var ccBars = [];
    for (var c = 0; c < n; c++) {
      if (cc[c] > 0) {
        var bh = (cc[c] / ccMax) * plotH;
        ccBars.push({ x: xAt(c) - barW / 2, y: baseline - bh, width: barW, height: bh, second: c, value: cc[c] });
      }
    }

    var tickCount = Math.min(6, n);
    var xTicks = [];
    for (var t = 0; t < tickCount; t++) {
      var idx = tickCount === 1 ? 0 : Math.round((t * (n - 1)) / (tickCount - 1));
      xTicks.push({ x: xAt(idx), second: idx, label: formatDuration(idx * 1000) });
    }

    var yTickCount = 5;
    var yTicks = [];
    for (var y = 0; y < yTickCount; y++) {
      var value = (yMax * y) / (yTickCount - 1);
      yTicks.push({ y: yAt(value), value: value, label: formatK(value) });
    }

    return { areaPath: areaPath, linePath: linePath, downMarkers: downMarkers, ccBars: ccBars, xTicks: xTicks, yTicks: yTicks };
  }

  // ---- replay (pure) -----------------------------------------------------
  // Replay tab math, kept DOM-free (node-testable, see pure_fn_tests.mjs).
  // `renderReplayView` further below is the thin rAF-driven DOM glue.

  /** Gate deciding whether the Replay tab appears: "rendered ONLY when
   * data.replay present". Shared by `initTabs` and the node tests. */
  function hasReplay(report) {
    return !!(report && report.replay && report.replay.tracks && report.replay.tracks.length > 0);
  }

  /** How long (ms) a dot fades out after its track's last sample, once the
   * playhead has moved past it. */
  var REPLAY_FADE_MS = 3000;

  /** Pad `bounds` 5% into an SVG `viewBox`-shaped `{x,y,width,height}`.
   * Degenerate bounds (width/height <= 0) fall back to a 1x1 span first,
   * so the stage never collapses to a zero-area viewBox. */
  function replayViewBox(bounds) {
    var b = bounds || { min_x: 0, min_y: 0, max_x: 0, max_y: 0 };
    var w = b.max_x - b.min_x;
    var h = b.max_y - b.min_y;
    if (!(w > 0)) w = 1;
    if (!(h > 0)) h = 1;
    var padX = w * 0.05;
    var padY = h * 0.05;
    return { x: b.min_x - padX, y: b.min_y - padY, width: w + padX * 2, height: h + padY * 2 };
  }

  /** Opacity for a dot holding position `lastT` once playhead `t` has
   * passed it: 1 up to `lastT`, linear fade to 0 over `REPLAY_FADE_MS`. */
  function replayFadeOpacity(t, lastT) {
    if (t <= lastT) return 1;
    var elapsed = t - lastT;
    if (elapsed >= REPLAY_FADE_MS) return 0;
    return 1 - elapsed / REPLAY_FADE_MS;
  }

  /** Position (+ opacity) for one track at time `t` (ms): interpolates
   * between the `[t_ms, x, y]` samples bracketing `t`, `null` for an
   * empty track. Holds the first sample before it, the last (fading via
   * `replayFadeOpacity`) at/after it. */
  function replayPositionAt(track, t) {
    var samples = (track && track.samples) || [];
    var n = samples.length;
    if (n === 0) return null;

    var first = samples[0];
    if (t <= first[0]) return { x: first[1], y: first[2], opacity: 1 };

    var last = samples[n - 1];
    if (t >= last[0]) {
      return { x: last[1], y: last[2], opacity: replayFadeOpacity(t, last[0]) };
    }

    for (var i = 0; i < n - 1; i++) {
      var a = samples[i];
      var b = samples[i + 1];
      if (t >= a[0] && t <= b[0]) {
        var span = b[0] - a[0];
        var frac = span === 0 ? 0 : (t - a[0]) / span;
        return { x: a[1] + (b[1] - a[1]) * frac, y: a[2] + (b[2] - a[2]) * frac, opacity: 1 };
      }
    }
    return { x: last[1], y: last[2], opacity: replayFadeOpacity(t, last[0]) }; // unreachable
  }

  /** `replayPositionAt` mapped over every track; the one function the
   * animation loop calls every frame. */
  function positionsAt(tracks, t) {
    return (tracks || []).map(function (track) {
      return replayPositionAt(track, t);
    });
  }

  /** True when `t` falls inside any `[start_ms, end_ms]` pair (inclusive
   * both ends). Shared by `isDownAt`/`isDeadAt`. */
  function withinAnyReplayInterval(intervals, t) {
    return (intervals || []).some(function (iv) {
      return t >= iv[0] && t <= iv[1];
    });
  }

  function isDownAt(track, t) {
    return withinAnyReplayInterval(track && track.down_intervals, t);
  }

  function isDeadAt(track, t) {
    return withinAnyReplayInterval(track && track.dead_intervals, t);
  }

  var REPLAY_SPEEDS = [1, 4, 8];
  var REPLAY_DEFAULT_SPEED = 4;

  // ---- DOM glue --------------------------------------------------------

  function byId(id) {
    return document.getElementById(id);
  }

  function renderTeamChips(container, teams) {
    container.textContent = "";
    teams.forEach(function (t) {
      var chip = document.createElement("span");
      chip.className = "axilog-team-chip team-" + t.cssClass;
      chip.textContent = t.color + " · " + t.count + " " + t.kind;
      container.appendChild(chip);
    });
  }

  function renderWarnings(container, warnings) {
    if (!warnings || warnings.length === 0) {
      container.hidden = true;
      container.textContent = "";
      return;
    }
    container.hidden = false;
    container.textContent = "";
    var heading = document.createElement("strong");
    heading.textContent = "Warnings";
    container.appendChild(heading);
    var list = document.createElement("ul");
    warnings.forEach(function (w) {
      var li = document.createElement("li");
      li.textContent = w;
      list.appendChild(li);
    });
    container.appendChild(list);
  }

  function renderStat(el, value) {
    if (value === null || value === undefined) {
      el.hidden = true;
      el.textContent = "";
      return;
    }
    el.hidden = false;
    el.textContent = value;
  }

  function renderHeader(report) {
    var model = buildHeaderModel(report);

    byId("axilog-map").textContent = model.map;
    byId("axilog-duration").textContent = "Duration: " + model.duration;

    renderStat(
      byId("axilog-recorded-by"),
      model.recordedBy ? "Recorded by: " + model.recordedBy : null
    );

    renderStat(
      byId("axilog-commander"),
      model.commander
        ? "Commander: " +
            model.commander.account +
            (model.commander.variant ? " (" + model.commander.variant + ")" : "")
        : null
    );

    renderStat(
      byId("axilog-tick-rate"),
      model.tickRateAvg !== null ? "Tick rate: " + model.tickRateAvg.toFixed(1) + "/s" : null
    );

    renderWarnings(byId("axilog-warnings"), model.warnings);
    renderTeamChips(byId("axilog-teams"), model.teams);
  }

  /**
   * Sortable `<table>` for one view. `getRows()` re-derives rows on every
   * rebuild (sort click, or an external mode change) without this
   * function needing to know why. `sortState` (`{key, direction}`) is
   * mutated in place so the current sort survives a rebuild. Optional
   * `buildFootRow(rows)` returns a same-keyed object rendered as
   * `<tfoot>`.
   */
  function renderSortableTable(container, columns, formatters, getRows, sortState, buildFootRow) {
    container.textContent = "";
    var table = document.createElement("table");
    table.className = "axilog-table";

    var thead = document.createElement("thead");
    var headRow = document.createElement("tr");
    // `headerCells` pairs each column's <th> (the ARIA-correct home for
    // `aria-sort` -- see the WAI-ARIA 1.2 spec's `aria-sort` entry, which
    // defines the attribute on a columnheader/th, not an inner button)
    // with its sort button/arrow, so `updateSortIndicators` below doesn't
    // need a DOM query round-trip per rebuild.
    var headerCells = [];
    columns.forEach(function (col) {
      var th = document.createElement("th");
      var btn = document.createElement("button");
      btn.type = "button";
      btn.className = "axilog-sort-btn";
      var label = document.createElement("span");
      label.textContent = col.label;
      btn.appendChild(label);
      var arrow = document.createElement("span");
      arrow.className = "axilog-sort-arrow";
      arrow.setAttribute("aria-hidden", "true");
      btn.appendChild(arrow);
      btn.addEventListener("click", function () {
        if (sortState.key === col.key) {
          sortState.direction = sortState.direction === "asc" ? "desc" : "asc";
        } else {
          sortState.key = col.key;
          sortState.direction = "asc";
        }
        rebuild();
      });
      th.appendChild(btn);
      headRow.appendChild(th);
      headerCells.push({ key: col.key, th: th, arrow: arrow });
    });
    thead.appendChild(headRow);
    table.appendChild(thead);

    var tbody = document.createElement("tbody");
    table.appendChild(tbody);

    var tfoot = null;
    if (buildFootRow) {
      tfoot = document.createElement("tfoot");
      table.appendChild(tfoot);
    }

    container.appendChild(table);

    function updateSortIndicators() {
      headerCells.forEach(function (cell) {
        if (cell.key === sortState.key) {
          cell.th.setAttribute("aria-sort", sortState.direction === "asc" ? "ascending" : "descending");
          cell.arrow.textContent = sortState.direction === "asc" ? "↑" : "↓";
        } else {
          cell.th.removeAttribute("aria-sort");
          cell.arrow.textContent = "";
        }
      });
    }

    function rebuild() {
      var rows = sortRows(getRows(), sortState.key, sortState.direction);

      tbody.textContent = "";
      rows.forEach(function (row) {
        var tr = document.createElement("tr");
        if (row.nonSquad) {
          tr.className = "axilog-row-muted";
        }
        columns.forEach(function (col) {
          var td = document.createElement("td");
          var raw = row[col.key];
          var fmt = formatters[col.key];
          td.textContent = fmt ? fmt(raw) : raw;
          if (col.key === "professionDisplay" && row.eliteSpec) {
            td.className = "axilog-badge-elite-cell";
          }
          tr.appendChild(td);
        });
        tbody.appendChild(tr);
      });

      if (tfoot) {
        tfoot.textContent = "";
        var footValues = buildFootRow(rows);
        var footTr = document.createElement("tr");
        columns.forEach(function (col, idx) {
          var td = document.createElement("td");
          if (idx === 0) {
            td.textContent = "Squad total";
          } else if (Object.prototype.hasOwnProperty.call(footValues, col.key)) {
            var fmt = formatters[col.key];
            var raw = footValues[col.key];
            td.textContent = fmt ? fmt(raw) : raw;
          } else {
            td.textContent = "";
          }
          footTr.appendChild(td);
        });
        footTr.className = "axilog-foot-row";
        tfoot.appendChild(footTr);
      }

      updateSortIndicators();
    }

    rebuild();
  }

  function renderDamageView(container, report) {
    var sortState = { key: DAMAGE_DEFAULT_SORT.key, direction: DAMAGE_DEFAULT_SORT.direction };
    renderSortableTable(
      container,
      DAMAGE_COLUMNS,
      DAMAGE_FORMATTERS,
      function () {
        return buildDamageRows(report.players);
      },
      sortState,
      buildDamageTotals
    );
  }

  function renderSupportView(container, report) {
    var sortState = { key: SUPPORT_DEFAULT_SORT.key, direction: SUPPORT_DEFAULT_SORT.direction };
    renderSortableTable(
      container,
      SUPPORT_COLUMNS,
      SUPPORT_FORMATTERS,
      function () {
        return buildSupportRows(report.players);
      },
      sortState,
      null
    );
  }

  function renderBoonsView(container, report) {
    var sortState = { key: BOON_DEFAULT_SORT.key, direction: BOON_DEFAULT_SORT.direction };
    var mode = GENERATION_DEFAULT_MODE;

    var toggleWrap = document.createElement("div");
    toggleWrap.className = "axilog-segmented";
    toggleWrap.setAttribute("role", "group");
    toggleWrap.setAttribute("aria-label", "Boon generation mode");

    var tableWrap = document.createElement("div");

    GENERATION_MODES.forEach(function (m) {
      var btn = document.createElement("button");
      btn.type = "button";
      btn.className = "axilog-segmented-btn" + (m === mode ? " is-active" : "");
      btn.textContent = m.charAt(0).toUpperCase() + m.slice(1);
      btn.setAttribute("aria-pressed", m === mode ? "true" : "false");
      btn.addEventListener("click", function () {
        if (mode === m) return;
        mode = m;
        var buttons = toggleWrap.querySelectorAll(".axilog-segmented-btn");
        for (var i = 0; i < buttons.length; i++) {
          buttons[i].classList.remove("is-active");
          buttons[i].setAttribute("aria-pressed", "false");
        }
        btn.classList.add("is-active");
        btn.setAttribute("aria-pressed", "true");
        rebuildTable();
      });
      toggleWrap.appendChild(btn);
    });

    container.textContent = "";
    container.appendChild(toggleWrap);
    container.appendChild(tableWrap);

    function rebuildTable() {
      renderSortableTable(
        tableWrap,
        BOON_COLUMNS,
        BOON_FORMATTERS,
        function () {
          return buildBoonRows(report.players, mode);
        },
        sortState,
        null
      );
    }

    rebuildTable();
  }

  var SVG_NS = "http://www.w3.org/2000/svg";

  /** Create a namespaced SVG element with the given presentation
   * attributes set. Thin DOM glue -- no derivation happens here, only
   * assignment of values `buildTimelinePaths` already computed. */
  function svgEl(name, attrs) {
    var el = document.createElementNS(SVG_NS, name);
    if (attrs) {
      Object.keys(attrs).forEach(function (key) {
        el.setAttribute(key, attrs[key]);
      });
    }
    return el;
  }

  /**
   * Render the squad-damage timeline as an inline, responsive SVG (fixed
   * `TIMELINE_WIDTH`x`TIMELINE_HEIGHT` viewBox, `width: 100%` in CSS so it
   * scales to the container) into `container`, from
   * `report.timeline.per_second`. All colors come from CSS custom
   * properties via class names (see report.css's `.axilog-timeline-*`
   * rules) so the chart re-themes with the existing dark/light toggle
   * without any JS involvement.
   */
  function renderTimeline(container, report) {
    container.textContent = "";

    var perSecond = report.timeline && report.timeline.per_second;
    if (!perSecond || !perSecond.squad_damage || perSecond.squad_damage.length === 0) {
      var empty = document.createElement("p");
      empty.className = "axilog-timeline-empty";
      empty.textContent = "No timeline data for this encounter.";
      container.appendChild(empty);
      return;
    }

    var paths = buildTimelinePaths(perSecond, TIMELINE_WIDTH, TIMELINE_HEIGHT);
    var m = TIMELINE_MARGIN;

    var svg = svgEl("svg", {
      viewBox: "0 0 " + TIMELINE_WIDTH + " " + TIMELINE_HEIGHT,
      width: "100%",
      height: TIMELINE_HEIGHT,
      class: "axilog-timeline-svg",
      role: "img",
      "aria-label": "Squad damage over time, with downs and CC-applied overlay",
    });

    // y-axis gridlines + labels (drawn first, furthest back).
    paths.yTicks.forEach(function (t) {
      svg.appendChild(
        svgEl("line", {
          x1: m.left,
          x2: TIMELINE_WIDTH - m.right,
          y1: t.y,
          y2: t.y,
          class: "axilog-timeline-gridline",
        })
      );
      var yLabel = svgEl("text", {
        x: m.left - 8,
        y: t.y,
        class: "axilog-timeline-axis-label",
        "text-anchor": "end",
        "dominant-baseline": "middle",
      });
      yLabel.textContent = t.label;
      svg.appendChild(yLabel);
    });

    // cc_applied translucent bar overlay, behind the damage area/line.
    paths.ccBars.forEach(function (bar) {
      var rect = svgEl("rect", {
        x: bar.x,
        y: bar.y,
        width: bar.width,
        height: bar.height,
        class: "axilog-timeline-cc-bar",
      });
      var title = svgEl("title", {});
      title.textContent =
        "CC applied at " + formatDuration(bar.second * 1000) + ": " + formatThousands(bar.value);
      rect.appendChild(title);
      svg.appendChild(rect);
    });

    // squad damage: filled area (primary series) + outline.
    if (paths.areaPath) {
      svg.appendChild(svgEl("path", { d: paths.areaPath, class: "axilog-timeline-area" }));
    }
    if (paths.linePath) {
      svg.appendChild(svgEl("path", { d: paths.linePath, class: "axilog-timeline-line" }));
    }

    // downs markers, on top of the line.
    paths.downMarkers.forEach(function (mk) {
      var circle = svgEl("circle", {
        cx: mk.x,
        cy: mk.y,
        r: 4,
        class: "axilog-timeline-down-marker",
      });
      var title = svgEl("title", {});
      title.textContent =
        "Down at " + formatDuration(mk.second * 1000) + (mk.count > 1 ? " (x" + mk.count + ")" : "");
      circle.appendChild(title);
      svg.appendChild(circle);
    });

    // x-axis labels.
    paths.xTicks.forEach(function (t) {
      var xLabel = svgEl("text", {
        x: t.x,
        y: TIMELINE_HEIGHT - m.bottom + 16,
        class: "axilog-timeline-axis-label",
        "text-anchor": "middle",
      });
      xLabel.textContent = t.label;
      svg.appendChild(xLabel);
    });

    container.appendChild(svg);
  }

  /** `team-<color>` (+ `is-enemy`) class for a dot, reusing `teamCssClass`
   * for the same unknown-color fallback as the header chips. */
  function replayMarkerClass(track) {
    return "axilog-replay-marker team-" + teamCssClass(track.team) + (track.is_squad ? "" : " is-enemy");
  }

  /** Animated Replay tab: play/pause + scrub + mm:ss readout + 1x/4x/8x
   * speed over an inline SVG stage (squad filled, enemy hollow, commander
   * gold ring, pulsing red ring while `isDownAt`, 25% opacity while
   * `isDeadAt`). Lazily built once; thin glue driving `positionsAt`. */
  function renderReplayView(container, report) {
    container.textContent = "";

    var replay = report.replay;
    if (!replay || !replay.tracks || replay.tracks.length === 0) {
      var empty = document.createElement("p");
      empty.className = "axilog-timeline-empty";
      empty.textContent = "No replay data for this encounter.";
      container.appendChild(empty);
      return;
    }

    var tracks = replay.tracks, durationMs = report.encounter.duration_ms, vb = replayViewBox(replay.bounds);

    var controls = document.createElement("div");
    controls.className = "axilog-replay-controls";

    var playBtn = document.createElement("button");
    playBtn.type = "button";
    playBtn.className = "axilog-replay-btn";
    playBtn.textContent = "Play";

    var scrub = document.createElement("input");
    scrub.type = "range";
    scrub.className = "axilog-replay-scrub";
    scrub.min = "0";
    scrub.max = String(Math.max(0, durationMs));
    scrub.step = "100";
    scrub.value = "0";
    scrub.setAttribute("aria-label", "Replay scrub position");

    var timeLabel = document.createElement("span");
    timeLabel.className = "axilog-replay-time";

    var speedWrap = document.createElement("div");
    speedWrap.className = "axilog-segmented";
    speedWrap.setAttribute("role", "group");
    speedWrap.setAttribute("aria-label", "Playback speed");

    controls.appendChild(playBtn);
    controls.appendChild(scrub);
    controls.appendChild(timeLabel);
    controls.appendChild(speedWrap);

    var stage = document.createElement("div");
    stage.className = "axilog-replay-stage";

    var svg = svgEl("svg", {
      viewBox: vb.x + " " + vb.y + " " + vb.width + " " + vb.height,
      width: "100%",
      class: "axilog-timeline-svg",
      role: "img",
      "aria-label": "Combat replay position stage",
    });

    var gridCols = 8, gridRows = 6;
    for (var gx = 1; gx < gridCols; gx++) {
      var lx = vb.x + (vb.width * gx) / gridCols;
      svg.appendChild(svgEl("line", { x1: lx, x2: lx, y1: vb.y, y2: vb.y + vb.height, class: "axilog-timeline-gridline" }));
    }
    for (var gy = 1; gy < gridRows; gy++) {
      var ly = vb.y + (vb.height * gy) / gridRows;
      svg.appendChild(svgEl("line", { x1: vb.x, x2: vb.x + vb.width, y1: ly, y2: ly, class: "axilog-timeline-gridline" }));
    }

    var markerRadius = Math.max(vb.width, vb.height) * 0.012;
    var dots = tracks.map(function (track) {
      var g = svgEl("g", { class: "axilog-replay-dot" });

      var commanderRing = null;
      if (track.commander) {
        commanderRing = svgEl("circle", { class: "axilog-replay-commander-ring", r: markerRadius * 1.9 });
        g.appendChild(commanderRing);
      }

      var downRing = svgEl("circle", { class: "axilog-replay-down-ring", r: markerRadius * 1.6 });
      downRing.hidden = true;
      g.appendChild(downRing);

      var marker = svgEl("circle", { class: replayMarkerClass(track), r: markerRadius });
      var title = svgEl("title", {});
      title.textContent = track.name;
      marker.appendChild(title);
      g.appendChild(marker);

      svg.appendChild(g);
      return { group: g, marker: marker, downRing: downRing, commanderRing: commanderRing };
    });

    stage.appendChild(svg);
    container.appendChild(controls);
    container.appendChild(stage);

    var playing = false, speed = REPLAY_DEFAULT_SPEED, currentMs = 0, lastWallTs = null;

    REPLAY_SPEEDS.forEach(function (s) {
      var btn = document.createElement("button");
      btn.type = "button";
      btn.className = "axilog-segmented-btn" + (s === speed ? " is-active" : "");
      btn.textContent = s + "x";
      btn.setAttribute("aria-pressed", s === speed ? "true" : "false");
      btn.addEventListener("click", function () {
        speed = s;
        var buttons = speedWrap.querySelectorAll(".axilog-segmented-btn");
        for (var i = 0; i < buttons.length; i++) {
          buttons[i].classList.remove("is-active");
          buttons[i].setAttribute("aria-pressed", "false");
        }
        btn.classList.add("is-active");
        btn.setAttribute("aria-pressed", "true");
      });
      speedWrap.appendChild(btn);
    });

    function applyPositions(ms) {
      var positions = positionsAt(tracks, ms);
      for (var i = 0; i < tracks.length; i++) {
        var track = tracks[i];
        var pos = positions[i];
        var dot = dots[i];
        if (!pos) {
          dot.group.hidden = true;
          continue;
        }
        dot.group.hidden = false;
        dot.marker.setAttribute("cx", pos.x);
        dot.marker.setAttribute("cy", pos.y);
        dot.downRing.setAttribute("cx", pos.x);
        dot.downRing.setAttribute("cy", pos.y);
        if (dot.commanderRing) {
          dot.commanderRing.setAttribute("cx", pos.x);
          dot.commanderRing.setAttribute("cy", pos.y);
        }
        dot.group.style.opacity = isDeadAt(track, ms) ? "0.25" : String(pos.opacity);
        dot.downRing.hidden = !isDownAt(track, ms);
      }
    }

    function setCurrentTime(ms) {
      currentMs = Math.max(0, Math.min(durationMs, ms));
      scrub.value = String(currentMs);
      timeLabel.textContent = formatDuration(currentMs) + " / " + formatDuration(durationMs);
      applyPositions(currentMs);
    }

    function updatePlayButton() {
      playBtn.textContent = playing ? "Pause" : "Play";
    }

    function tick(ts) {
      if (!playing) return;
      if (lastWallTs !== null) {
        setCurrentTime(currentMs + (ts - lastWallTs) * speed);
      }
      lastWallTs = ts;
      if (currentMs >= durationMs) {
        playing = false;
        lastWallTs = null;
        updatePlayButton();
        return;
      }
      requestAnimationFrame(tick);
    }

    function play() {
      if (playing) return;
      if (currentMs >= durationMs) setCurrentTime(0);
      playing = true;
      lastWallTs = null;
      updatePlayButton();
      requestAnimationFrame(tick);
    }

    function pause() {
      playing = false;
      lastWallTs = null;
      updatePlayButton();
    }

    playBtn.addEventListener("click", function () {
      if (playing) pause();
      else play();
    });

    scrub.addEventListener("input", function () {
      pause();
      setCurrentTime(Number(scrub.value));
    });

    setCurrentTime(0);
  }

  var BASE_TAB_DEFS = [
    { id: "damage", buttonId: "axilog-tab-damage", panelId: "axilog-view-damage", render: renderDamageView },
    { id: "support", buttonId: "axilog-tab-support", panelId: "axilog-view-support", render: renderSupportView },
    { id: "boons", buttonId: "axilog-tab-boons", panelId: "axilog-view-boons", render: renderBoonsView },
  ];

  var REPLAY_TAB_DEF = { id: "replay", buttonId: "axilog-tab-replay", panelId: "axilog-view-replay", render: renderReplayView };

  /** Appended only when `hasReplay` says there's something to show --
   * "tab only appears when data.replay present". The button itself is
   * always in the skeleton, `hidden` by default; `initTabs` unhides it. */
  function tabDefsFor(report) {
    return hasReplay(report) ? BASE_TAB_DEFS.concat([REPLAY_TAB_DEF]) : BASE_TAB_DEFS;
  }

  function initTabs(report) {
    var TAB_DEFS = tabDefsFor(report);
    if (hasReplay(report)) {
      byId("axilog-tab-replay").hidden = false;
    }

    var rendered = {};
    var buttons = TAB_DEFS.map(function (def) {
      return byId(def.buttonId);
    });

    function activate(index) {
      TAB_DEFS.forEach(function (def, i) {
        var btn = byId(def.buttonId);
        var panel = byId(def.panelId);
        var active = i === index;
        panel.hidden = !active;
        btn.setAttribute("aria-selected", active ? "true" : "false");
        btn.tabIndex = active ? 0 : -1;
        if (active && !rendered[def.id]) {
          def.render(panel, report);
          rendered[def.id] = true;
        }
        if (active) {
          btn.focus();
        }
      });
    }

    TAB_DEFS.forEach(function (def, index) {
      var btn = byId(def.buttonId);
      btn.addEventListener("click", function () {
        activate(index);
      });
      btn.addEventListener("keydown", function (evt) {
        var delta = evt.key === "ArrowRight" ? 1 : evt.key === "ArrowLeft" ? -1 : 0;
        if (delta === 0) return;
        evt.preventDefault();
        var next = (index + delta + TAB_DEFS.length) % TAB_DEFS.length;
        activate(next);
      });
    });

    // Render the first (default) tab eagerly without stealing focus from
    // the page on load; later tabs render lazily on first activation.
    TAB_DEFS.forEach(function (def, i) {
      var panel = byId(def.panelId);
      var btn = byId(def.buttonId);
      var active = i === 0;
      panel.hidden = !active;
      btn.setAttribute("aria-selected", active ? "true" : "false");
      btn.tabIndex = active ? 0 : -1;
      if (active) {
        def.render(panel, report);
        rendered[def.id] = true;
      }
    });
  }

  /** Parse the embedded `axilog-data` JSON payload. Reads via
   * `textContent` only (never innerHTML) per the XSS contract above —
   * `JSON.parse` treats the result as inert data, never as markup/script. */
  function readEmbeddedReport() {
    var node = byId("axilog-data");
    return JSON.parse(node.textContent);
  }

  function init() {
    var report = readEmbeddedReport();
    renderHeader(report);
    initTabs(report);
    renderTimeline(byId("axilog-timeline-chart"), report);

    var toggle = byId("axilog-theme-toggle");
    if (toggle) {
      toggle.addEventListener("click", function () {
        var html = document.documentElement;
        var next = html.getAttribute("data-theme") === "light" ? "dark" : "light";
        html.setAttribute("data-theme", next);
      });
    }
  }

  if (typeof document !== "undefined") {
    init();
  }

  return {
    // formatting
    formatThousands: formatThousands,
    formatFixed: formatFixed,
    formatPercent: formatPercent,
    formatStacks: formatStacks,
    formatSecondsFromMs: formatSecondsFromMs,
    formatDuration: formatDuration,
    formatK: formatK,
    // sorting
    sortRows: sortRows,
    // header / team chips
    teamCssClass: teamCssClass,
    buildTeamChips: buildTeamChips,
    findCommander: findCommander,
    buildHeaderModel: buildHeaderModel,
    // damage view
    professionDisplay: professionDisplay,
    buildDamageRows: buildDamageRows,
    buildDamageTotals: buildDamageTotals,
    DAMAGE_COLUMNS: DAMAGE_COLUMNS,
    DAMAGE_DEFAULT_SORT: DAMAGE_DEFAULT_SORT,
    // support view
    buildSupportRows: buildSupportRows,
    SUPPORT_COLUMNS: SUPPORT_COLUMNS,
    SUPPORT_DEFAULT_SORT: SUPPORT_DEFAULT_SORT,
    // boons view
    buildBoonRows: buildBoonRows,
    BOON_COLUMNS: BOON_COLUMNS,
    BOON_DEFAULT_SORT: BOON_DEFAULT_SORT,
    GENERATION_MODES: GENERATION_MODES,
    GENERATION_DEFAULT_MODE: GENERATION_DEFAULT_MODE,
    PRESENCE_BOON_NAMES: PRESENCE_BOON_NAMES,
    GENERATION_BOON_NAMES: GENERATION_BOON_NAMES,
    // timeline
    buildTimelinePaths: buildTimelinePaths,
    TIMELINE_WIDTH: TIMELINE_WIDTH,
    TIMELINE_HEIGHT: TIMELINE_HEIGHT,
    // replay
    hasReplay: hasReplay,
    replayViewBox: replayViewBox,
    positionsAt: positionsAt,
    isDownAt: isDownAt,
    isDeadAt: isDeadAt,
    REPLAY_FADE_MS: REPLAY_FADE_MS,
    REPLAY_SPEEDS: REPLAY_SPEEDS,
    REPLAY_DEFAULT_SPEED: REPLAY_DEFAULT_SPEED,
  };
});

// IMPORTANT: this entire file is inlined verbatim (byte-for-byte) into an
// HTML script element by axilog-html's render() — see
// crates/axilog-html/src/lib.rs. The HTML tokenizer's "script data state"
// scans raw script text for the literal ASCII bytes that spell "end tag,
// open bracket, slash, s-c-r-i-p-t" (deliberately not written out in one
// piece anywhere in this note, so the note doesn't trip the very rule
// it's describing — matched case-insensitively) and closes the element
// the instant it finds that byte sequence, regardless of whether it sits
// inside a JS string, a comment, or is otherwise inert to the JS parser.
// So: never write those literal bytes contiguously anywhere in this
// file, including in comments/prose — spell it out in words, or split it
// across a concatenation (e.g. a "<" string joined with a "/script>"
// string) if it must appear in an actual JS string value. A prior
// regression here (a doc-comment example containing the literal bytes)
// truncated the whole script element mid-file in real browsers, leaving
// the header permanently unrendered — `render()`'s Rust-side tests assert
// this file contains no such sequence specifically to prevent a repeat.
