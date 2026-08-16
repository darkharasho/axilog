#!/usr/bin/env python3
"""Regenerate `analysis::instant_cast::catalog` from the GW2EI C# sources.

GW2EI declares 649 `InstantCastFinder`s across ~44 profession-helper
files. Every one is a constructor plus a builder chain over a fixed
vocabulary, so they are EXTRACTED rather than hand-transcribed -- the same
call `gen_damage_mod_catalog.py` makes about the damage-modifier
catalogue, and for the same reason: a hand-copied table of that size is
neither reviewable nor verifiable.

Nothing here guesses. A finder subclass this project cannot evaluate, an
unresolved skill/species/build symbol, a non-literal constructor argument,
an arbitrary `.UsingChecker(lambda)` or any unhandled builder method
raises `Skip`, which lands the finder in the skipped table WITH its reason
instead of producing a subtly wrong definition. That distinction matters
more here than it does for damage modifiers: dropping an unrepresentable
CHECKER would WIDEN a finder, making it fire on events Elite Insights
never counts, so a widened finder is worse than a missing one.

The accounting it prints -- `considered == transcribed + skipped` -- is
the completeness claim, and is why this script is committed rather than
left as a scratch file: the claim is only worth anything if it can be
re-run.

Usage:

    python3 scripts/gen_instant_cast_catalog.py [/path/to/GW2EI/checkout]

then `git diff`: a clean tree means the committed catalog is exactly what
the current GW2EI source produces. Standard library only.
"""

import collections
import glob
import os
import re
import sys

ROOT = sys.argv[1] if len(sys.argv) > 1 else "/var/tmp/gw2ei"
OUT = os.path.normpath(
    os.path.join(
        os.path.dirname(os.path.abspath(__file__)),
        "..", "crates", "axilog-core", "src", "analysis", "instant_cast", "catalog",
    )
)


class Skip(Exception):
    """A finder that cannot be transcribed faithfully, with the reason."""


def rs_id(n):
    """A GW2EI skill/buff/species id as a Rust `u32` literal.

    GW2EI uses NEGATIVE ids as pseudo-skill sentinels (weaver attunement
    pairs, `WeaponSwap = -2`). This project already carries that
    convention -- `analysis::skill_map::WEAPON_SWAP_SKILL_ID` is
    `(-2i32) as u32` -- so the same reinterpretation is used here rather
    than inventing a second encoding.
    """
    return f"(({n}i32) as u32)" if n < 0 else str(n)


# ----------------------------------------------------------------------
# GW2EI constant tables
# ----------------------------------------------------------------------

def _lines(path):
    full = os.path.join(ROOT, path)
    if not os.path.exists(full):
        return []
    return open(full, encoding="utf-8-sig").read().splitlines()


# `GW2Builds` mixes plain integers with `ulong.MinValue`/`MaxValue`
# aliases for StartOfLife/EndOfLife.
BUILDS = {}
for line in _lines("GW2EIEvtcParser/ParserHelpers/GW2Builds.cs"):
    m = re.match(r"\s*public const (?:ulong|long) (\w+)\s*=\s*(\w+)(?:\.(\w+))?;", line)
    if m:
        v = m.group(2)
        if v == "ulong":
            BUILDS[m.group(1)] = 0 if "MinValue" in line else 2 ** 64 - 1
        else:
            BUILDS[m.group(1)] = int(v)

# `ArcDPSBuilds` is a NESTED class inside ArcDPSEnums.cs, not a file of
# its own. Pointing at a non-existent `ArcDPSBuilds.cs` silently left this
# table empty, which no finder happened to depend on -- no
# `InstantCastFinder` construction calls `.WithEvtcBuilds(...)` today -- but
# the first one that did would have been skipped for an "unresolved build
# symbol" that was really a bad path. The assertion below is the guard.
ARC_BUILDS = {}
for line in _lines("GW2EIEvtcParser/ParserHelpers/ArcDPSEnums.cs"):
    m = re.match(r"\s*public const (?:ulong|int|long) (\w+)\s*=\s*(-?\w+)", line)
    if m:
        v = m.group(2)
        if v.lstrip("-").isdigit():
            ARC_BUILDS[m.group(1)] = int(v)
        elif "MinValue" in line:
            ARC_BUILDS[m.group(1)] = "MIN"
        elif "MaxValue" in line:
            ARC_BUILDS[m.group(1)] = "MAX"

SKILLS = {}
for line in _lines("GW2EIEvtcParser/ParserHelpers/IDs/SkillIDs.cs"):
    m = re.match(r"\s*public const long (\w+)\s*=\s*(-?\d+);", line)
    if m:
        SKILLS[m.group(1)] = int(m.group(2))

# `MinionID` and `TargetID`/`TrashID` all live in SpeciesIDs.cs as plain
# `Name = 1234,` enum members; the enums do not overlap in the names the
# finders use, so one flat table is enough.
SPECIES = {}
for line in _lines("GW2EIEvtcParser/ParserHelpers/IDs/SpeciesIDs.cs"):
    m = re.match(r"\s*(\w+)\s*=\s*(-?\d+),?\s*(?://.*)?$", line)
    if m:
        SPECIES.setdefault(m.group(1), int(m.group(2)))

# Stable 16-byte effect GUIDs, `Name -> bytes`. The hex string is consumed
# a byte pair at a time in written order (`GUID(ReadOnlySpan<char> hex)`),
# which is the same order `evtc::guid` lays out `src_agent ++ dst_agent`,
# so the bytes compare directly with no endianness dance.
EFFECT_GUIDS = {}
for line in _lines("GW2EIEvtcParser/ParserHelpers/GUIDs/EffectGUIDs.cs"):
    m = re.match(r'\s*public static readonly GUID (\w+)\s*=\s*new\("([0-9A-Fa-f]{32})"\)', line)
    if m:
        EFFECT_GUIDS[m.group(1)] = bytes.fromhex(m.group(2))

for _name, _table in (("GW2Builds", BUILDS), ("ArcDPSBuilds", ARC_BUILDS),
                      ("SkillIDs", SKILLS), ("SpeciesIDs", SPECIES),
                      ("EffectGUIDs", EFFECT_GUIDS)):
    if not _table:
        sys.exit(f"{_name} table came back empty -- wrong path or changed syntax")


# ----------------------------------------------------------------------
# C# statement extraction
# ----------------------------------------------------------------------

def _match_paren(text, open_idx):
    """Index of the ')' closing the '(' at `open_idx`, skipping strings."""
    d = 0
    k = open_idx
    while k < len(text):
        c = text[k]
        if c == "(":
            d += 1
        elif c == ")":
            d -= 1
            if d == 0:
                return k
        elif c == '"':
            k += 1
            while text[k] != '"':
                if text[k] == "\\":
                    k += 1
                k += 1
        k += 1
    raise Skip("unbalanced parentheses")


def strip_comments(text):
    """Blank out C# comments, preserving line numbers and string literals.

    Not cosmetic: `GuardianHelper.cs:25-27` carries three commented-out
    `BuffLossCastFinder`s in an OLD three-argument form. Parsing them
    produced three phantom finders and three phantom skip reasons -- and,
    worse, a commented-out finder that still parsed cleanly would have
    been transcribed as live.
    """
    out = []
    i = 0
    n = len(text)
    while i < n:
        c = text[i]
        if c == '"':
            j = i + 1
            while j < n and text[j] != '"':
                if text[j] == "\\":
                    j += 1
                j += 1
            out.append(text[i:j + 1])
            i = j + 1
        elif text.startswith("//", i):
            j = text.find("\n", i)
            j = n if j < 0 else j
            out.append(" " * (j - i))
            i = j
        elif text.startswith("/*", i):
            j = text.find("*/", i + 2)
            j = n if j < 0 else j + 2
            # Keep the newlines so line numbers survive.
            out.append("".join("\n" if ch == "\n" else " " for ch in text[i:j]))
            i = j
        else:
            out.append(c)
            i += 1
    return "".join(out)


def find_stmts(text):
    """Yield (line, ctor, argstr, chain) per `new XxxCastFinder(...)`."""
    # `\w*CastFinder\w*`, not `\w*CastFinder`: several subclasses carry a
    # SUFFIX (`EffectCastFinderByDst`). Anchoring on the bare name made 63
    # constructions invisible to the regex entirely -- neither transcribed
    # nor skipped, so the accounting balanced while under-counting the
    # source. Any name change now shows up as an "unknown subclass" skip.
    for m in re.finditer(r"new\s+(\w*CastFinder\w*)\s*\(", text):
        ctor = m.group(1)
        open_idx = m.end() - 1
        close = _match_paren(text, open_idx)
        args = text[open_idx + 1:close]
        k = close + 1
        chain = []
        while True:
            mm = re.match(r"\s*\.(\w+)\(", text[k:])
            if not mm:
                break
            s = k + mm.end() - 1
            p = _match_paren(text, s)
            chain.append((mm.group(1), text[s + 1:p]))
            k = p + 1
        yield (text[:m.start()].count("\n") + 1, ctor, args, chain)


def split_top(s):
    """Split on top-level commas, ignoring nesting and string literals."""
    out = []
    d = 0
    cur = ""
    instr = False
    i = 0
    while i < len(s):
        ch = s[i]
        if instr:
            cur += ch
            if ch == "\\":
                cur += s[i + 1]
                i += 2
                continue
            if ch == '"':
                instr = False
            i += 1
            continue
        if ch == '"':
            instr = True
            cur += ch
            i += 1
            continue
        if ch in "([{":
            d += 1
        elif ch in ")]}":
            d -= 1
        if ch == "," and d == 0:
            out.append(cur.strip())
            cur = ""
        else:
            cur += ch
        i += 1
    if cur.strip():
        out.append(cur.strip())
    return out


# ----------------------------------------------------------------------
# expression resolution
# ----------------------------------------------------------------------

def resolve_id(expr):
    """A skill or buff id: a literal, a `SkillIDs` symbol, or a cast."""
    e = expr.strip().rstrip("L")
    e = re.sub(r"^\(\s*long\s*\)\s*", "", e).strip()
    if re.fullmatch(r"-?\d+", e):
        return int(e)
    e = e.split(".")[-1]
    if e in SKILLS:
        return SKILLS[e]
    raise Skip(f"unresolved skill symbol `{expr.strip()}`")


def resolve_species(expr):
    e = expr.strip()
    e = re.sub(r"^\(\s*int\s*\)\s*", "", e).strip()
    if re.fullmatch(r"-?\d+", e):
        return int(e)
    e = e.split(".")[-1]
    if e in SPECIES:
        return SPECIES[e]
    raise Skip(f"unresolved species symbol `{expr.strip()}`")


def file_species_sets(text):
    """Named `HashSet<int>` species collections declared in one helper.

    `RangerHelper` passes its 120-odd `JuvenilePetIDs` set to the
    pet-spawn finder by NAME. Resolving it is worth the parsing: that one
    finder is every ranger pet spawn in the log.

    The declarations are matched in source order, which is what lets a
    set built by `.Union(...)` of earlier sets resolve -- `JuvenilePetIDs`
    is exactly that, a short literal list unioned with eleven per-family
    sets. Missing the union chain would have silently produced a
    17-species pet finder instead of a 69-species one.
    """
    out = {}
    for m in re.finditer(
        r"(?:HashSet|List|IReadOnlyList|IEnumerable)<int>\s+(\w+)\s*=\s*"
        r"(?:new\s+[\w<>\s]*)?[\{\[](.*?)[\}\]]"
        r"((?:\s*\.\w+\([^()]*\))*)\s*;",
        text,
        re.S,
    ):
        try:
            ids = [resolve_species(p) for p in split_top(m.group(2)) if p.strip()]
            for call, arg in re.findall(r"\.(\w+)\(([^()]*)\)", m.group(3)):
                if call in ("ToHashSet", "ToList", "AsReadOnly"):
                    continue
                if call != "Union" or arg.strip() not in out:
                    raise Skip(f"unresolvable set operation `.{call}({arg.strip()})`")
                ids.extend(out[arg.strip()])
        except Skip:
            # An unresolvable member or operation makes the whole set
            # unusable; leaving it out means the finder that names it is
            # skipped WITH a reason rather than transcribed short.
            continue
        out[m.group(1)] = sorted(set(ids))
    return out


def resolve_species_list(expr, named):
    """A `MinionSpawnCastFinder` species argument: one id or a collection."""
    e = expr.strip()
    m = re.fullmatch(r"(?:new\s+[\w<>\[\], ]*)?[\[\{](.*)[\]\}]", e, re.S)
    if m:
        return [resolve_species(p) for p in split_top(m.group(1)) if p.strip()]
    if e in named:
        return named[e]
    return [resolve_species(e)]


def resolve_int(expr):
    e = expr.strip().rstrip("L")
    if re.fullmatch(r"-?\d+", e):
        return int(e)
    if e.split(".")[-1] == "ServerDelayConstant":
        return 150
    if e.split(".")[-1] == "DefaultICD":
        return 50
    raise Skip(f"non-literal integer `{expr.strip()}`")


def resolve_guid(expr):
    """An `EffectGUIDs.Name` reference -> its Rust const identifier."""
    e = expr.strip().split(".")[-1]
    if e not in EFFECT_GUIDS:
        raise Skip(f"unresolved effect GUID `{expr.strip()}`")
    return e


def resolve_spec_list(expr):
    """One `Spec.X` or a `[Spec.X, Spec.Y]` collection expression."""
    e = expr.strip()
    m = re.fullmatch(r"(?:new\s+[\w<>\[\], ]*)?[\[\{](.*)[\]\}]", e, re.S)
    parts = split_top(m.group(1)) if m else [e]
    return [resolve_spec(p) for p in parts if p.strip()]


def resolve_spec(expr):
    e = expr.strip().split(".")[-1]
    if not re.fullmatch(r"\w+", e):
        raise Skip(f"non-literal spec `{expr.strip()}`")
    return e


def resolve_build(expr, table, lo_name, hi_name):
    e = expr.strip()
    key = e.split(".")[-1]
    if key in table:
        v = table[key]
        if v == "MIN":
            return lo_name
        if v == "MAX":
            return hi_name
        return v
    if re.fullmatch(r"-?\d+", e):
        return int(e)
    raise Skip(f"unresolved build symbol `{e}`")


# ----------------------------------------------------------------------
# constructor -> Trigger
# ----------------------------------------------------------------------

# Subclasses this project's engine evaluates. The value is a builder
# taking the split constructor arguments and returning the Rust `Trigger`
# expression.
def _t_buff(kind):
    def build(args):
        if len(args) != 2:
            raise Skip(f"{kind} takes (skillID, buffID); got {len(args)} args")
        return f"Trigger::{kind} {{ buff_id: {rs_id(resolve_id(args[1]))} }}", rs_id(resolve_id(args[0]))
    return build


def _t_skill(kind):
    def build(args):
        if len(args) != 2:
            raise Skip(f"{kind} takes (skillID, triggerID); got {len(args)} args")
        return f"Trigger::{kind} {{ skill_id: {rs_id(resolve_id(args[1]))} }}", rs_id(resolve_id(args[0]))
    return build


def _t_minion_command(args):
    if len(args) != 2:
        raise Skip("MinionCommandCastFinder takes (skillID, speciesID)")
    return (
        f"Trigger::MinionCommand {{ species_id: {rs_id(resolve_species(args[1]))} }}",
        rs_id(resolve_id(args[0])),
    )


def _t_minion_spawn(args, named):
    if len(args) != 2:
        raise Skip("MinionSpawnCastFinder takes (skillID, species)")
    ids = resolve_species_list(args[1], named)
    body = ", ".join(rs_id(i) for i in ids)
    return f"Trigger::MinionSpawn {{ species_ids: &[{body}] }}", rs_id(resolve_id(args[0]))


def _t_effect(by_dst):
    def build(args):
        if len(args) != 2:
            raise Skip("EffectCastFinder takes (skillID, effectGUID)")
        guid = resolve_guid(args[1])
        return (
            f"Trigger::Effect {{ guid: &{guid_const(guid)}, "
            f"by_dst: {str(by_dst).lower()} }}",
            rs_id(resolve_id(args[0])),
        )
    return build


def guid_const(name):
    """The Rust const identifier a GUID symbol is emitted under."""
    return "G_" + re.sub(r"(?<!^)(?=[A-Z0-9])", "_", name).upper().replace("__", "_")


CTORS = {
    "EffectCastFinder": _t_effect(False),
    "EffectCastFinderByDst": _t_effect(True),
    "BuffGainCastFinder": _t_buff("BuffGain"),
    "BuffLossCastFinder": _t_buff("BuffLoss"),
    "BuffGiveCastFinder": _t_buff("BuffGive"),
    "BuffExtendCastFinder": _t_buff("BuffExtend"),
    "DamageCastFinder": _t_skill("Damage"),
    "BreakbarDamageCastFinder": _t_skill("BreakbarDamage"),
    "MinionCastCastFinder": _t_skill("MinionCast"),
    "MissileCastFinder": _t_skill("Missile"),
    "EXTHealingCastFinder": _t_skill("ExtHealing"),
    "MinionCommandCastFinder": _t_minion_command,
    "MinionSpawnCastFinder": _t_minion_spawn,
}

# Subclasses deliberately out of scope, with the reason recorded in the
# emitted table rather than left implicit.
UNSUPPORTED = {
    "MarkerCastFinder": "marker-effect events are not decoded by this project",
    "EXTBarrierCastFinder": "the barrier extension is not decoded by this project",
    "BandTogetherCastFinder": "bespoke subclass with log-specific state",
    "WeaponSwapCastFinder": "weapon-swap events are not decoded by this project",
}

# The buff-side checkers name the RECIPIENT (`To`) or the APPLIER (`By`);
# the effect-side ones name `Src`/`Dst`. `Party` is a property of the
# EVENT, not of the finder, so `To`/`Src` map to `Key` and `By`/`Dst` to
# `Other` regardless of which one the subclass calls its caster.
#
# The fourth element is the implied `IsAroundDst` guard: GW2EI's
# `UsingDst*Checker` bodies all begin `evt.IsAroundDst &&`, because an
# effect that sits on the ground has no Dst to read a spec off.
SPEC_CHECKERS = {
    "UsingToSpecChecker": ("Key", False, False, False),
    "UsingToNotSpecChecker": ("Key", False, True, False),
    "UsingToBaseSpecChecker": ("Key", True, False, False),
    "UsingBySpecChecker": ("Other", False, False, False),
    "UsingByNotSpecChecker": ("Other", False, True, False),
    "UsingByBaseSpecChecker": ("Other", True, False, False),
    "UsingSrcSpecChecker": ("Key", False, False, False),
    "UsingSrcSpecsChecker": ("Key", False, False, False),
    "UsingSrcNotSpecChecker": ("Key", False, True, False),
    "UsingSrcNotSpecsChecker": ("Key", False, True, False),
    "UsingSrcBaseSpecChecker": ("Key", True, False, False),
    "UsingSrcNotBaseSpecChecker": ("Key", True, True, False),
    "UsingDstSpecChecker": ("Other", False, False, True),
    "UsingDstSpecsChecker": ("Other", False, False, True),
    "UsingDstNotSpecChecker": ("Other", False, True, True),
    "UsingDstNotSpecsChecker": ("Other", False, True, True),
    "UsingDstBaseSpecChecker": ("Other", True, False, True),
    "UsingDstNotBaseSpecChecker": ("Other", True, True, True),
}

# The `Using[No]SecondaryEffect*Checker` family: twelve builder methods
# that differ only in `(inverted_src, type_rel, negated)`.
SECONDARY_CHECKERS = {
    "UsingSecondaryEffectSameSrcChecker": (False, "Any", False),
    "UsingSecondaryEffectInvertedSrcChecker": (True, "Any", False),
    "UsingSecondaryEffectSameSrcSameTypeChecker": (False, "Same", False),
    "UsingSecondaryEffectInvertedSrcSameTypeChecker": (True, "Same", False),
    "UsingSecondaryEffectSameSrcInvertedTypeChecker": (False, "Inverted", False),
    "UsingSecondaryEffectInvertedSrcInvertedTypeChecker": (True, "Inverted", False),
    "UsingNoSecondaryEffectSameSrcChecker": (False, "Any", True),
    "UsingNoSecondaryEffectInvertedSrcChecker": (True, "Any", True),
    "UsingNoSecondaryEffectSameSrcSameTypeChecker": (False, "Same", True),
    "UsingNoSecondaryEffectInvertedSrcSameTypeChecker": (True, "Same", True),
    "UsingNoSecondaryEffectSameSrcInvertedTypeChecker": (False, "Inverted", True),
    "UsingNoSecondaryEffectInvertedSrcInvertedTypeChecker": (True, "Inverted", True),
}


def analyse(ctor, argstr, chain, named):
    """One C# statement -> a Rust `FinderDef` literal, or `Skip`."""
    if ctor in UNSUPPORTED:
        raise Skip(UNSUPPORTED[ctor])
    if ctor not in CTORS:
        raise Skip(f"unknown finder subclass `{ctor}`")

    args = split_top(argstr)
    build = CTORS[ctor]
    trigger, skill_id = (
        build(args, named) if ctor == "MinionSpawnCastFinder" else build(args)
    )

    is_effect = ctor in ("EffectCastFinder", "EffectCastFinderByDst")
    fields = {}
    checks = []
    enable = []
    # Every GUID this finder names, so the emitter can declare a const for
    # each one used in the file rather than inlining 16 bytes per mention.
    guids = set()
    if is_effect:
        guids.add(resolve_guid(args[1]))
    for name, arg in chain:
        parts = split_top(arg)
        if name == "WithBuilds":
            fields["min_gw2_build"] = resolve_build(
                parts[0], BUILDS, "START_OF_LIFE", "END_OF_LIFE")
            if len(parts) > 1:
                fields["max_gw2_build"] = resolve_build(
                    parts[1], BUILDS, "START_OF_LIFE", "END_OF_LIFE")
        elif name == "WithEvtcBuilds":
            fields["min_evtc_build"] = resolve_build(
                parts[0], ARC_BUILDS, "EVTC_START_OF_LIFE", "EVTC_END_OF_LIFE")
            if len(parts) > 1:
                fields["max_evtc_build"] = resolve_build(
                    parts[1], ARC_BUILDS, "EVTC_START_OF_LIFE", "EVTC_END_OF_LIFE")
        elif name == "UsingOrigin":
            fields["origin"] = "CastOrigin::" + resolve_spec(parts[0])
        elif name == "UsingNotAccurate":
            fields["not_accurate"] = True
        elif name == "WithMinions":
            fields["minions"] = True
        elif name == "UsingICD":
            fields["icd"] = resolve_int(parts[0]) if parts else 50
        elif name == "UsingTimeOffset":
            fields["time_offset"] = resolve_int(parts[0])
        elif name == "UsingBeforeWeaponSwap":
            fields["swap_snap"] = "SwapSnap::Before"
        elif name == "UsingAfterWeaponSwap":
            fields["swap_snap"] = "SwapSnap::After"
        elif name == "UsingDisableWithEffectData":
            enable.append("Enable::NoEffectData")
        elif name == "UsingDisableWithMissileData":
            enable.append("Enable::NoMissileData")
        elif name in SPEC_CHECKERS:
            party, base, neg, around = SPEC_CHECKERS[name]
            specs = ", ".join(f'"{s}"' for s in resolve_spec_list(parts[0]))
            if around:
                checks.append("Check::AroundDst { negated: false }")
            checks.append(
                f"Check::Spec {{ party: Party::{party}, specs: &[{specs}], "
                f"base: {str(base).lower()}, negated: {str(neg).lower()} }}"
            )
        elif name in SECONDARY_CHECKERS:
            inv, rel, neg = SECONDARY_CHECKERS[name]
            guid = resolve_guid(parts[0])
            off = resolve_int(parts[1]) if len(parts) > 1 else 0
            eps = resolve_int(parts[2]) if len(parts) > 2 else 150
            checks.append(
                f"Check::SecondaryEffect {{ guid: &{guid_const(guid)}, "
                f"inverted_src: {str(inv).lower()}, type_rel: TypeRel::{rel}, "
                f"time_offset: {off}, epsilon: {eps}, negated: {str(neg).lower()} }}"
            )
            guids.add(guid)
        elif name == "UsingIsAroundDstChecker":
            checks.append("Check::AroundDst { negated: false }")
        elif name == "UsingNotIsAroundDstChecker":
            checks.append("Check::AroundDst { negated: true }")
        elif name == "UsingDurationChecker":
            if is_effect:
                # The effect form is exact equality, or an INCLUSIVE
                # range -- not the buff form's epsilon band. See
                # `Check::EffectDuration`.
                lo = resolve_int(parts[0])
                hi = resolve_int(parts[1]) if len(parts) > 1 else lo
                checks.append(f"Check::EffectDuration {{ min: {lo}, max: {hi} }}")
            else:
                dur = resolve_int(parts[0])
                eps = resolve_int(parts[1]) if len(parts) > 1 else 150
                checks.append(f"Check::Duration {{ duration: {dur}, epsilon: {eps} }}")
        elif name == "UsingNoAnimatedCastChecker":
            # `CombatData.IsCasting` asks whether the caster's animated
            # cast WINDOW (start plus actual duration) contains a time.
            # This project pairs cast starts with stops in
            # `analysis::rotation`, downstream of here, so the window is
            # not available -- and dropping the checker would widen the
            # finder onto casts EI excludes.
            raise Skip("animated-cast window intersection is not available here")
        elif name == "UsingChecker":
            # An arbitrary closure over the parsed log. Dropping it would
            # WIDEN the finder; see this script's docstring.
            raise Skip("arbitrary `.UsingChecker(lambda)` predicate")
        else:
            raise Skip(f"unhandled builder method `.{name}(...)`")

    # The healing extension's own enable condition is installed by the
    # ctor, not by a builder call.
    if ctor == "EXTHealingCastFinder":
        enable.append("Enable::HasExtHealing")

    # `EffectCastFinder`'s ctor installs `UsingNotAccurate()` and the
    # `HasEffectData` enable condition. Both are modelled on the TRIGGER
    # (`Trigger::forces_not_accurate` / `forces_has_effect_data`) rather
    # than emitted per row, so a hand-edited row cannot lose them.

    return skill_id, trigger, fields, checks, enable, guids


# ----------------------------------------------------------------------
# emission
# ----------------------------------------------------------------------

def rs_build(v, lo, hi):
    if isinstance(v, str):
        return v
    if v == 0:
        return lo
    if v >= 2 ** 64 - 1:
        return hi
    return str(v)


def emit_guid_consts(names):
    """The `G_*` byte-array consts a catalog file's finders reference."""
    out = []
    for n in sorted(names):
        body = ", ".join(f"0x{b:02X}" for b in EFFECT_GUIDS[n])
        out.append(f"/// `EffectGUIDs.{n}`.")
        out.append(f"const {guid_const(n)}: [u8; 16] = [{body}];")
    return "\n".join(out)


def emit_row(rec):
    skill_id, trigger, fields, checks, enable, _guids, source, line = rec
    out = [f"    FinderDef {{"]
    out.append(f"        skill_id: {skill_id},")
    out.append(f"        source: \"{source}\",")
    out.append(f"        trigger: {trigger},")
    for key in ("origin", "not_accurate", "minions", "icd", "time_offset", "swap_snap"):
        if key in fields:
            v = fields[key]
            v = str(v).lower() if isinstance(v, bool) else v
            out.append(f"        {key}: {v},")
    if enable:
        out.append(f"        enable: &[{', '.join(enable)}],")
    if checks:
        out.append("        checks: &[")
        for c in checks:
            out.append(f"            {c},")
        out.append("        ],")
    for key, lo, hi in (
        ("min_gw2_build", "START_OF_LIFE", "END_OF_LIFE"),
        ("max_gw2_build", "START_OF_LIFE", "END_OF_LIFE"),
        ("min_evtc_build", "EVTC_START_OF_LIFE", "EVTC_END_OF_LIFE"),
        ("max_evtc_build", "EVTC_START_OF_LIFE", "EVTC_END_OF_LIFE"),
    ):
        if key in fields:
            out.append(f"        {key}: {rs_build(fields[key], lo, hi)},")
    out.append("        ..FinderDef::DEFAULT")
    out.append("    },")
    return "\n".join(out)


def group_of(path):
    """The catalog file a helper's finders land in: its profession dir."""
    parts = os.path.normpath(path).split(os.sep)
    if "ProfHelpers" in parts:
        i = parts.index("ProfHelpers")
        if i + 2 < len(parts):
            return parts[i + 1].lower()
        return "shared"
    return "shared"


def main():
    files = sorted(
        glob.glob(ROOT + "/GW2EIEvtcParser/EIData/ProfHelpers/**/*.cs", recursive=True)
        + glob.glob(ROOT + "/GW2EIEvtcParser/Extensions/**/*.cs", recursive=True)
    )
    if not files:
        sys.exit(f"no GW2EI sources under {ROOT} -- pass the checkout path")

    kept = collections.defaultdict(list)
    skipped = collections.Counter()
    considered = 0
    per_ctor = collections.Counter()

    for path in files:
        text = strip_comments(open(path, encoding="utf-8-sig").read())
        named = file_species_sets(text)
        helper = os.path.basename(path)[:-3]
        for line, ctor, argstr, chain in find_stmts(text):
            considered += 1
            per_ctor[ctor] += 1
            try:
                skill_id, trigger, fields, checks, enable, guids = analyse(
                    ctor, argstr, chain, named)
            except Skip as e:
                skipped[str(e)] += 1
                continue
            kept[group_of(path)].append(
                (skill_id, trigger, fields, checks, enable, guids, helper, line)
            )

    os.makedirs(OUT, exist_ok=True)
    groups = sorted(kept)
    for g in groups:
        rows = sorted(kept[g], key=lambda r: (r[6], r[7]))
        body = "\n".join(emit_row(r) for r in rows)
        used = set().union(*(r[5] for r in rows)) if rows else set()
        consts = emit_guid_consts(used)
        with open(os.path.join(OUT, f"{g}.rs"), "w") as fh:
            fh.write(
                "// @generated by scripts/gen_instant_cast_catalog.py -- do not edit.\n"
                f"//! Instant-cast finders transcribed from GW2EI's `{g}` profession\n"
                "//! helpers. See `super`'s module doc for the extraction accounting.\n\n"
                "use crate::analysis::instant_cast::model::*;\n\n"
                + (consts + "\n\n" if consts else "")
                + f"pub const FINDERS: &[FinderDef] = &[\n{body}\n];\n"
            )

    total_kept = sum(len(v) for v in kept.values())
    total_skipped = sum(skipped.values())
    assert considered == total_kept + total_skipped, "accounting does not balance"

    with open(os.path.join(OUT, "mod.rs"), "w") as fh:
        fh.write(
            "// @generated by scripts/gen_instant_cast_catalog.py -- do not edit.\n"
            "//! The transcribed `InstantCastFinder` catalog.\n"
            "//!\n"
            f"//! Extracted from {len(files)} GW2EI source files: **{considered}** finder\n"
            f"//! constructions considered, **{total_kept}** transcribed,\n"
            f"//! **{total_skipped}** skipped. Every skip carries a named reason (below);\n"
            "//! nothing is approximated, because a finder that loses an\n"
            "//! unrepresentable checker would fire on events Elite Insights never\n"
            "//! counts -- worse than a missing finder, not better.\n"
            "//!\n"
            "//! Skips by reason:\n"
            "//!\n"
            + "".join(
                f"//! - {n} x {r}\n" for r, n in sorted(skipped.items(), key=lambda kv: -kv[1])
            )
            + "//!\n"
            "//! Constructions by subclass:\n"
            "//!\n"
            + "".join(
                f"//! - {n} x `{c}`\n" for c, n in sorted(per_ctor.items(), key=lambda kv: -kv[1])
            )
            + "\n"
            + "".join(f"pub mod {g};\n" for g in groups)
            + "\nuse crate::analysis::instant_cast::model::FinderDef;\n\n"
            "/// Every transcribed finder, in catalog order.\n"
            "pub fn all() -> Vec<&'static FinderDef> {\n"
            "    [\n"
            + "".join(f"        {g}::FINDERS,\n" for g in groups)
            + "    ]\n    .iter()\n    .flat_map(|s| s.iter())\n    .collect()\n}\n"
        )

    print(f"considered {considered} = transcribed {total_kept} + skipped {total_skipped}")
    for r, n in sorted(skipped.items(), key=lambda kv: -kv[1]):
        print(f"  skip {n:4d}  {r}")
    for g in groups:
        print(f"  {g}: {len(kept[g])}")


if __name__ == "__main__":
    main()
