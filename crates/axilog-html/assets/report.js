/*
 * axilog WvW report — client-side renderer.
 *
 * XSS CONTRACT (read before touching any render* function):
 *   Every string that originates from the parsed log (account/character
 *   names, marker ids, team colors/guids, map name, warning text,
 *   commander-tag variant, etc.) MUST be written to the DOM via
 *   `Node.textContent` (or `element.textContent =`) only. NEVER use
 *   `innerHTML`, `insertAdjacentHTML`, or string-concatenated markup for
 *   log-derived values. `textContent` treats the value purely as text —
 *   it cannot be parsed as HTML/script, so it is safe even if the value
 *   contains characters like `<`, `>`, or a literal `</script>` string.
 *   The Rust side additionally makes the raw `<script id="axilog-data">`
 *   payload itself injection-safe (see axilog-html/src/lib.rs), but this
 *   textContent-only rule is the second, independent line of defense and
 *   must hold regardless of how the data got here.
 *
 * Structure: render logic is factored into PURE functions (data in,
 * plain-object/array "view model" out — no DOM access) followed by thin
 * DOM glue that walks the view model and assigns textContent/classList.
 * This keeps the formatting/derivation logic testable outside a browser
 * (see later milestone tasks' node-based tests) independent of DOM
 * plumbing.
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

  // ---- pure functions -----------------------------------------------

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

  /** Known team colors get their own CSS class; anything else falls back
   * to "unknown" so future/uncommon team colors still render sanely. */
  var KNOWN_TEAM_COLORS = { red: true, blue: true, green: true };
  function teamCssClass(color) {
    return KNOWN_TEAM_COLORS.hasOwnProperty(color) ? color : "unknown";
  }

  /**
   * Build the team-chip view model: one entry per `encounter.teams`,
   * with the count of players whose `team` field matches that team's
   * color. Pure — no DOM.
   */
  function buildTeamChips(encounter, players) {
    var counts = {};
    (players || []).forEach(function (p) {
      counts[p.team] = (counts[p.team] || 0) + 1;
    });
    return (encounter.teams || []).map(function (t) {
      return {
        color: t.color,
        cssClass: teamCssClass(t.color),
        count: counts[t.color] || 0,
      };
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
      teams: buildTeamChips(encounter, report.players),
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

  // ---- DOM glue --------------------------------------------------------

  function byId(id) {
    return document.getElementById(id);
  }

  function renderTeamChips(container, teams) {
    container.textContent = "";
    teams.forEach(function (t) {
      var chip = document.createElement("span");
      chip.className = "axilog-team-chip team-" + t.cssClass;
      chip.textContent = t.color + " · " + t.count;
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

  /** Parse the embedded `<script id="axilog-data">` JSON payload. Reads
   * via `textContent` only (never innerHTML) per the XSS contract above —
   * `JSON.parse` treats the result as inert data, never as markup/script. */
  function readEmbeddedReport() {
    var node = byId("axilog-data");
    return JSON.parse(node.textContent);
  }

  function init() {
    var report = readEmbeddedReport();
    renderHeader(report);

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
    formatDuration: formatDuration,
    teamCssClass: teamCssClass,
    buildTeamChips: buildTeamChips,
    findCommander: findCommander,
    buildHeaderModel: buildHeaderModel,
  };
});
