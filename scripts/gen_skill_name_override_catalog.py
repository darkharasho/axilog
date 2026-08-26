#!/usr/bin/env python3
"""Regenerate `analysis::skill_name_overrides` from the GW2EI C# sources.

`SkillItemOverrides.cs` holds two sibling dictionaries.
`gen_skill_icon_override_catalog.py` extracts `OverridenSkillIcons`; this
script extracts `OverridenSkillNames`, which has the identical shape --
`{ IdSymbol, "Display Name" }` -- differing only in that the value is a
string literal rather than a symbol, so there is no symbol table to
resolve and no ambiguity to skip on.

Why the names matter separately from the icons. ArenaNet's `/v2/skills`
does not list every id an arcdps log carries, and for those ids arcdps
often writes a bare numeric placeholder into the log's own skill table --
skill 1066 arrives named literally `"1066"`. `skill_map::resolve_name`
rejects a numeric name (correctly: it is not a name), the API catalog has
never heard of the id, and the result is the `"Skill 1066"` placeholder a
WvW player reported seeing in a rendered healing table. GW2EI names that
id `Resurrect`, from this table.

Scope. Only the POSITIVE ids. The negative pseudo ids are already hand-
transcribed as `skill_map::PSEUDO_SKILL_NAMES`, with a documented gap
(EI's twelve Weaver dual-attunement ids are named from its BUFF table, not
from this one); duplicating them here would create a second source of
truth for the same 25 entries.

Usage:

    python3 scripts/gen_skill_name_override_catalog.py [/path/to/GW2EI/checkout]

then `git diff`: a clean tree means the committed catalog is exactly what
the current GW2EI source produces. Standard library only.
"""

import collections
import os
import re
import sys

from gen_skill_icon_override_catalog import (
    PARSER, TABLE, Skip, consts, entries, resolve_id, strip_comments, table_body,
)

OUT = os.path.normpath(
    os.path.join(
        os.path.dirname(os.path.abspath(__file__)),
        "..", "crates", "axilog-core", "src", "analysis", "skill_name_overrides.rs",
    )
)


def load_ids():
    # Both visibilities, for the same reason the icon generator gives: the
    # arcdps-synthetic ids are declared `internal`.
    return {
        k: int(v)
        for k, v in consts(
            os.path.join(PARSER, "ParserHelpers/IDs/SkillIDs.cs"),
            r"(?:public|internal) const long (\w+)\s*=\s*(-?\d+);",
        ).items()
    }


def resolve_name(sym):
    if sym.startswith('"') and sym.endswith('"') and len(sym) >= 2:
        text = sym[1:-1]
        if not text.strip():
            raise Skip("name literal is empty")
        return text
    raise Skip("name argument is not a string literal")


def main():
    ids = load_ids()
    body = table_body(
        strip_comments(open(TABLE, encoding="utf-8-sig").read()),
        "OverridenSkillNames",
    )

    considered = 0
    skipped = collections.Counter()
    by_id = collections.defaultdict(set)
    declarations = collections.Counter()

    for id_sym, name_sym in entries(body):
        considered += 1
        try:
            # Resolve BOTH before touching `by_id` -- it is a defaultdict, so
            # indexing first would leave an empty entry behind on a raise.
            skill_id = resolve_id(id_sym, ids)
            display = resolve_name(name_sym)
        except Skip as e:
            skipped[str(e)] += 1
            continue
        by_id[skill_id].add(display)
        declarations[skill_id] += 1

    rows, repeats = [], 0
    for skill_id, names in sorted(by_id.items()):
        if len(names) > 1:
            # Two different names for one id is a contradiction we cannot
            # break, exactly as with two icons: no entry rather than a coin
            # flip, so the id keeps whatever the earlier rungs resolved.
            skipped["id declared with more than one name"] += declarations[skill_id]
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
            # No leading indent: rustdoc reads a 4-space-indented block in a
            # doc comment as a Rust code sample and tries to compile it.
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


HEADER = '''//! Skill NAME overrides from GW2EI's C# sources.
//!
//! GENERATED by `scripts/gen_skill_name_override_catalog.py` -- do not
//! hand-edit. Re-run it and `git diff` to verify this table against
//! GW2EI's source.
//!
//! The sibling of [`super::skill_icon_overrides`], from the sibling
//! dictionary in the same file, closing the same gap for names that one
//! closes for art: ArenaNet's `/v2/skills` does not list every id an
//! arcdps log carries, and for the ids it omits arcdps itself often writes
//! a bare numeric placeholder into the log's own skill table (id 1066
//! arrives named literally `"1066"`). With no log name, no pseudo name and
//! no API entry, such an id used to render as `"Skill 1066"`.
//!
//! POSITIVE ids only. The negative pseudo ids are hand-transcribed as
//! [`super::skill_map::PSEUDO_SKILL_NAMES`], which documents its own gap;
//! two sources of truth for the same 25 entries would be worse than one.
//!
//! The generator's accounting for this table:
//!
//! considered {considered} = transcribed {count} + repeat declarations
//! {repeats} + skipped {skipped}
//!
//! A repeat is the same id re-stated elsewhere in the table agreeing on the
//! name -- `Resurrect` and `Resurrect2` are two ids, not a repeat. The rest
//! are skipped:
//!
{skip_table}
//!
//! Entries are sorted by id so lookups can binary-search.

/// GW2EI's overriding display name for skill `id`, or `None` when it has
/// none.
pub fn name(id: u32) -> Option<&'static str> {{
    SKILL_NAME_OVERRIDES
        .binary_search_by_key(&id, |&(sid, _)| sid)
        .ok()
        .map(|i| SKILL_NAME_OVERRIDES[i].1)
}}

/// `(skill id, display name)`, sorted by id.
pub const SKILL_NAME_OVERRIDES: &[(u32, &str)] = &[
'''


if __name__ == "__main__":
    main()
