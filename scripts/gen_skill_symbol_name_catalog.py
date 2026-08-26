#!/usr/bin/env python3
"""Regenerate `analysis::skill_symbol_names` from GW2EI's `SkillIDs.cs`.

The last-resort NAME rung. `gen_skill_name_override_catalog.py` harvests
GW2EI's `OverridenSkillNames` -- 293 ids it deliberately re-labels. This
script harvests something much larger and much dumber: every `const long`
in `SkillIDs.cs`, the file where GW2EI gives an id a *symbol* so its own
code can refer to it.

A symbol is not a display name. `GladiatorsDefenseAnimation` is how a C#
programmer writes it, not how a player reads it. But measured against a
real ~4000-log WvW corpus, 22 distinct ids still rendered as the literal
`"Skill <id>"` after the override rung landed, and every one of them has a
symbol here. The choice at this rung is not "good name vs. better name",
it is "de-camel-cased symbol vs. `Skill 23288`" -- so the bar is low and
the win is broad: one table closes the whole tail instead of a hand-added
row per id a player happens to report.

Precedence. This rung sits BELOW every rung that has real naming
authority -- the log's own skill table, the pseudo names, the `/v2/skills`
catalog, and GW2EI's overrides -- and immediately above the `Skill {id}`
placeholder. It can therefore never rename anything another rung named; it
can only replace a placeholder.

Scope. POSITIVE ids only, matching the override generator: negatives are
`skill_map::PSEUDO_SKILL_NAMES`, and a second source of truth for those
would be worse than one.

Usage:

    python3 scripts/gen_skill_symbol_name_catalog.py [/path/to/GW2EI/checkout]

then `git diff`. Standard library only.
"""

import collections
import os
import re
import sys

from gen_skill_icon_override_catalog import PARSER, Skip, consts

OUT = os.path.normpath(
    os.path.join(
        os.path.dirname(os.path.abspath(__file__)),
        "..", "crates", "axilog-core", "src", "analysis", "skill_symbol_names.rs",
    )
)

# Split a C# identifier into display words. Three boundaries, in order:
# lower|digit -> upper  (`GenericKill`  -> `Generic|Kill`)
# upper -> upper+lower  (`ArcDPSGeneric` -> `ArcDPS|Generic`, keeping the
#                        acronym whole rather than shattering it to `A|r|c`)
# letter -> digit       (`Resurrect2`   -> `Resurrect|2`)
_BOUNDARIES = [
    (re.compile(r"(?<=[a-z0-9])(?=[A-Z])"), " "),
    (re.compile(r"(?<=[A-Z])(?=[A-Z][a-z])"), " "),
    (re.compile(r"(?<=[A-Za-z])(?=\d)"), " "),
]


def display_name(symbol):
    text = symbol.replace("_", " ")
    for pattern, sep in _BOUNDARIES:
        text = pattern.sub(sep, text)
    text = " ".join(text.split())
    if not text:
        raise Skip("symbol de-camel-cases to nothing")
    if text.isdigit():
        raise Skip("symbol is purely numeric")
    return text


def main():
    # Both visibilities, for the reason the icon generator gives: the
    # arcdps-synthetic ids are declared `internal`.
    symbols = consts(
        os.path.join(PARSER, "ParserHelpers/IDs/SkillIDs.cs"),
        r"(?:public|internal) const long (\w+)\s*=\s*(-?\d+);",
    )

    considered = 0
    skipped = collections.Counter()
    by_id = collections.defaultdict(set)
    declarations = collections.Counter()

    for symbol, raw in symbols.items():
        considered += 1
        skill_id = int(raw)
        if skill_id < 0:
            skipped["negative pseudo id (owned by PSEUDO_SKILL_NAMES)"] += 1
            continue
        if skill_id > 0xFFFF_FFFF:
            skipped["id does not fit u32"] += 1
            continue
        try:
            display = display_name(symbol)
        except Skip as e:
            skipped[str(e)] += 1
            continue
        by_id[skill_id].add(display)
        declarations[skill_id] += 1

    rows, repeats = [], 0
    for skill_id, names in sorted(by_id.items()):
        if len(names) > 1:
            # Same contradiction rule the sibling generators use: two
            # different names for one id is not a tie we get to break, so
            # emit nothing and let the id keep its placeholder. Guessing
            # here would be worse than the placeholder, which at least
            # tells the reader we do not know.
            skipped["id declared under more than one symbol"] += declarations[skill_id]
            continue
        rows.append((skill_id, next(iter(names))))
        repeats += declarations[skill_id] - 1

    total_skipped = sum(skipped.values())
    with open(OUT, "w") as f:
        f.write(HEADER.format(
            count=len(rows),
            considered=considered,
            repeats=repeats,
            skipped=total_skipped,
            skip_table="\n".join(
                f"//! - {n} {reason}" for reason, n in skipped.most_common()
            ) or "//! - (none)",
        ))
        for skill_id, display in rows:
            escaped = display.replace("\\", "\\\\").replace('"', '\\"')
            f.write(f'    ({skill_id}, "{escaped}"),\n')
        f.write("];\n")

    print(f"considered {considered} = transcribed {len(rows)} "
          f"+ repeat declarations {repeats} + skipped {total_skipped}")
    for reason, n in skipped.most_common():
        print(f"  skipped {n}: {reason}")
    assert considered == len(rows) + repeats + total_skipped, "accounting must balance"


HEADER = '''//! Skill names de-camel-cased from GW2EI's `SkillIDs.cs` symbols.
//!
//! GENERATED by `scripts/gen_skill_symbol_name_catalog.py` -- do not
//! hand-edit. Re-run it and `git diff` to verify this table against
//! GW2EI's source.
//!
//! The LAST-RESORT name rung, consulted only after the log's own skill
//! table, the pseudo names, the `/v2/skills` catalog and
//! [`super::skill_name_overrides`] have all declined -- and immediately
//! before the `"Skill {{id}}"` placeholder. It can never rename anything
//! another rung named; it can only replace a placeholder.
//!
//! A symbol is not a display name: `GladiatorsDefenseAnimation` is how
//! GW2EI's C# refers to the id, not how a player reads it. That is the
//! right trade at this rung and nowhere above it. Measured on a real
//! ~4000-log WvW corpus, 22 distinct ids still rendered as `"Skill <id>"`
//! after the override rung landed, and every one had a symbol here; the
//! comparison is against `"Skill 23288"`, not against a real name.
//!
//! POSITIVE ids only, matching [`super::skill_name_overrides`]: the
//! negative pseudo ids are [`super::skill_map::PSEUDO_SKILL_NAMES`].
//!
//! The generator's accounting for this table:
//!
//! considered {considered} = transcribed {count} + repeat declarations
//! {repeats} + skipped {skipped}
//!
//! A repeat is the same id declared under a second symbol that
//! de-camel-cases identically. The rest are skipped:
//!
{skip_table}
//!
//! Entries are sorted by id so lookups can binary-search.

/// A display name de-camel-cased from GW2EI's symbol for skill `id`, or
/// `None` when it has none.
pub fn name(id: u32) -> Option<&'static str> {{
    SKILL_SYMBOL_NAMES
        .binary_search_by_key(&id, |&(sid, _)| sid)
        .ok()
        .map(|i| SKILL_SYMBOL_NAMES[i].1)
}}

/// `(skill id, display name)`, sorted by id.
pub const SKILL_SYMBOL_NAMES: &[(u32, &str)] = &[
'''


if __name__ == "__main__":
    main()
