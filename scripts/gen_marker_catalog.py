#!/usr/bin/env python3
"""Regenerate `analysis::marker_icons` from the GW2EI C# sources.

arcdps reports a squad marker or commander tag as a raw 32-hex-character
content GUID -- `1993FADB6FB70E4383A223A54D311F7D` -- and nothing else. The
log does not say it is purple, does not say it is a tag rather than an
arrow, and carries no art. GW2EI keeps the tables that answer all three,
and this script extracts them, in the same style as
`gen_buff_icon_catalog.py` and `gen_skill_icon_override_catalog.py`.

Three GW2EI tables are read:

  MarkerGUIDs.*                     GUID -> symbol name, which is where the
                                    kind and the label come from
                                    (`PurpleCommanderTag`, `ArrowOverhead`).
  ParserIcons.SquadMarkerToIcon     overhead squad marker GUID -> wiki PNG.
  ParserIcons.CommanderTagToIcon    commander/catmander tag GUID -> wiki PNG.

The symbol name is the only source for kind and label -- GW2EI encodes both
positionally in the identifier rather than in a field -- so the split is a
suffix match, not a guess: `*Overhead` is a squad marker, `*CommanderTag`
and `*CatmanderTag` are tags, and the remaining prefix is the label.

Nothing here guesses. A GUID that is not 32 hex characters, a symbol whose
kind cannot be determined from its name, or a GUID mapped to two different
icons all raise `Skip` and land in the skipped table WITH the reason
instead of producing a wrong entry. The accounting printed at the end --
`considered == transcribed + skipped` -- is the machine-diff behind this
catalog's completeness claim.

GUIDs are stored LOWERCASE. GW2EI writes them uppercase in C#, axilog emits
them lowercase in `encounter.markers[].marker`, and a case mismatch would
make every lookup miss silently rather than loudly.

An entry may have no icon: GW2EI names 26 marker GUIDs but only supplies
art for 8 + 18 of them via the two icon maps. A GUID with a name and no
icon is still worth carrying -- the name alone lets a consumer say "purple
commander tag" instead of showing a hex string.

Usage:

    python3 scripts/gen_marker_catalog.py [/path/to/GW2EI/checkout]

then `git diff`: a clean tree means the committed catalog is exactly what
the current GW2EI source produces. Standard library only.
"""

import collections
import os
import re
import sys

ROOT = sys.argv[1] if len(sys.argv) > 1 else "/tmp/gw2ei"
PARSER = os.path.join(ROOT, "GW2EIEvtcParser")
GUIDS = os.path.join(PARSER, "ParserHelpers/GUIDs/MarkerGUIDs.cs")
ICONS = os.path.join(PARSER, "ParserHelpers/Images/ParserIcons.cs")
OUT = os.path.normpath(
    os.path.join(
        os.path.dirname(os.path.abspath(__file__)),
        "..", "crates", "axilog-core", "src", "analysis", "marker_icons.rs",
    )
)

HEX32 = re.compile(r"^[0-9a-f]{32}$")


class Skip(Exception):
    """An entry that cannot be transcribed, carrying the reason why."""


def strip_comments(text):
    """Blank out `//` and `/* */` comments, preserving string literals."""
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


def read(path):
    return strip_comments(open(path, encoding="utf-8-sig").read())


def load_guids():
    """`MarkerGUIDs` symbol -> lowercase GUID."""
    text = read(GUIDS)
    found = dict(re.findall(
        r'public static readonly GUID\s+(\w+)\s*=\s*new\("([0-9A-Fa-f]+)"\)', text))
    if not found:
        raise SystemExit(f"no GUID constants found in {GUIDS} -- GW2EI moved them")
    return {sym: guid.lower() for sym, guid in found.items()}


def load_icon_consts():
    """`ParserIcons` constant name -> URL."""
    text = read(ICONS)
    return dict(re.findall(
        r'(?:public|internal) const string (\w+)\s*=\s*"([^"]+)";', text))


def load_icon_map(name, icon_consts):
    """A `{ MarkerGUIDs.X, IconConst }` dictionary, as symbol -> URL."""
    text = read(ICONS)
    m = re.search(rf"{name}\s*=\s*new Dictionary<GUID, string>\(\)", text)
    if not m:
        raise SystemExit(f"{name} not found in {ICONS} -- GW2EI moved or renamed it")
    start = text.index("{", m.end())
    depth, i = 0, start
    while i < len(text):
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                break
        i += 1
    body = text[start + 1:i]
    out = {}
    for guid_sym, icon_sym in re.findall(r"\{\s*MarkerGUIDs\.(\w+)\s*,\s*(\w+)\s*\}", body):
        if icon_sym in icon_consts:
            out[guid_sym] = icon_consts[icon_sym]
    return out


def classify(sym):
    """`(kind, label)` from the symbol name.

    GW2EI encodes both positionally in the identifier -- there is no field
    to read -- so this is a suffix match rather than an inference.
    """
    for suffix, kind in (
        ("Overhead", "squad_marker"),
        ("CommanderTag", "commander_tag"),
        ("CatmanderTag", "catmander_tag"),
    ):
        if sym.endswith(suffix):
            label = sym[: -len(suffix)]
            if not label:
                raise Skip("symbol is a bare kind suffix with no label")
            # `XOverhead` -> `X`; everything else is already a word.
            return kind, label
    raise Skip("symbol name does not end in a known marker kind")


def main():
    guids = load_guids()
    icon_consts = load_icon_consts()
    icons = {}
    icons.update(load_icon_map("SquadMarkerToIcon", icon_consts))
    icons.update(load_icon_map("CommanderTagToIcon", icon_consts))

    considered = 0
    skipped = collections.Counter()
    by_guid = {}

    for sym, guid in sorted(guids.items()):
        considered += 1
        try:
            if not HEX32.fullmatch(guid):
                raise Skip("GUID is not 32 hex characters")
            kind, label = classify(sym)
            icon = icons.get(sym)
            if guid in by_guid and by_guid[guid][3] != icon:
                raise Skip("GUID mapped to more than one icon")
        except Skip as e:
            skipped[str(e)] += 1
            continue
        by_guid[guid] = (guid, kind, label, icon)

    rows = sorted(by_guid.values())
    total_skipped = sum(skipped.values())
    with_icon = sum(1 for r in rows if r[3])

    with open(OUT, "w") as f:
        f.write(HEADER.format(
            count=len(rows),
            considered=considered,
            skipped=total_skipped,
            with_icon=with_icon,
            without_icon=len(rows) - with_icon,
            # No leading indent: rustdoc reads a 4-space-indented block in a
            # doc comment as a Rust code sample and tries to compile it.
            skip_table="\n".join(
                f"//! - {n} {reason}" for reason, n in skipped.most_common()
            ) or "//! - (none)",
        ))
        for guid, kind, label, icon in rows:
            icon_lit = f'Some("{icon}")' if icon else "None"
            f.write(f'    Marker {{ guid: "{guid}", kind: "{kind}", '
                    f'label: "{label}", icon: {icon_lit} }},\n')
        f.write("];\n")

    print(f"considered {considered} = transcribed {len(rows)} + skipped {total_skipped}"
          f"  ({with_icon} with art, {len(rows) - with_icon} named only)")
    for reason, n in skipped.most_common():
        print(f"  skipped {n}: {reason}")
    assert considered == len(rows) + total_skipped, "accounting must balance"


HEADER = '''//! Squad marker and commander tag identities, from the GW2EI C# sources.
//!
//! GENERATED by `scripts/gen_marker_catalog.py` -- do not hand-edit. Re-run
//! it and `git diff` to verify this table against GW2EI's source.
//!
//! arcdps reports a marker as a raw 32-hex-character content GUID and
//! nothing else -- not its colour, not whether it is a tag or an arrow, and
//! no art. This table answers all three, from `MarkerGUIDs` (identity) plus
//! `ParserIcons.SquadMarkerToIcon` and `ParserIcons.CommanderTagToIcon`
//! (art).
//!
//! GUIDs are stored LOWERCASE, matching what axilog emits in
//! `encounter.markers[].marker`. GW2EI writes them uppercase, and a case
//! mismatch would make every lookup miss silently rather than loudly.
//!
//! The generator's accounting for this table:
//!
//! considered {considered} = transcribed {count} + skipped {skipped}
//!
//! Of the {count} transcribed, {with_icon} carry art and {without_icon} are
//! named only -- GW2EI names more marker GUIDs than it supplies icons for,
//! and a name alone still beats showing a hex string.
//!
{skip_table}
//!
//! Entries are sorted by GUID so lookups can binary-search.

/// One marker identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Marker {{
    /// Lowercase 32-hex content GUID, as `encounter.markers[].marker` spells it.
    pub guid: &'static str,
    /// `"squad_marker"`, `"commander_tag"` or `"catmander_tag"`.
    pub kind: &'static str,
    /// `"Arrow"`, `"Purple"`, `"X"` -- the symbol name minus its kind suffix.
    pub label: &'static str,
    /// Wiki art, when GW2EI supplies any for this GUID.
    pub icon: Option<&'static str>,
}}

/// The marker `guid` names, or `None` when GW2EI does not know it.
///
/// Matching is case-sensitive against the lowercase spelling; callers
/// holding an uppercase GUID must lowercase it first.
pub fn lookup(guid: &str) -> Option<&'static Marker> {{
    MARKERS
        .binary_search_by_key(&guid, |m| m.guid)
        .ok()
        .map(|i| &MARKERS[i])
}}

/// Every marker identity GW2EI declares, sorted by GUID.
pub const MARKERS: &[Marker] = &[
'''


if __name__ == "__main__":
    main()
