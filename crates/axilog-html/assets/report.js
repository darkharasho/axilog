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
    columns.forEach(function (col) {
      var th = document.createElement("th");
      var btn = document.createElement("button");
      btn.type = "button";
      btn.className = "axilog-sort-btn";
      btn.setAttribute("data-key", col.key);
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
      var buttons = headRow.querySelectorAll(".axilog-sort-btn");
      for (var i = 0; i < buttons.length; i++) {
        var btn = buttons[i];
        var key = btn.getAttribute("data-key");
        var arrow = btn.querySelector(".axilog-sort-arrow");
        if (key === sortState.key) {
          btn.setAttribute("aria-sort", sortState.direction === "asc" ? "ascending" : "descending");
          arrow.textContent = sortState.direction === "asc" ? "↑" : "↓";
        } else {
          btn.removeAttribute("aria-sort");
          arrow.textContent = "";
        }
      }
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

  var TAB_DEFS = [
    { id: "damage", buttonId: "axilog-tab-damage", panelId: "axilog-view-damage", render: renderDamageView },
    { id: "support", buttonId: "axilog-tab-support", panelId: "axilog-view-support", render: renderSupportView },
    { id: "boons", buttonId: "axilog-tab-boons", panelId: "axilog-view-boons", render: renderBoonsView },
  ];

  function initTabs(report) {
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
