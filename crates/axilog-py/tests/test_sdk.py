"""stdlib-unittest suite for the axilog PyO3 module (M6 Task 2).

Mirrors `crates/axilog-node/__test__/sdk.test.mjs` (the Node SDK's
equivalent M5 Task 2 suite) one-for-one where the two SDKs' surfaces
match: same fixture, same calibrated values, same CLI-parity approach --
cross-check that file if a change here looks surprising.

Every fixture/binary path below is resolved from *this file's own
location* (via `__file__`), not the process cwd, so the suite runs
identically whichever of the two documented invocations is used (see the
README "Tests" section):

    # from the repo root
    crates/axilog-py/.venv/bin/python -m unittest discover -s crates/axilog-py/tests

    # from crates/axilog-py/
    .venv/bin/python -m unittest discover -s tests

Requires the `axilog` extension module to already be installed into the
interpreter running this suite (`maturin develop --release` in
`crates/axilog-py/.venv`, see README) -- this suite does not build it.
"""

import json
import os
import subprocess
import tempfile
import unittest

import axilog

THIS_DIR = os.path.dirname(os.path.abspath(__file__))
# crates/axilog-py/tests -> crates/axilog-py -> crates -> repo root
REPO_ROOT = os.path.abspath(os.path.join(THIS_DIR, "..", "..", ".."))
FIXTURE = os.path.join(REPO_ROOT, "fixtures", "wvw-small.anon.zevtc")
GOLDEN_EI_JSON = os.path.join(REPO_ROOT, "fixtures", "wvw-small.ei.json")
CLI_BIN = os.path.join(REPO_ROOT, "target", "debug", "axilog")

# M3 Task 3 / M2 Task 3 calibration targets (see
# `crates/axilog-core/tests/golden.rs` / `support_golden.rs`): this exact
# committed fixture's squad damage total and squad support sums, verified
# directly against a real dps.report EI export
# (`fixtures/wvw-small.ei.json`'s `squadTotalDamage`/`squadCondiCleanse`/
# `squadCondiCleanseSelf`/`squadBoonStrips`/`squadResurrects`). Read here
# as plain literals (same as the Node suite), independent of the Rust-side
# calibration tests' own tolerance windows.
EXPECTED_PLAYER_COUNT = 42
EXPECTED_SQUAD_DAMAGE_TOTAL = 2138414
EXPECTED_SUPPORT_SUMS = {
    "cleanses": 801,
    "cleanses_self": 97,
    "strips": 437,
    "resurrects": 6,
}
# `Anon132.5884`'s Quickness (a duration-type boon, id 1187) presence_pct,
# read from the golden EI JSON's `players[].boons["1187"].uptime` (for a
# duration boon, EI's `uptime` field IS the presence percentage -- see
# `axilog_core::analysis::buffs::uptime`'s module doc). Cross-checked live
# below against the golden fixture file rather than hardcoded only, per
# the Task 2 brief's "cross-checked against fixtures/wvw-small.ei.json".
STABLE_BOON_ACCOUNT = ":Anon132.5884"
STABLE_BOON_GOLDEN_ACCOUNT = "Anon132.5884"  # golden JSON has no leading ':'
STABLE_BOON_NAME = "Quickness"
STABLE_BOON_ID = "1187"


def sum_by(items, key):
    return sum(key(x) for x in items)


# ----------------------------------------------------------------------
# Native 1.0 container accessors.
#
# `parse_file`/`parse_bytes` return `{axilog, blocks, catalogs, coverage,
# encounter, entities}` -- there is no flat `players[]` any more. The
# roster lives in `entities[]` and every per-entity analysis block is a
# `blocks.<name>.by_entity` map keyed by `entities[].id`, so the shape
# assertions below join the two rather than reading nested objects. Same
# accessors as the Node suite's, kept name-for-name.
# ----------------------------------------------------------------------

# The pre-1.0 `Report.players[]` was the log-recording squad PLUS
# non-squad friendly players -- 42 on this fixture, which is also what the
# golden EI export's `players` length and `parse_file_ei` report. The two
# roles have to stay split: the squad alone accounts for all 2,138,414
# damage but only 776 of the 801 cleanses.
PLAYER_ROLES = frozenset(("squad", "friendly_player"))


def players(report):
    return [e for e in report["entities"] if e["role"] in PLAYER_ROLES]


def slot(block, entity):
    """`entity`'s row in a `blocks.<name>` map, or None if it has none.

    Keys are JSON object keys, so they arrive as strings even though
    `entities[].id` is an int.
    """
    if not block:
        return None
    return block.get("by_entity", {}).get(str(entity["id"]))


def buff_id_by_name(report, name):
    """The buff catalog id `name` is published under, e.g. 'Quickness' -> '1187'."""
    for buff_id, desc in report["catalogs"]["buffs"].items():
        if desc["name"] == name:
            return buff_id
    return None


def decode_series(test, series):
    """Expand a run-length-encoded `blocks.series` channel to a flat list."""
    test.assertEqual(series["enc"], "rle", "series channels are run-length encoded")
    out = []
    for value, run in series["data"]:
        out.extend([value] * run)
    test.assertEqual(len(out), series["len"], "the run lengths must sum to the declared len")
    return out


def first_diff_paths(a, b, limit=10, path="$", out=None):
    """First `limit` paths where `a` and `b` differ, DFS over plain
    dicts/lists. Used only to produce a readable failure message for the
    dual-path CLI-parity test -- a bare assertEqual on two large parsed
    reports just dumps the entire diff.
    """
    if out is None:
        out = []
    if len(out) >= limit:
        return out
    if a == b:
        return out
    a_is_obj = isinstance(a, (dict, list))
    b_is_obj = isinstance(b, (dict, list))
    if not a_is_obj or not b_is_obj:
        out.append(f"{path}: {a!r} != {b!r}")
        return out
    a_is_list = isinstance(a, list)
    b_is_list = isinstance(b, list)
    if a_is_list != b_is_list:
        out.append(f"{path}: array-ness differs ({a_is_list} vs {b_is_list})")
        return out
    if a_is_list:
        keys = range(max(len(a), len(b)))
        a_map, b_map = a, b
    else:
        keys = sorted(set(a.keys()) | set(b.keys()))
        a_map, b_map = a, b
    for k in keys:
        if len(out) >= limit:
            break
        child_path = f"{path}[{k}]" if a_is_list else f"{path}.{k}"
        has_a = k < len(a_map) if a_is_list else k in a_map
        has_b = k < len(b_map) if b_is_list else k in b_map
        if not has_a:
            out.append(f"{child_path}: missing on left, right={b_map[k]!r}")
        elif not has_b:
            out.append(f"{child_path}: missing on right, left={a_map[k]!r}")
        else:
            first_diff_paths(a_map[k], b_map[k], limit, child_path, out)
    return out


class ParseFileTests(unittest.TestCase):
    def test_shape_and_calibrated_values(self):
        report = axilog.parse_file(FIXTURE)

        self.assertEqual(report["axilog"]["schema"], "1.0")
        self.assertIsInstance(report["axilog"]["version"], str)
        # `parse_file` has a real file name to thread through; only the
        # bare name, never the full path.
        self.assertEqual(report["axilog"]["generated_from"], "wvw-small.anon.zevtc")

        roster = players(report)
        self.assertGreater(len(roster), 0)
        self.assertEqual(len(roster), EXPECTED_PLAYER_COUNT)

        damage = report["blocks"]["damage"]
        squad_damage_total = sum_by(roster, lambda p: (slot(damage, p) or {}).get("total", 0))
        self.assertEqual(squad_damage_total, EXPECTED_SQUAD_DAMAGE_TOTAL)
        # The squad rollup is computed independently of the per-entity map,
        # so this pins the two against each other as well as the golden.
        self.assertEqual(damage["squad"]["total"], EXPECTED_SQUAD_DAMAGE_TOTAL)

        support_block = report["blocks"]["support"]

        def support_sum(key):
            return sum_by(roster, lambda p: (slot(support_block, p) or {}).get(key, 0))

        support = {k: support_sum(k) for k in EXPECTED_SUPPORT_SUMS}
        self.assertEqual(support, EXPECTED_SUPPORT_SUMS)

    def test_one_boon_uptime_cross_checked_against_golden_ei_json(self):
        report = axilog.parse_file(FIXTURE)
        player = next(
            (p for p in players(report) if p["account"] == STABLE_BOON_ACCOUNT), None
        )
        self.assertIsNotNone(
            player, f"expected fixture to contain account {STABLE_BOON_ACCOUNT}"
        )
        # Boon rows are keyed by buff id, with the display name in the catalog.
        buff_id = buff_id_by_name(report, STABLE_BOON_NAME)
        self.assertIsNotNone(
            buff_id, f"expected the buff catalog to describe {STABLE_BOON_NAME}"
        )
        boon = (slot(report["blocks"]["boons"], player) or {}).get(buff_id)
        self.assertIsNotNone(
            boon, f"expected {STABLE_BOON_ACCOUNT} to have a {STABLE_BOON_NAME} entry"
        )

        with open(GOLDEN_EI_JSON, encoding="utf-8") as f:
            golden = json.load(f)
        golden_player = next(
            (p for p in golden["players"] if p["account"] == STABLE_BOON_GOLDEN_ACCOUNT), None
        )
        self.assertIsNotNone(
            golden_player,
            f"expected golden EI fixture to contain account {STABLE_BOON_GOLDEN_ACCOUNT}",
        )
        golden_presence_pct = golden_player["boons"][STABLE_BOON_ID]["uptime"]

        self.assertLess(
            abs(boon["uptime_pct"] - golden_presence_pct),
            0.05,
            f"{STABLE_BOON_NAME} uptime_pct {boon['uptime_pct']} not within 0.05 "
            f"of golden {golden_presence_pct}",
        )


class ReplayOptInTests(unittest.TestCase):
    """`replay=True` gates POSITIONS, not the whole block. The down/dead
    intervals under `blocks.replay.by_entity` are computed on every parse, so
    -- unlike every other opt-in in this file -- `coverage.replay ==
    "present"` is not a statement about whether the flag was passed. What the
    flag adds is `blocks.replay.tracks`. Mirrors
    `crates/axilog-node/__test__/sdk.test.mjs`'s equivalent test."""

    def test_intervals_are_always_on_and_positions_are_gated(self):
        without_replay = axilog.parse_file(FIXTURE)
        intervals_only = without_replay["blocks"]["replay"]
        self.assertEqual(without_replay["coverage"]["replay"], "present")
        self.assertNotIn("tracks", intervals_only, "positions need replay=True")
        self.assertGreater(len(intervals_only["by_entity"]), 0, "expected intervals rows")
        row = next(iter(intervals_only["by_entity"].values()))
        for k in ("start_ms", "end_ms", "active_ms"):
            self.assertIsInstance(row[k], int, f"by_entity row {k}")
        self.assertIsInstance(row["down"], list)
        self.assertIsInstance(row["dead"], list)

        with_replay = axilog.parse_file(FIXTURE, replay=True)
        self.assertEqual(with_replay["coverage"]["replay"], "present")
        replay = with_replay["blocks"]["replay"]
        self.assertEqual(
            replay["by_entity"],
            intervals_only["by_entity"],
            "turning the position gate on must not change the intervals half",
        )

        tracks = replay["tracks"]
        self.assertIsInstance(tracks["poll_ms"], int)
        for k in ("min_x", "max_x", "min_y", "max_y"):
            self.assertIsInstance(tracks["bounds"][k], (int, float), f"bounds.{k}")

        # Tracks are a by-entity map, not a flat array: the name/team/squad
        # membership the pre-1.0 `tracks[]` duplicated now live once on the
        # `entities[]` row the key joins to.
        track_ids = list(tracks["by_entity"])
        self.assertGreater(len(track_ids), 0, "expected at least one replay track")
        entity_ids = {str(e["id"]) for e in with_replay["entities"]}
        for tid in track_ids:
            self.assertIn(tid, entity_ids, f"replay track {tid} joins to no entities[] row")
        track = tracks["by_entity"][track_ids[0]]
        self.assertIsInstance(track["samples"], list)
        # Kept on the track as well as on `by_entity`: the track roster also
        # covers enemy players, whom the always-on intervals pass never walks.
        self.assertIsInstance(track["down_intervals"], list)
        self.assertIsInstance(track["dead_intervals"], list)
        if track["samples"]:
            self.assertEqual(len(track["samples"][0]), 3, "each sample is a [t, x, y] triple")

        explicitly_off = axilog.parse_file(FIXTURE, replay=False)
        self.assertNotIn("tracks", explicitly_off["blocks"]["replay"])

        with open(FIXTURE, "rb") as f:
            data = f.read()
        bytes_with_replay = axilog.parse_bytes(data, replay=True)
        self.assertIn("tracks", bytes_with_replay["blocks"]["replay"])


class SkillDamageOptInTests(unittest.TestCase):
    """M12 Task 1: `skill_damage=True` opts into the native per-skill damage
    distribution block; absent by default. Mirrors `crates/axilog-node/
    __test__/sdk.test.mjs`'s equivalent test."""

    def test_skill_damage_absent_by_default_present_and_shaped_when_requested(self):
        without = axilog.parse_file(FIXTURE)
        any_player = players(without)[0]
        self.assertNotIn("by_skill", slot(without["blocks"]["damage"], any_player))
        self.assertNotIn("by_skill_taken", slot(without["blocks"]["damage"], any_player))

        with_it = axilog.parse_file(FIXTURE, skill_damage=True)
        damage = with_it["blocks"]["damage"]
        p0 = next(p for p in players(with_it) if (slot(damage, p) or {}).get("total", 0) > 0)
        d0 = slot(damage, p0)
        self.assertIn("by_skill", d0)
        self.assertIn("by_skill_taken", d0)
        skill_ids = list(d0["by_skill"])
        self.assertGreater(len(skill_ids), 0, "expected at least one outgoing skill entry")
        entry = d0["by_skill"][skill_ids[0]]
        for k in ("total", "hits", "crit_hits", "flank_hits", "min", "max"):
            self.assertIsInstance(entry[k], int, f"by_skill entries carry a numeric {k}")
        # Every referenced skill id must be described by the skill catalog.
        for sid in skill_ids:
            self.assertIn(sid, with_it["catalogs"]["skills"], f"skill {sid} missing from catalog")
        # sum(by_skill[*]["total"]) == the damage total exactly (internal invariant).
        self.assertEqual(sum(e["total"] for e in d0["by_skill"].values()), d0["total"])
        # The per-target breakdown gains the same distribution under the flag.
        per_target = list(d0["per_target"].values())
        self.assertGreater(len(per_target), 0, "expected at least one per-target entry")
        self.assertTrue(all("by_skill" in t for t in per_target))

        explicitly_off = axilog.parse_file(FIXTURE, skill_damage=False)
        self.assertNotIn("by_skill", slot(explicitly_off["blocks"]["damage"], any_player))

        with open(FIXTURE, "rb") as f:
            data = f.read()
        bytes_with_it = axilog.parse_bytes(data, skill_damage=True)
        self.assertIn("by_skill", slot(bytes_with_it["blocks"]["damage"], p0))


class TimeseriesOptInTests(unittest.TestCase):
    """M12 Task 2: `timeseries=True` opts into the native per-player
    per-second series block AND the per-enemy `dps_targets` summary; both
    absent by default. Mirrors `crates/axilog-node/__test__/sdk.test.mjs`'s
    equivalent test."""

    def test_timeseries_absent_by_default_present_and_shaped_when_requested(self):
        without = axilog.parse_file(FIXTURE)
        any_player = players(without)[0]
        self.assertIsNone(
            slot(without["blocks"]["series"], any_player),
            "the per-entity series must be absent by default",
        )
        # The squad-level series is always on -- only the per-entity map is gated.
        self.assertIn("damage", without["blocks"]["series"]["squad"])

        with_it = axilog.parse_file(FIXTURE, timeseries=True)
        damage_block = with_it["blocks"]["damage"]
        p0 = next(p for p in players(with_it) if (slot(damage_block, p) or {}).get("total", 0) > 0)
        series = slot(with_it["blocks"]["series"], p0)
        self.assertIsNotNone(series, "expected a per-entity series when timeseries=True")
        for k in ("damage", "damage_taken", "power_damage_taken"):
            self.assertIsInstance(series[k]["interval_ms"], int, f"{k} carries its bucket width")

        damage = decode_series(self, series["damage"])
        self.assertGreater(len(damage), 0, "expected at least one bucket")
        # Cumulative: the final bucket equals the damage total exactly.
        self.assertEqual(damage[-1], slot(damage_block, p0)["total"])
        # Monotonic non-decreasing.
        for a, b in zip(damage, damage[1:]):
            self.assertLessEqual(a, b, "the damage series must be cumulative")

        # The per-target series joins to the always-on per-target damage map.
        per_target_ids = list(series["per_target"])
        self.assertGreater(len(per_target_ids), 0, "expected at least one per-target series")
        damage_targets = slot(damage_block, p0)["per_target"]
        for tid in per_target_ids:
            self.assertIn(tid, damage_targets, f"per-target series {tid} has no damage entry")
        # sum(per_target[*]["total"]) == damage total exactly (internal invariant).
        self.assertEqual(
            sum(t["total"] for t in damage_targets.values()), slot(damage_block, p0)["total"]
        )

        explicitly_off = axilog.parse_file(FIXTURE, timeseries=False)
        self.assertIsNone(slot(explicitly_off["blocks"]["series"], any_player))

        with open(FIXTURE, "rb") as f:
            data = f.read()
        bytes_with_it = axilog.parse_bytes(data, timeseries=True)
        self.assertIsNotNone(slot(bytes_with_it["blocks"]["series"], p0))


class MissilesOptInTests(unittest.TestCase):
    """final-review fix wave: `missiles=True` opts into the native
    top-level missile analytics block; absent by default. Mirrors
    `crates/axilog-node/__test__/sdk.test.mjs`'s equivalent test."""

    def test_missiles_absent_by_default_present_and_shaped_when_requested(self):
        without = axilog.parse_file(FIXTURE)
        self.assertNotIn("missiles", without["blocks"])
        self.assertEqual(without["coverage"]["missiles"], "not_computed")

        with_it = axilog.parse_file(FIXTURE, missiles=True)
        self.assertIn("missiles", with_it["blocks"])
        self.assertEqual(with_it["coverage"]["missiles"], "present")
        missiles = with_it["blocks"]["missiles"]
        for k in ("fired", "hit", "denied", "incoming_fired", "incoming_denied"):
            self.assertIsInstance(missiles["squad"][k], int, f"squad.{k}")
        per_entity = list(missiles["by_entity"].values())
        self.assertGreater(len(per_entity), 0, "expected at least one per-entity missile row")
        for k in ("fired", "hit", "denied", "reflected_at_self"):
            self.assertIsInstance(per_entity[0][k], int, f"by_entity rows carry a numeric {k}")

        explicitly_off = axilog.parse_file(FIXTURE, missiles=False)
        self.assertNotIn("missiles", explicitly_off["blocks"])


class ModifiersOptInTests(unittest.TestCase):
    """M16: the damage-modifier block is opt-in via `modifiers=True`.
    Unlike every other opt-in block that flag gates a COMPUTATION rather
    than a copy -- `analyze()` never runs the modifier engine."""

    def test_modifiers_absent_by_default_present_and_shaped_when_requested(self):
        """M16: `modifiers=True` adds `players[].damage_mods` plus the
        top-level `damage_mod_map`. Mirrors `crates/axilog-node/__test__/
        sdk.test.mjs`'s equivalent test."""
        without = axilog.parse_file(FIXTURE)
        self.assertNotIn("damage_mods", without["catalogs"])
        self.assertNotIn("damage_mods", without["blocks"])
        self.assertEqual(without["coverage"]["damage_mods"], "not_computed")

        with_it = axilog.parse_file(FIXTURE, modifiers=True)
        catalog = with_it["catalogs"]["damage_mods"]
        self.assertGreater(len(catalog), 0, "expected at least one referenced modifier id")
        self.assertEqual(with_it["coverage"]["damage_mods"], "present")

        # Two SCOPES per entity: `overall` (whole fight) and the sparse
        # `per_target` map beside it, keyed by the target's entity id.
        # Within each scope the direction that used to be an
        # outgoing/incoming array split is carried by the id's SIGN.
        #
        # The per-target scope is the expensive half (~11x the whole-fight
        # arrays), so the NATIVE path does not compute it -- only
        # `parse_file_ei` asks for it, and an absent `per_target` on a
        # present block means exactly that.
        row_sets = list(with_it["blocks"]["damage_mods"]["by_entity"].values())
        self.assertGreater(len(row_sets), 0, "expected at least one entity with modifier rows")
        saw_incoming = saw_outgoing = saw_per_target = False

        def check_scope(rows):
            nonlocal saw_incoming, saw_outgoing
            for mod_id, r in rows.items():
                if int(mod_id) < 0:
                    saw_incoming = True
                else:
                    saw_outgoing = True
                # Native keys carry no "d" prefix -- that is an ei-json-ism.
                self.assertFalse(mod_id.startswith("d"), f"unexpected 'd' prefix on {mod_id}")
                self.assertIn(mod_id, catalog, f"modifier {mod_id} missing from the catalog")
                for k in ("hit_count", "total_hit_count", "total_damage"):
                    self.assertIsInstance(r[k], int, f"{k} must be an int")
                self.assertIsInstance(r["damage_gain"], float)
                self.assertGreaterEqual(r["hit_count"], 1)
                self.assertLessEqual(r["hit_count"], r["total_hit_count"])

        for entity in row_sets:
            self.assertIn("overall", entity, "every damage_mods row carries an overall scope")
            check_scope(entity["overall"])
            for per_target in entity.get("per_target", {}).values():
                saw_per_target = True
                check_scope(per_target)
        self.assertTrue(saw_outgoing, "expected at least one outgoing (positive-id) row")
        self.assertTrue(saw_incoming, "expected at least one incoming (negative-id) row")
        self.assertFalse(saw_per_target, "the native path must not compute the per-target split")

        desc = next(iter(catalog.values()))
        for k in ("name", "description"):
            self.assertIsInstance(desc[k], str)
        for k in ("non_multiplier", "is_counter", "skill_based", "approximate"):
            self.assertIsInstance(desc[k], bool)

        explicitly_off = axilog.parse_file(FIXTURE, modifiers=False)
        self.assertNotIn("damage_mods", explicitly_off["catalogs"])
        self.assertNotIn("damage_mods", explicitly_off["blocks"])


class ParseFileEiOptInTests(unittest.TestCase):
    """final-review fix wave: `parse_file_ei` accepts the same
    replay/skill_damage/timeseries/missiles keyword args `parse_file` does
    -- `skill_damage=True`/`timeseries=True` are what let
    `totalDamageDist`/`damage1S` surface in the ei-json output (see
    `axilog_ei::to_ei_json`, which reads them straight off the native
    `Report`). Default call (no kwargs) must keep omitting both -- the
    back-compat requirement. Mirrors `crates/axilog-node/__test__/
    sdk.test.mjs`'s equivalent test."""

    def test_replay_adds_gw2ei_combat_replay_positions_and_metadata(self):
        """M15 Task 3: `replay=True` adds GW2EI's own combat-replay surface
        -- per-actor `combatReplayData.{positions, orientations, dc,
        iconURL}` plus the top-level `combatReplayMetaData` -- and leaves
        M11's always-on `start`/`end`/`down`/`dead` untouched. Mirrors
        `crates/axilog-node/__test__/sdk.test.mjs`'s equivalent test."""
        ei = axilog.parse_file_ei(FIXTURE, replay=True)

        meta = ei["combatReplayMetaData"]
        self.assertEqual(meta["pollingRate"], 300)
        self.assertEqual(meta["sizes"], [523, 750])
        # The f32-text contract: EI writes a C# float, so this is exactly
        # 0.009 -- a widened f64 would arrive as 0.008999999612569809.
        self.assertEqual(meta["inchToPixel"], 0.009)
        self.assertEqual(len(meta["maps"]), 1)

        crd = ei["players"][0]["combatReplayData"]
        self.assertGreater(len(crd["positions"]), 0)
        self.assertEqual(len(crd["positions"]), len(crd["orientations"]))
        self.assertEqual(len(crd["positions"][0]), 2)
        self.assertGreater(len(crd["dc"]), 0)
        self.assertTrue(crd["iconURL"].startswith("https://"))

        plain = axilog.parse_file_ei(FIXTURE)["players"][0]["combatReplayData"]
        for key in ("start", "end", "down", "dead"):
            self.assertEqual(crd[key], plain[key], key)

    def test_skill_damage_and_timeseries_surface_only_when_requested(self):
        without_opts = axilog.parse_file_ei(FIXTURE)
        p0_without = without_opts["players"][0]
        self.assertNotIn("totalDamageDist", p0_without)
        self.assertNotIn("damage1S", p0_without)

        with_opts = axilog.parse_file_ei(FIXTURE, skill_damage=True, timeseries=True)
        p0_with = next(
            (
                p
                for p in with_opts["players"]
                if p.get("totalDamageDist") and len(p["totalDamageDist"][0]) > 0
            ),
            None,
        )
        self.assertIsNotNone(
            p0_with,
            "expected at least one player with a non-empty totalDamageDist "
            "when skill_damage=True",
        )
        self.assertIsInstance(p0_with["totalDamageDist"], list)
        self.assertIn("damage1S", p0_with)
        self.assertGreater(
            len(p0_with["damage1S"][0]),
            0,
            "expected a non-empty per-second series inside damage1S's phase wrapper",
        )


    def test_modifiers_add_ei_damage_modifier_arrays_and_map(self):
        """M16: `modifiers=True` adds EI's own
        `damageModifiers`/`incomingDamageModifiers`/`damageModifiersTarget`/
        `incomingDamageModifiersTarget` plus the top-level `damageModMap`
        (keyed `"d<signed id>"`). Mirrors the node suite's equivalent."""
        without = axilog.parse_file_ei(FIXTURE)
        self.assertNotIn("damageModMap", without)
        self.assertNotIn("damageModifiers", without["players"][0])

        ei = axilog.parse_file_ei(FIXTURE, modifiers=True)
        self.assertIn("damageModMap", ei)
        for k in ei["damageModMap"]:
            self.assertTrue(k.startswith("d"), f"damageModMap keys carry EI's 'd' prefix, got {k}")

        n_targets = len(ei["targets"])
        with_rows = [p for p in ei["players"] if p["damageModifiers"]]
        self.assertTrue(with_rows, "expected at least one player with damageModifiers")
        p = with_rows[0]
        # EI nests the four numbers one level deeper, as a per-phase array.
        item = p["damageModifiers"][0]["damageModifiers"][0]
        for k in ("hitCount", "totalHitCount", "damageGain", "totalDamage"):
            self.assertIn(k, item)
        for k in ("damageModifiersTarget", "incomingDamageModifiersTarget"):
            self.assertEqual(len(p[k]), n_targets, f"{k} must have one slot per targets[] entry")
        self.assertTrue(
            any(slot for slot in p["damageModifiersTarget"]),
            "expected at least one populated per-target slot",
        )


class ParseBytesTests(unittest.TestCase):
    def test_parse_bytes_matches_parse_file(self):
        from_file = axilog.parse_file(FIXTURE)
        with open(FIXTURE, "rb") as f:
            data = f.read()
        from_bytes = axilog.parse_bytes(data)

        # `generated_from` is the ONE documented difference: a buffer has no
        # file name to offer, so `parse_bytes` omits the key entirely.
        self.assertNotIn("generated_from", from_bytes["axilog"])
        file_header = {k: v for k, v in from_file["axilog"].items() if k != "generated_from"}
        self.assertEqual(from_bytes["axilog"], file_header)
        self.assertEqual(
            {k: v for k, v in from_bytes.items() if k != "axilog"},
            {k: v for k, v in from_file.items() if k != "axilog"},
        )

    def test_parse_bytes_rejects_bytearray(self):
        """Verify parse_bytes only accepts bytes, not bytearray."""
        with open(FIXTURE, "rb") as f:
            data = bytearray(f.read())
        with self.assertRaises(TypeError):
            axilog.parse_bytes(data)


class ParseFileEiTests(unittest.TestCase):
    def test_axibridge_key_shapes(self):
        ei = axilog.parse_file_ei(FIXTURE)

        self.assertIsInstance(ei["players"], list)
        self.assertGreater(len(ei["players"]), 0)
        p0 = ei["players"][0]
        self.assertIsInstance(p0["account"], str)
        self.assertGreater(len(p0["account"]), 0)

        self.assertIsInstance(p0["dpsAll"], list)
        self.assertGreater(len(p0["dpsAll"]), 0)
        self.assertIsInstance(p0["dpsAll"][0]["damage"], int)

        self.assertIsInstance(p0["support"], list)
        self.assertGreater(len(p0["support"]), 0)
        self.assertIsInstance(p0["support"][0]["condiCleanse"], int)

        self.assertIsInstance(p0["buffUptimes"], list)
        self.assertGreater(len(p0["buffUptimes"]), 0, "expected non-empty buffUptimes")

        self.assertIsInstance(ei["targets"], list)
        self.assertGreater(len(ei["targets"]), 0)
        for t in ei["targets"]:
            self.assertIsInstance(t["enemyPlayer"], bool)
            # M11 Task 3: every target is a real (non-aggregate) agent --
            # axibridge filters `!t.isFake` everywhere it reads `targets[]`.
            self.assertEqual(t["isFake"], False, "every target must be isFake: false")

        # M11 Task 3: `activeTimes`/`combatReplayData` are ALWAYS present
        # (not gated on a `--replay`-equivalent option -- `parse_file_ei`
        # takes none), with `down`/`dead` arrays of `[start, end]` pairs
        # (positions stay absent -- see `axilog_ei::to_ei_json`'s module
        # comment).
        self.assertIsInstance(p0["activeTimes"], list)
        self.assertEqual(len(p0["activeTimes"]), 1)
        self.assertIsInstance(p0["activeTimes"][0], int)
        self.assertIn("combatReplayData", p0)
        self.assertIsInstance(p0["combatReplayData"]["start"], int)
        self.assertIsInstance(p0["combatReplayData"]["end"], int)
        self.assertIsInstance(p0["combatReplayData"]["down"], list)
        self.assertIsInstance(p0["combatReplayData"]["dead"], list)
        self.assertNotIn(
            "positions", p0["combatReplayData"],
            "positions must stay absent without replay=True",
        )
        self.assertNotIn(
            "combatReplayMetaData", ei,
            "combatReplayMetaData must stay absent without replay=True",
        )

        self.assertIn("wvWMapData", ei)
        self.assertIsNotNone(ei["wvWMapData"])
        for key in ("redTeamID", "blueTeamID", "greenTeamID"):
            self.assertIsInstance(ei["wvWMapData"][key], int)

        # Cross-check against the golden EI fixture this same log was
        # verified against (see `crates/axilog-core/tests/*_golden.rs`).
        with open(GOLDEN_EI_JSON, encoding="utf-8") as f:
            golden = json.load(f)
        self.assertGreater(len(golden["players"]), 0)


class AnonymizeFileTests(unittest.TestCase):
    def test_round_trip_parses_identically(self):
        with tempfile.TemporaryDirectory(prefix="axilog-py-test-") as tmp_dir:
            out_path = os.path.join(tmp_dir, "wvw-small.anon2.zevtc")

            rewritten = axilog.anonymize_file(FIXTURE, out_path)
            self.assertGreater(rewritten, 0, "expected at least one player agent rewritten")

            original = axilog.parse_file(FIXTURE)
            round_tripped = axilog.parse_file(out_path)

            self.assertEqual(len(players(round_tripped)), len(players(original)))
            self.assertEqual(len(round_tripped["entities"]), len(original["entities"]))

            def squad_damage(report):
                block = report["blocks"]["damage"]
                return sum_by(players(report), lambda p: (slot(block, p) or {}).get("total", 0))

            self.assertEqual(squad_damage(round_tripped), squad_damage(original))

            self.assertEqual(
                round_tripped["encounter"]["duration_ms"], original["encounter"]["duration_ms"]
            )
            self.assertEqual(
                round_tripped["blocks"]["damage"]["squad"]["total"],
                original["blocks"]["damage"]["squad"]["total"],
            )


class ErrorHandlingTests(unittest.TestCase):
    def test_missing_file_raises_oserror(self):
        with tempfile.TemporaryDirectory(prefix="axilog-py-test-") as tmp_dir:
            missing_path = os.path.join(tmp_dir, "does-not-exist.zevtc")
            with self.assertRaises(OSError) as ctx:
                axilog.parse_file(missing_path)
            self.assertTrue(str(ctx.exception))

    def test_corrupt_bytes_raises_valueerror(self):
        with self.assertRaises(ValueError) as ctx:
            axilog.parse_bytes(b"this is not a valid evtc/zevtc payload")
        self.assertTrue(str(ctx.exception))


class CliParityTests(unittest.TestCase):
    """Dual-path parity: axilog.parse_file's Report must exactly match the
    CLI's `parse --format json` output for the same fixture.

    If `target/debug/axilog` is missing, build it first:
    `cargo build -p axilog-cli` from the repo root.
    """

    def test_parse_file_matches_cli_format_json(self):
        if not os.path.isfile(CLI_BIN):
            self.fail(
                f"CLI binary not found at {CLI_BIN}; run `cargo build -p axilog-cli` "
                "from the repo root first"
            )

        py_report = axilog.parse_file(FIXTURE)
        stdout = subprocess.run(
            [CLI_BIN, "parse", FIXTURE, "--format", "json"],
            check=True,
            capture_output=True,
        ).stdout
        cli_report = json.loads(stdout)

        if py_report != cli_report:
            diffs = first_diff_paths(py_report, cli_report, 10)
            self.fail(
                "axilog.parse_file output diverges from CLI --format json output at "
                f"{len(diffs)} path(s):\n" + "\n".join(diffs)
            )


if __name__ == "__main__":
    unittest.main()
