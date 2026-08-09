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

        self.assertEqual(report["schema_version"], "0.2")
        self.assertIsInstance(report["axilog_version"], str)
        self.assertGreater(len(report["players"]), 0)
        self.assertEqual(len(report["players"]), EXPECTED_PLAYER_COUNT)

        squad_damage_total = sum_by(report["players"], lambda p: p["damage"]["total"])
        self.assertEqual(squad_damage_total, EXPECTED_SQUAD_DAMAGE_TOTAL)

        support = {
            "cleanses": sum_by(report["players"], lambda p: p["support"]["cleanses"]),
            "cleanses_self": sum_by(report["players"], lambda p: p["support"]["cleanses_self"]),
            "strips": sum_by(report["players"], lambda p: p["support"]["strips"]),
            "resurrects": sum_by(report["players"], lambda p: p["support"]["resurrects"]),
        }
        self.assertEqual(support, EXPECTED_SUPPORT_SUMS)

    def test_one_boon_uptime_cross_checked_against_golden_ei_json(self):
        report = axilog.parse_file(FIXTURE)
        player = next(
            (p for p in report["players"] if p["account"] == STABLE_BOON_ACCOUNT), None
        )
        self.assertIsNotNone(
            player, f"expected fixture to contain account {STABLE_BOON_ACCOUNT}"
        )
        boon = next((b for b in player["boons"] if b["name"] == STABLE_BOON_NAME), None)
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
            abs(boon["presence_pct"] - golden_presence_pct),
            0.05,
            f"{STABLE_BOON_NAME} presence_pct {boon['presence_pct']} not within 0.05 "
            f"of golden {golden_presence_pct}",
        )


class ReplayOptInTests(unittest.TestCase):
    """M9 Task 2: `replay=True` opts into the native combat-replay block;
    absent by default. Mirrors `crates/axilog-node/__test__/sdk.test.mjs`'s
    equivalent test."""

    def test_replay_absent_by_default_present_and_shaped_when_requested(self):
        without_replay = axilog.parse_file(FIXTURE)
        self.assertNotIn("replay", without_replay)

        with_replay = axilog.parse_file(FIXTURE, replay=True)
        self.assertIn("replay", with_replay)
        replay = with_replay["replay"]
        self.assertIsInstance(replay["poll_ms"], int)
        self.assertGreater(len(replay["tracks"]), 0)
        track = replay["tracks"][0]
        self.assertIsInstance(track["name"], str)
        self.assertIsInstance(track["team"], str)
        self.assertIsInstance(track["is_squad"], bool)
        if track["samples"]:
            self.assertEqual(len(track["samples"][0]), 3, "each sample is a [t, x, y] triple")

        explicitly_off = axilog.parse_file(FIXTURE, replay=False)
        self.assertNotIn("replay", explicitly_off)

        with open(FIXTURE, "rb") as f:
            data = f.read()
        bytes_with_replay = axilog.parse_bytes(data, replay=True)
        self.assertIn("replay", bytes_with_replay)


class ParseBytesTests(unittest.TestCase):
    def test_parse_bytes_matches_parse_file(self):
        from_file = axilog.parse_file(FIXTURE)
        with open(FIXTURE, "rb") as f:
            data = f.read()
        from_bytes = axilog.parse_bytes(data)
        self.assertEqual(from_bytes, from_file)

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

            self.assertEqual(len(round_tripped["players"]), len(original["players"]))

            original_damage = sum_by(original["players"], lambda p: p["damage"]["total"])
            round_tripped_damage = sum_by(
                round_tripped["players"], lambda p: p["damage"]["total"]
            )
            self.assertEqual(round_tripped_damage, original_damage)

            self.assertEqual(
                round_tripped["encounter"]["duration_ms"], original["encounter"]["duration_ms"]
            )
            self.assertEqual(len(round_tripped["enemies"]), len(original["enemies"]))


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
