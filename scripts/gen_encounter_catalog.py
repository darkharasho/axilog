#!/usr/bin/env python3
"""Regenerate `pve::encounters` from the GW2EI C# sources.

This rewrites the target file WHOLE. Anything hand-added to it -- a
`mod tests`, a helper -- is deleted on the next run, silently and with a
green suite. Put new tests in `crates/axilog-core/tests/encounter_catalog.rs`,
where regeneration cannot reach them (the same split
`gen_marker_catalog.py` uses).

arcdps writes the encounter's *trigger species id* into bytes 13-14 of the
evtc header and nothing else: no name, no category, no "this was a raid".
Every PvE log this project has ever parsed therefore came out labelled
"Detailed WvW - World vs World", because `model::resolve` had no table to
ask (axibridge issue: raid/strike/fractal logs all reading as WvW).

GW2EI keeps that table, spread across two files, and this script joins them:

  SpeciesIDs.TargetID           enum member -> trigger id. Members alias each
                                other and the bare `SpeciesIDs` consts, so the
                                values are resolved symbolically, not textually.
  LogData.DetectLogic           the `switch (targetID)` that maps a trigger id
                                to the `LogLogic` subclass that handles it.

and then walks each subclass's base chain for the two facts EI stores on
the class rather than in a table:

  LogCategoryInformation.Category      Fractal / RaidWing / RaidEncounter / ...
  LogCategoryInformation.SubCategory   SpiritVale / ShatteredObservatory / ...

Both are read INDEPENDENTLY up the chain. They are routinely set at
different depths -- `Gorseval : SpiritVale : RaidLogic` gets its
sub-category from the wing and its category from the wing's own base -- so
stopping at the first ancestor that names either one loses the other.

## Names

EI's *default* fight name (`LogLogic.GetLogicName`) is "the character name
of the target whose species is the trigger id" -- i.e. the boss's own agent
name, straight out of the log's agent table. That needs no table at all,
and it is what `pve::resolve_name` does at parse time.

Only the ~20 logics that OVERRIDE `GetLogicName` with a constant need to be
carried here, and they are exactly the encounters whose name is not any one
agent's name: multi-boss fights ("Twin Largos", "Bandit Trio"), event
encounters ("Siege the Stronghold", "Spirit Race"), and the EoD/SotO strikes
EI names after the instance ("Harvest Temple", "Kaineng Overlook"). Those
become `name: Some(..)`; everything else is `name: None`, meaning "ask the
agent table".

An override whose body is anything other than a single `return "...";` --
`AiKeeperOfThePeak` picks between three names by which mode the log is in,
`WvWLogic` and `UnknownInstanceLogic` read the map -- cannot be transcribed
as a constant. Those land in the skipped table WITH the reason and fall back
to the agent name, which for all three is a reasonable answer rather than a
wrong one.

## Conditional cases

A handful of `case` arms return DIFFERENT logics depending on what else is
in the log: a Xera id with haunting statues before it is really a Twisted
Castle log, a Dhuum id with eyes is really a Statue of Darkness log. This
script records the arm's *unconditional* logic (the one EI falls through
to), flags the id in the doc header, and does not attempt the redirect --
that needs the agent table, which a static catalog does not have.

Nothing here guesses. A case whose logic class cannot be found, a TargetID
member that will not resolve to an integer, or a trigger id claimed by two
different logics all raise `Skip` and land in the skipped table with the
reason instead of producing a wrong entry. The accounting printed at the
end -- `considered == transcribed + skipped` -- is the machine-diff behind
this catalog's completeness claim.

Usage:

    python3 scripts/gen_encounter_catalog.py [/path/to/GW2EI/checkout]

then `git diff`: a clean tree means the committed catalog is exactly what
the current GW2EI source produces. Standard library only.
"""

import collections
import glob
import os
import re
import sys

ROOT = sys.argv[1] if len(sys.argv) > 1 else "/tmp/gw2ei"
PARSER = os.path.join(ROOT, "GW2EIEvtcParser")
SPECIES = os.path.join(PARSER, "ParserHelpers/IDs/SpeciesIDs.cs")
LOGDATA = os.path.join(PARSER, "ParsedData/LogData.cs")
LOGIC_DIR = os.path.join(PARSER, "LogLogic")
OUT = os.path.normpath(
    os.path.join(
        os.path.dirname(os.path.abspath(__file__)),
        "..", "crates", "axilog-core", "src", "pve", "encounters.rs",
    )
)

# EI's `LogCategory` enum -> the slug `Encounter::kind` carries. EI's own
# vocabulary is kept verbatim rather than collapsed into "raid"/"strike":
# `RaidEncounter` spans festival bosses, IBS/EoD strikes AND the SotO and
# Visions of Eternity encounters, so any collapse would mislabel some of them.
CATEGORY_SLUGS = {
    "Fractal": "fractal",
    "RaidEncounter": "raid_encounter",
    "RaidWing": "raid_wing",
    "WvW": "wvw",
    "Golem": "golem",
    "Story": "story",
    "OpenWorld": "open_world",
    "Convergence": "convergence",
    "UnknownEncounter": "unknown_encounter",
    "Unknown": "unknown",
}


class Skip(Exception):
    """An entry that cannot be transcribed, carrying the reason why."""


def read(path):
    with open(path, encoding="utf-8-sig") as fh:
        return fh.read()


# ---------------------------------------------------------------- TargetID

def target_ids():
    """`TargetID` member name -> integer trigger id.

    Members alias one another (`_EtherealBarrier1 = EtherealBarrier1`) and
    the bare `SpeciesIDs` consts outside the enum, so every right-hand side
    is resolved symbolically until it bottoms out in a literal. A member
    that never bottoms out is dropped rather than guessed at.
    """
    src = read(SPECIES)
    raw = {}
    for m in re.finditer(r"^\s*([A-Za-z_]\w*)\s*=\s*([^,;]+?)\s*,\s*(?://.*)?$", src, re.M):
        raw.setdefault(m.group(1), m.group(2).strip())
    for m in re.finditer(r"const\s+int\s+([A-Za-z_]\w*)\s*=\s*([^;]+);", src):
        raw.setdefault(m.group(1), m.group(2).strip())

    def resolve(name, seen=()):
        if name in seen:
            return None
        value = raw.get(name)
        if value is None:
            return None
        value = value.replace("SpeciesIDs.", "").strip()
        if re.fullmatch(r"-?\d+", value):
            return int(value)
        if re.fullmatch(r"0[xX][0-9a-fA-F]+", value):
            return int(value, 16)
        if re.fullmatch(r"[A-Za-z_]\w*", value):
            return resolve(value, seen + (name,))
        return None

    body = src[src.index("enum TargetID"):]
    depth, end = 0, len(body)
    for i, ch in enumerate(body):
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                end = i
                break
    body = body[:end]

    out = {}
    for m in re.finditer(r"^\s*([A-Za-z_]\w*)\s*=", body, re.M):
        value = resolve(m.group(1))
        if value is not None:
            out[m.group(1)] = value
    return out


# ------------------------------------------------------------- DetectLogic

def detect_logic_cases():
    """`TargetID` member name -> (logic class, is_conditional).

    `is_conditional` marks an arm with no top-level `return new` -- every
    return sits inside an `if`, so EI's choice depends on the agent table
    and this catalog records only the arm's last (fall-through) logic.
    """
    src = read(LOGDATA)
    body = src[src.index("internal static LogLogic.LogLogic DetectLogic"):
               src.index("internal void CompleteLogName")]

    groups, labels, block = [], [], []
    for line in body.splitlines():
        m = re.match(r"case TargetID\.(\w+):$", line.strip())
        if m:
            if block:
                groups.append((labels, block))
                labels, block = [], []
            labels.append(m.group(1))
            continue
        if labels:
            block.append(line)
    if labels:
        groups.append((labels, block))

    out = {}
    for labels, block in groups:
        depth, top, last = 0, None, None
        for line in block:
            m = re.search(r"return new (\w+)\(", line)
            if m:
                last = m.group(1)
                if depth == 0:
                    top = m.group(1)
            depth += line.count("{") - line.count("}")
        if last is None:
            continue
        for label in labels:
            out[label] = (top or last, top is None)
    return out


# ------------------------------------------------------------ logic classes

def logic_classes():
    """Class name -> (source path, base class, file text)."""
    out = {}
    for path in sorted(glob.glob(os.path.join(LOGIC_DIR, "**", "*.cs"), recursive=True)):
        text = read(path)
        for m in re.finditer(
            r"^\s*(?:internal|public|abstract|sealed|partial|\s)*class\s+(\w+)\s*(?::\s*([\w<>, ]+))?",
            text, re.M,
        ):
            out.setdefault(m.group(1), (path, (m.group(2) or "").split(",")[0].strip(), text))
    return out


CATEGORY_RE = re.compile(r"Category\s*=\s*(?:LogCategories\.)?LogCategory\.(\w+)")
SUBCATEGORY_RE = re.compile(r"SubCategory\s*=\s*(?:LogCategories\.)?SubLogCategory\.(\w+)")


def walk_up(classes, klass, pattern):
    """First match for `pattern` walking `klass`'s base chain, or None.

    Category and sub-category are looked up with two separate walks on
    purpose: they are set at different depths of the chain (see module doc).

    A class that assigns the pattern MORE THAN ONCE, with different values,
    has no single answer -- `WvWLogic` picks its sub-category from the map
    id inside a `switch`, so the first assignment in the file
    ("EternalBattlegrounds") is one arm of a choice, not the class's
    identity. Those raise `Skip`; taking the first match would be exactly
    the kind of guess this generator refuses to make.
    """
    seen = set()
    while klass in classes and klass not in seen:
        seen.add(klass)
        _path, base, text = classes[klass]
        found = set(pattern.findall(text))
        if len(found) > 1:
            raise Skip("%s assigns %s conditionally (%s)"
                       % (klass, pattern.pattern.split("\\")[0],
                          ", ".join(sorted(found))))
        if found:
            return found.pop()
        klass = base
    return None


def logic_name_override(classes, klass):
    """`(name, None)` for a constant override, `(None, reason)` otherwise."""
    if klass not in classes:
        return None, None
    text = classes[klass][2]
    i = text.find("override string GetLogicName")
    if i < 0:
        return None, None
    j = text.find("{", i)
    depth, end = 0, j
    for n, ch in enumerate(text[j:]):
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                end = j + n + 1
                break
    m = re.fullmatch(r'\{\s*return\s+"([^"]*)";\s*\}', text[j:end].strip())
    if m:
        return m.group(1), None
    return None, "GetLogicName override is not a single constant return"


# ------------------------------------------------------------------- emit

def rust_str(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def main():
    ids = target_ids()
    cases = detect_logic_cases()
    classes = logic_classes()

    rows, skipped, conditional = [], [], []
    for member in sorted(cases):
        klass, is_conditional = cases[member]
        try:
            if member not in ids:
                raise Skip("TargetID member does not resolve to an integer")
            if klass not in classes:
                raise Skip("logic class %s has no source file" % klass)
            category = walk_up(classes, klass, CATEGORY_RE)
            if category is None:
                raise Skip("logic class %s declares no LogCategory" % klass)
            if category not in CATEGORY_SLUGS:
                raise Skip("unmapped LogCategory %s" % category)
            # An ambiguous sub-category drops the FIELD, not the row: the
            # id, its category and its name are all still known and useful.
            # An ambiguous category drops the row, because `kind` is what
            # the field exists to answer.
            try:
                sub = walk_up(classes, klass, SUBCATEGORY_RE)
            except Skip as exc:
                skipped.append(("%s (sub_category only)" % member, str(exc)))
                sub = None
            name, reason = logic_name_override(classes, klass)
        except Skip as exc:
            skipped.append((member, str(exc)))
            continue
        if reason:
            skipped.append(("%s (name only)" % member, reason))
        if is_conditional:
            conditional.append(member)
        rows.append({
            "id": ids[member],
            "member": member,
            "logic": klass,
            "category": CATEGORY_SLUGS[category],
            "sub": sub,
            "name": name,
        })

    by_id = collections.defaultdict(list)
    for row in rows:
        by_id[row["id"]].append(row)
    kept = []
    for trigger_id, group in sorted(by_id.items()):
        logics = {r["logic"] for r in group}
        if len(logics) > 1:
            skipped.append((
                "id %d" % trigger_id,
                "claimed by %s" % " and ".join(sorted(logics)),
            ))
            continue
        kept.append(group[0])
    kept.sort(key=lambda r: r["id"])

    considered = len(cases)
    named = sum(1 for r in kept if r["name"])
    cats = collections.Counter(r["category"] for r in kept)

    skip_table = "//! Skipped:\n" + "\n".join(
        "//!   %-44s %s" % (what, why) for what, why in sorted(skipped)
    ) if skipped else "//! Nothing was skipped."
    cond_note = ", ".join(sorted(conditional)) or "none"
    cat_lines = "\n".join(
        "//!   %-20s %d" % (slug, n) for slug, n in sorted(cats.items())
    )

    body = []
    for r in kept:
        name = "Some(%s)" % rust_str(r["name"]) if r["name"] else "None"
        sub = "Some(%s)" % rust_str(r["sub"]) if r["sub"] else "None"
        body.append(
            "    Encounter { trigger_id: %d, member: %s, logic: %s, "
            "category: %s, sub_category: %s, name: %s },"
            % (r["id"], rust_str(r["member"]), rust_str(r["logic"]),
               rust_str(r["category"]), sub, name)
        )

    with open(OUT, "w", encoding="utf-8") as fh:
        fh.write(HEADER.format(
            count=len(kept),
            considered=considered,
            skipped_count=len(skipped),
            named=named,
            cat_lines=cat_lines,
            cond_note=cond_note,
            skip_table=skip_table,
        ))
        fh.write("\n".join(body))
        fh.write("\n];\n")

    print("considered %d, transcribed %d, skipped %d"
          % (considered, len(kept), len(skipped)))


HEADER = '''//! Trigger-id -> encounter identity, transcribed from GW2EI.
//!
//! GENERATED by `scripts/gen_encounter_catalog.py` -- do not edit by hand.
//! Tests live in `crates/axilog-core/tests/encounter_catalog.rs`, out of the
//! generator's reach.
//!
//! arcdps writes one number about the encounter into the evtc header: the
//! trigger species id (bytes 13-14, [`crate::evtc::RawHeader::boss_id`]).
//! This is the table that turns that number into "Gorseval, a Spirit Vale
//! raid wing boss" instead of the "Detailed WvW - World vs World" every PvE
//! log used to report.
//!
//! {count} of GW2EI's {considered} `LogData.DetectLogic` cases are
//! transcribed; {skipped_count} were skipped (below). By category:
//!
{cat_lines}
//!
//! `name` is `Some(..)` only for the {named} encounters GW2EI names with a
//! constant its `LogLogic.GetLogicName` override returns -- multi-boss
//! fights ("Twin Largos"), event encounters ("Siege the Stronghold") and
//! the strikes it names after the instance ("Harvest Temple"). For every
//! other id, GW2EI's DEFAULT rule applies -- the fight is named after the
//! boss's own agent name, read from the log's agent table -- so `name` is
//! `None` and [`super::resolve`] does the lookup at parse time.
//!
//! Conditional cases -- ids whose `DetectLogic` arm returns different
//! logics depending on what else is in the log, where this table records
//! only the fall-through one: {cond_note}.
//!
{skip_table}
//!
//! Entries are sorted by `trigger_id` so lookups can binary-search.

/// One encounter identity, keyed by arcdps's header trigger id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Encounter {{
    /// The evtc header's `boss_id` ([`crate::evtc::RawHeader::boss_id`]).
    pub trigger_id: u32,
    /// GW2EI's `TargetID` enum member, e.g. `"Gorseval"`. Diagnostic only --
    /// it is the join key back into GW2EI's source, not a display name.
    pub member: &'static str,
    /// The `LogLogic` subclass GW2EI hands this id to, e.g. `"TwinLargos"`.
    /// Also diagnostic: two ids sharing a logic are the same fight.
    pub logic: &'static str,
    /// `"raid_wing"`, `"fractal"`, `"raid_encounter"`, `"golem"`, ... --
    /// GW2EI's own `LogCategory`, lowercased. This is what
    /// [`crate::model::Encounter::kind`] carries.
    pub category: &'static str,
    /// GW2EI's `SubLogCategory`, e.g. `"SpiritVale"` -- the wing/fractal
    /// grouping, when it declares one.
    pub sub_category: Option<&'static str>,
    /// The fixed name GW2EI gives this fight, when it does not name it
    /// after the boss agent. `None` means "use the boss's agent name".
    pub name: Option<&'static str>,
}}

/// The encounter `trigger_id` identifies, or `None` when GW2EI has no
/// logic for it -- an unsupported or brand-new boss, which
/// [`super::resolve`] still names from the agent table.
pub fn lookup(trigger_id: u32) -> Option<&'static Encounter> {{
    ENCOUNTERS
        .binary_search_by_key(&trigger_id, |e| e.trigger_id)
        .ok()
        .map(|i| &ENCOUNTERS[i])
}}

/// Every encounter identity GW2EI declares, sorted by `trigger_id`.
pub const ENCOUNTERS: &[Encounter] = &[
'''


if __name__ == "__main__":
    main()
