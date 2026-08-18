#!/usr/bin/env python3
"""Regenerate `analysis::skill_icon_overrides` from the GW2EI C# sources.

ArenaNet's `/v2/skills` does not list every id an arcdps log can carry.
Sigil procs, pet skills, combo finishers, phantasms and the parser's own
synthetic ids all reach us through the log's skill table and all come back
`invalid` from the API -- 84 of the committed WvW fixture's 508 skill ids
do, checked against the live endpoint. So `gen_skill_icon_catalog.py` has
already extracted everything ITS source contains; the table is not stale,
the source is incomplete.

GW2EI closes the same gap with a hand-curated table,
`SkillItemOverrides.OverridenSkillIcons`, and that table is what this
script extracts -- the same discipline `gen_buff_icon_catalog.py` applies
to GW2EI's buff table.

Three catalogs, three sources, one answer:

  skill_icons           `/v2/skills`, ArenaNet's own data.
  skill_icon_overrides  this table -- ids the API does not know, PLUS
                        deliberate corrections to ids it does.
  buff_icons            GW2EI's buff table, for boons and conditions.

Precedence is set by the consumer, not here. `catalogs.rs` puts overrides
FIRST, which is what GW2EI itself does (`SkillItem.cs` consults
`OverridenSkillIcons` before falling back to `ApiSkill.Icon`): an entry in
this table is a deliberate correction, so deferring to the API would
reinstate exactly the value GW2EI overrode.

Shape of the extraction. The table is a plain C# dictionary initializer of
`{ IdSymbol, IconSymbol },` pairs, where the id is a `SkillIDs` constant or
an integer literal and the icon is an `*Images`/`ParserIcons` constant or a
string literal.

Nothing here guesses. An unresolved id symbol, an unresolved or ambiguous
icon symbol, a synthetic (non-positive) id, or an id declared twice with
different art all raise `Skip` and land in the skipped table WITH the
reason instead of producing a wrong entry. The accounting printed at the
end -- `considered == transcribed + repeats + skipped` -- is the
machine-diff behind this catalog's completeness claim.

Icons are stored whole rather than as `(signature, file_id)`: like the buff
table these span several hosts (`render.guildwars2.com`,
`wiki.guildwars2.com`, ...), so there is no shared prefix worth factoring
out, and lookups return `&'static str` without allocating.

Usage:

    python3 scripts/gen_skill_icon_override_catalog.py [/path/to/GW2EI/checkout]

then `git diff`: a clean tree means the committed catalog is exactly what
the current GW2EI source produces. Standard library only.
"""

import collections
import glob
import os
import re
import sys

from icon_mirror import mirror

ROOT = sys.argv[1] if len(sys.argv) > 1 else "/tmp/gw2ei"
PARSER = os.path.join(ROOT, "GW2EIEvtcParser")
TABLE = os.path.join(PARSER, "ParsedData/Skills/SkillItemOverrides.cs")
OUT = os.path.normpath(
    os.path.join(
        os.path.dirname(os.path.abspath(__file__)),
        "..", "crates", "axilog-core", "src", "analysis", "skill_icon_overrides.rs",
    )
)


class Skip(Exception):
    """An entry that cannot be transcribed, carrying the reason why."""


def strip_comments(text):
    """Blank out `//` and `/* */` comments, preserving string literals.

    GW2EI leaves retired entries in place as comments, so this is load
    bearing rather than tidiness -- without it the catalog gains art the
    parser itself no longer uses.
    """
    out = []
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        if c == '"':
            out.append(c)
            i += 1
            while i < n and text[i] != '"':
                if text[i] == "\\":
                    out.append(text[i:i + 2])
                    i += 2
                    continue
                out.append(text[i])
                i += 1
            if i < n:
                out.append(text[i])
                i += 1
        elif text.startswith("//", i):
            while i < n and text[i] != "\n":
                i += 1
        elif text.startswith("/*", i):
            end = text.find("*/", i + 2)
            i = n if end < 0 else end + 2
        else:
            out.append(c)
            i += 1
    return "".join(out)


def consts(path, pattern):
    found = {}
    for line in open(path, encoding="utf-8-sig"):
        m = re.search(pattern, line.strip())
        if m:
            found[m.group(1)] = m.group(2)
    return found


def load_tables():
    # Both visibilities: the arcdps-synthetic ids (`ArcDPSDodge` and
    # friends) are declared `internal`, and they are precisely the ones a
    # WvW log carries most often.
    ids = {
        k: int(v)
        for k, v in consts(
            os.path.join(PARSER, "ParserHelpers/IDs/SkillIDs.cs"),
            r"(?:public|internal) const long (\w+)\s*=\s*(-?\d+);",
        ).items()
    }

    qualified, bare = {}, collections.defaultdict(set)
    aliases = {}
    for path in sorted(glob.glob(os.path.join(PARSER, "ParserHelpers/Images/*.cs"))):
        cls = os.path.basename(path)[:-3]
        for name, url in consts(path, r'public const string (\w+)\s*=\s*"([^"]+)";').items():
            qualified[f"{cls}.{name}"] = url
            bare[name].add(url)
        # `public const string IllusionaryInspiration = HealingPrism;` --
        # an alias to another constant rather than a literal. Collected in
        # a second pass so it can point at a constant declared later in the
        # file, or in a different image class.
        for name, target in consts(path, r"public const string (\w+)\s*=\s*(\w+);").items():
            aliases[f"{cls}.{name}"] = (cls, target)

    for sym, (cls, target) in aliases.items():
        url = qualified.get(f"{cls}.{target}")
        if url is None:
            urls = bare.get(target, set())
            # Same unambiguity rule the icon resolver applies: an alias to a
            # name several classes spell differently resolves to nothing
            # rather than to a guess.
            url = next(iter(urls)) if len(urls) == 1 else None
        if url is not None:
            qualified[sym] = url
            bare[sym.rsplit(".", 1)[-1]].add(url)
    return ids, qualified, bare


def table_body(text, name):
    """The text between the dictionary initializer's outermost braces."""
    m = re.search(rf"{name}\s*=\s*new\(\s*\)?\s*", text)
    if not m:
        raise SystemExit(f"{name} not found in {TABLE} -- GW2EI moved or renamed it")
    start = text.index("{", m.end())
    depth, i = 0, start
    while i < len(text):
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                return text[start + 1:i]
        i += 1
    raise SystemExit(f"{name}'s initializer is unbalanced")


def entries(body):
    """Yield each `{ id, icon }` pair as a `(id_sym, icon_sym)` tuple."""
    for m in re.finditer(r"\{([^{}]*)\}", body):
        parts = [p.strip() for p in split_args(m.group(1))]
        if len(parts) != 2 or not parts[0] or not parts[1]:
            continue
        yield parts[0], parts[1]


def split_args(body):
    """Split on top-level commas, ignoring commas inside string literals."""
    args, cur, in_str = [], "", False
    i = 0
    while i < len(body):
        c = body[i]
        if in_str:
            if c == "\\":
                cur += body[i:i + 2]
                i += 2
                continue
            if c == '"':
                in_str = False
            cur += c
        elif c == '"':
            in_str = True
            cur += c
        elif c == ",":
            args.append(cur)
            cur = ""
        else:
            cur += c
        i += 1
    args.append(cur)
    return args


def resolve_id(sym, ids):
    if re.fullmatch(r"-?\d+", sym):
        value = int(sym)
    elif re.fullmatch(r"\w+", sym):
        if sym not in ids:
            raise Skip("id symbol not declared in SkillIDs.cs")
        value = ids[sym]
    else:
        raise Skip("id argument is not a symbol or literal")
    if value <= 0:
        # GW2EI's synthetic placeholders (WeaponSwap is -2). A log never
        # carries them as skill ids, and they collide with each other.
        raise Skip("synthetic (non-positive) skill id")
    return value


def resolve_icon(sym, qualified, bare):
    if sym.startswith('"') and sym.endswith('"'):
        return sym[1:-1]
    if sym in qualified:
        return qualified[sym]
    # A `Class.Const` reference may name a constant declared in a DIFFERENT
    # image class -- a C# `using` alias this script does not model. Fall
    # back to the bare name, and accept it only when every class that
    # declares it agrees on the URL.
    name = sym.rsplit(".", 1)[-1]
    if re.fullmatch(r"\w+", name) and name in bare:
        urls = bare[name]
        if len(urls) == 1:
            return next(iter(urls))
        raise Skip("icon symbol is ambiguous across image classes")
    raise Skip("icon symbol unresolved")


def main():
    ids, qualified, bare = load_tables()
    body = table_body(strip_comments(open(TABLE, encoding="utf-8-sig").read()),
                      "OverridenSkillIcons")

    considered = 0
    skipped = collections.Counter()
    by_id = collections.defaultdict(set)
    declarations = collections.Counter()

    for id_sym, icon_sym in entries(body):
        considered += 1
        try:
            # Resolve BOTH before touching `by_id`: it is a defaultdict, so
            # indexing first would leave an empty entry behind when the
            # icon step raises.
            skill_id = resolve_id(id_sym, ids)
            icon = resolve_icon(icon_sym, qualified, bare)
        except Skip as e:
            skipped[str(e)] += 1
            continue
        by_id[skill_id].add(icon)
        declarations[skill_id] += 1

    rows, repeats = [], 0
    for skill_id, urls in sorted(by_id.items()):
        if len(urls) > 1:
            # Unlike the buff table there is no build window to break a tie
            # on, so a genuinely contradictory id gets no art rather than a
            # coin flip.
            skipped["id declared with more than one icon"] += declarations[skill_id]
            continue
        rows.append((skill_id, mirror(next(iter(urls)))))
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
        for skill_id, url in rows:
            f.write(f'    ({skill_id}, "{url}"),\n')
        f.write("];\n")

    print(f"considered {considered} = transcribed {len(rows)} "
          f"+ repeat declarations {repeats} + skipped {total_skipped}")
    for reason, n in skipped.most_common():
        print(f"  skipped {n}: {reason}")
    assert considered == len(rows) + repeats + total_skipped, "accounting must balance"


HEADER = '''//! Skill icons GW2EI overrides, from its C# sources.
//!
//! GENERATED by `scripts/gen_skill_icon_override_catalog.py` -- do not
//! hand-edit. Re-run it and `git diff` to verify this table against
//! GW2EI's source.
//!
//! ArenaNet's `/v2/skills` does not list every id an arcdps log carries:
//! sigil procs, pet skills, combo finishers and phantasms all come back
//! `invalid`, so [`super::skill_icons`] cannot supply their art no matter
//! how recently it was regenerated. GW2EI keeps a hand-curated table for
//! exactly that gap, and this is it.
//!
//! Entries here are also deliberate CORRECTIONS to ids the API does know,
//! which is why `catalogs.rs` consults this table BEFORE the API -- the
//! order GW2EI itself uses in `SkillItem.cs`.
//!
//! The generator's accounting for this table:
//!
//! considered {considered} = transcribed {count} + repeat declarations
//! {repeats} + skipped {skipped}
//!
//! A repeat is the same id re-stated elsewhere in the table agreeing on the
//! art. The rest are skipped:
//!
{skip_table}
//!
//! Entries are sorted by id so lookups can binary-search.

/// The icon URL GW2EI overrides for skill `id`, or `None` when it has no
/// unambiguous art for it.
///
/// Borrows rather than allocating: GW2EI's links span several hosts, so
/// there is no shared prefix to factor out and the whole URL is stored.
pub fn icon(id: u32) -> Option<&'static str> {{
    SKILL_ICON_OVERRIDES
        .binary_search_by_key(&id, |&(sid, _)| sid)
        .ok()
        .map(|i| SKILL_ICON_OVERRIDES[i].1)
}}

/// `(skill id, icon url)`, sorted by id.
pub const SKILL_ICON_OVERRIDES: &[(u32, &str)] = &[
'''


if __name__ == "__main__":
    main()
