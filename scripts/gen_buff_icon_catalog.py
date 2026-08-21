#!/usr/bin/env python3
"""Regenerate `analysis::buff_icons` from the GW2EI C# sources.

Boons and conditions are not skills. arcdps logs them THROUGH the skill
table -- id 717 arrives looking exactly like a skill -- but ArenaNet's
`/v2/skills` endpoint has no record for them, so the API-driven catalog in
`gen_skill_icon_catalog.py` cannot give them an icon. GW2EI maintains its
own buff table for precisely this reason, and that table is what this
script extracts, in the same style as `gen_damage_mod_catalog.py`.

The two catalogs are complements, not rivals: `skill_icons` covers real
API skills, `buff_icons` covers the buff ids the API does not know. Where
both have an entry the API wins, because it is ArenaNet's own data.

Shape of the extraction. Every buff is a `new Buff(...)` statement whose
second argument is the id (a `SkillIDs` constant or an integer literal)
and whose LAST argument is the icon (an `*Images`/`ParserIcons` constant
or a string literal). Both constructor arities in `Buff.cs` -- the 5-arg
and 7-arg forms -- put the icon last, so "last argument" is the rule
rather than a fixed position.

Nothing here guesses. An unresolved id symbol, an unresolved icon symbol,
an ambiguous icon symbol, a synthetic (non-positive) id, or an id defined
with two different icons all raise `Skip` and land in the skipped table
WITH the reason instead of producing a wrong entry. The accounting printed
at the end -- `considered == transcribed + skipped` -- is the machine-diff
behind this catalog's completeness claim.

Three details the naive scan gets wrong, each handled below:

  * Commented-out definitions. GW2EI leaves dead `new Buff(...)` lines in
    place; a raw regex sweep transcribes them as live. Comments are
    stripped before scanning.
  * Cross-class icon constants. `EngineerHelper.cs` says
    `BuffImages.GrenadeKit`, but that constant is declared in
    `SkillImages.cs` -- a C# alias this script does not try to model.
    Instead an unresolved `Class.Const` falls back to the bare `Const`
    across every image class, and is accepted only when it is unambiguous.
  * Icons span several hosts, so unlike the skill catalog there is no
    shared prefix worth factoring out. URLs are stored whole, which is
    also why lookups return `&'static str` and never allocate. Two of
    those hosts -- `i.imgur.com` and `assets.gw2dat.com` -- are not ones
    we control, so `icon_mirror.mirror` rewrites them to our own mirror
    on the way out; see that module for why, and for why an unrecognised
    URL on either host is a hard error rather than a pass-through.

Usage:

    python3 scripts/gen_buff_icon_catalog.py [/path/to/GW2EI/checkout]

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
OUT = os.path.normpath(
    os.path.join(
        os.path.dirname(os.path.abspath(__file__)),
        "..", "crates", "axilog-core", "src", "analysis", "buff_icons.rs",
    )
)


class Skip(Exception):
    """A buff that cannot be transcribed, carrying the reason why."""


def strip_comments(text):
    """Blank out `//` and `/* */` comments, preserving string literals.

    GW2EI keeps retired definitions around as comments, so this is load
    bearing rather than tidiness -- without it the catalog gains entries
    the parser itself no longer builds.
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


def load_builds():
    """`GW2Builds` constants, for ordering a buff's build windows.

    `StartOfLife`/`EndOfLife` are `ulong.MinValue`/`MaxValue` rather than
    literals, so they are mapped explicitly.

    Both `ulong` and `long` are accepted: GW2EI's newest entries switched
    type, and those are precisely the builds a tiebreak must be able to
    see, since the latest window is the one that wins.
    """
    path = os.path.join(PARSER, "ParserHelpers/GW2Builds.cs")
    builds = {"StartOfLife": 0, "EndOfLife": 2**64 - 1}
    builds.update({
        k: int(v)
        for k, v in consts(path, r"public const u?long (\w+)\s*=\s*(\d+);").items()
    })
    return builds


def load_tables():
    ids = {
        k: int(v)
        for k, v in consts(
            os.path.join(PARSER, "ParserHelpers/IDs/SkillIDs.cs"),
            r"public const long (\w+)\s*=\s*(-?\d+);",
        ).items()
    }

    qualified, bare = {}, collections.defaultdict(set)
    for path in sorted(glob.glob(os.path.join(PARSER, "ParserHelpers/Images/*.cs"))):
        cls = os.path.basename(path)[:-3]
        for name, url in consts(path, r'public const string (\w+)\s*=\s*"([^"]+)";').items():
            qualified[f"{cls}.{name}"] = url
            bare[name].add(url)
    return ids, qualified, bare


def split_args(body):
    """Split a C# argument list on top-level commas.

    Depth tracking covers `(`, `[` and `{` because the 7-argument form
    passes `new HashSet<Source> { ... }` inline.
    """
    args, depth, cur, in_str = [], 0, "", False
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
        elif c in "([{":
            depth += 1
            cur += c
        elif c in ")]}":
            depth -= 1
            cur += c
        elif c == "," and depth == 0:
            args.append(cur.strip())
            cur = ""
        else:
            cur += c
        i += 1
    if cur.strip():
        args.append(cur.strip())
    return args


def statements(text):
    """Yield `(args, chain)` for every `new Buff(...)` in `text`.

    `chain` is the trailing builder calls (`.WithBuilds(...)` and friends),
    which carry the build window a definition applies to -- needed because
    ArenaNet reuses ids across balance patches.
    """
    for m in re.finditer(r"new Buff\(", text):
        start = m.end()
        depth, j = 1, start
        while j < len(text) and depth:
            if text[j] == "(":
                depth += 1
            elif text[j] == ")":
                depth -= 1
            j += 1
        args_end = j - 1

        # Walk the `.Method(...)` chain that follows, stopping at whatever
        # ends the statement (the `,` of the enclosing collection, or `;`).
        depth, k = 0, j
        while k < len(text):
            c = text[k]
            if c == "(":
                depth += 1
            elif c == ")":
                if depth == 0:
                    break
                depth -= 1
            elif depth == 0 and (c == "," or c == ";"):
                break
            k += 1
        yield text[start:args_end], text[j:k]


def min_build(chain, builds):
    """The first build this definition applies to.

    A definition with no `.WithBuilds` has always applied, hence 0.
    """
    m = re.search(r"\.WithBuilds\(([^)]*)\)", chain)
    if not m:
        return 0
    first = m.group(1).split(",")[0].strip().rsplit(".", 1)[-1]
    if re.fullmatch(r"\d+", first):
        return int(first)
    if first not in builds:
        raise Skip("build constant not declared in GW2Builds.cs")
    return builds[first]


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
        # Negative ids are GW2EI's synthetic placeholders, not buffs the
        # log can ever carry, and they collide with each other.
        raise Skip("synthetic (non-positive) buff id")
    return value


def resolve_icon(sym, qualified, bare):
    if sym.startswith('"') and sym.endswith('"'):
        return sym[1:-1]
    if sym in qualified:
        return qualified[sym]
    # See the module docstring: `BuffImages.X` may name a constant that
    # actually lives in another image class. Accept the bare name only
    # when every class that declares it agrees on the URL.
    name = sym.rsplit(".", 1)[-1]
    if re.fullmatch(r"\w+", name) and name in bare:
        urls = bare[name]
        if len(urls) == 1:
            return next(iter(urls))
        raise Skip("icon symbol is ambiguous across image classes")
    raise Skip("icon symbol unresolved")


STACK_TYPES = {
    # `ArcDPSEnums.BuffStackType` -> `analysis::buffs::BuffStackType`.
    # `Unknown` is deliberately absent: it is GW2EI's "we do not know",
    # and transcribing it as any concrete variant would be a guess.
    "Queue": "Queue",
    "Regeneration": "Regeneration",
    "Force": "Force",
    "Stacking": "Stacking",
    "StackingUniquePerSrc": "StackingUniquePerSrc",
    "StackingConditionalLoss": "StackingConditionalLoss",
}


CLASSIFICATIONS = {
    "Condition", "Boon", "Offensive", "Defensive", "Support", "Debuff",
    "Gear", "Other", "Enhancement", "Nourishment", "OtherConsumable",
    "Hidden", "Unknown",
}


def resolve_meta(args):
    """`(name, stack_type, capacity, classification)` for one `new Buff(...)`.

    GW2EI declares buffs at exactly two arities (measured across the whole
    parser: 1,868 five-argument and 471 seven-argument, nothing else):

      5: (name, source(s), classification, link)        -- with `id` 2nd
      7: (name, id, source(s), StackType, capacity, classification, link)

    The five-argument form is not missing information; it is the
    `Buff(name, id, source, nature, link)` overload, whose body reads
    `this(name, id, source, BuffStackType.Force, 1, nature, link)`
    (`EIData/Buffs/Buff.cs:119-126`). Force/1 is therefore transcribed,
    not assumed.
    """
    name = args[0].strip()
    if not (name.startswith('"') and name.endswith('"')):
        raise Skip("name argument is not a string literal")
    name = name[1:-1]
    if not name:
        raise Skip("name is empty")
    if len(args) == 5:
        return name, "Force", 1, classification(args[3])
    sym = args[3].rsplit(".", 1)[-1].strip()
    if sym not in STACK_TYPES:
        raise Skip("stack type is Unknown or unrecognised")
    cap = args[4].strip()
    if not re.fullmatch(r"\d+", cap):
        raise Skip("capacity argument is not an integer literal")
    return name, STACK_TYPES[sym], int(cap), classification(args[5])


def classification(arg):
    """`BuffClassification` (`EIData/Buffs/Buff.cs:15-30`) for one argument.

    Load bearing for `Hidden` in particular: GW2EI's own
    `JsonPlayerBuilder.GetPlayerJsonBuffsUptime`
    (`JsonModels/JsonActors/JsonPlayerBuilder.cs:266-269`) SKIPS a hidden
    buff when building `buffUptimes`, so tracking one would report a row
    Elite Insights does not have.
    """
    sym = arg.rsplit(".", 1)[-1].strip()
    if sym not in CLASSIFICATIONS:
        raise Skip("classification is not a BuffClassification constant")
    return sym


def rust_str(value):
    """A Rust string literal. Buff names carry quotes and backslashes."""
    return '"' + value.replace("\\", "\\\\").replace('"', '\\"') + '"'


def main():
    ids, qualified, bare = load_tables()
    builds = load_builds()

    considered = 0
    skipped = collections.Counter()
    by_id = collections.defaultdict(list)
    declarations = collections.Counter()
    # The metadata table is accounted SEPARATELY, for the same reason the
    # skill catalog splits its two tables: the absences do not coincide. A
    # definition whose icon symbol is unresolvable still names itself and
    # still states its stack type, and dropping it from the metadata table
    # because of an unrelated art problem would lose a fact we have.
    meta_by_id = collections.defaultdict(list)
    meta_declarations = collections.Counter()
    meta_skipped = collections.Counter()

    for path in sorted(glob.glob(os.path.join(PARSER, "**/*.cs"), recursive=True)):
        raw = open(path, encoding="utf-8-sig").read()
        if "new Buff(" not in raw:
            continue
        for body, chain in statements(strip_comments(raw)):
            args = split_args(body)
            if len(args) < 4:
                # `Buff.cs`'s own constructor chaining, not a definition.
                continue
            considered += 1
            try:
                # Resolve everything BEFORE touching `by_id`: it is a
                # defaultdict, so indexing it first would leave an empty
                # entry behind when a later step raises `Skip`.
                buff_id = resolve_id(args[1], ids)
            except Skip as e:
                skipped[str(e)] += 1
                meta_skipped[str(e)] += 1
                continue
            try:
                entry = (min_build(chain, builds), resolve_icon(args[-1], qualified, bare))
                by_id[buff_id].append(entry)
                declarations[buff_id] += 1
            except Skip as e:
                skipped[str(e)] += 1
            try:
                # Resolve BEFORE indexing `meta_by_id`: it is a defaultdict,
                # so `meta_by_id[id].append(f())` creates the entry before
                # `f()` runs and leaves an empty list behind when `f`
                # raises. Same trap the icon path's comment above names.
                entry = (min_build(chain, builds), resolve_meta(args))
                meta_by_id[buff_id].append(entry)
                meta_declarations[buff_id] += 1
            except Skip as e:
                meta_skipped[str(e)] += 1

    # An id is declared once per build window, so most of the repeats below
    # are the same buff re-stated across a balance patch. They agree on the
    # icon and collapse to one row; they are counted rather than silently
    # dropped so the statement-level accounting still balances.
    rows, repeats, superseded = [], 0, 0
    for buff_id, defs in sorted(by_id.items()):
        urls = {icon for _, icon in defs}
        if len(urls) > 1:
            # ArenaNet reuses ids across balance patches -- 873 was
            # Retaliation until May 2021 and has been Resolution since,
            # with different art each side. The definition whose build
            # window opens latest is the one a log being parsed today
            # matches, so it wins; that is a rule, not a coin flip.
            newest = max(build for build, _ in defs)
            winners = {icon for build, icon in defs if build == newest}
            if len(winners) > 1:
                # Same window, different art: nothing left to break the
                # tie on, so the id gets no icon at all.
                skipped["id declared with more than one icon in the same build window"] += \
                    declarations[buff_id]
                continue
            icon = next(iter(winners))
            superseded += declarations[buff_id] - 1
        else:
            icon = next(iter(urls))
            repeats += declarations[buff_id] - 1
        rows.append((buff_id, mirror(icon)))

    # Same build-window winner rule as the icons above, applied to the
    # metadata triple: ArenaNet reuses ids across balance patches, and the
    # definition whose window opens latest is the one a log parsed today
    # matches.
    meta_rows, meta_repeats, meta_superseded = [], 0, 0
    for buff_id, defs in sorted(meta_by_id.items()):
        metas = {m for _, m in defs}
        if len(metas) > 1:
            newest = max(build for build, _ in defs)
            winners = {m for build, m in defs if build == newest}
            if len(winners) > 1:
                meta_skipped["id declared with conflicting metadata in the same build window"] += \
                    meta_declarations[buff_id]
                continue
            meta = next(iter(winners))
            meta_superseded += meta_declarations[buff_id] - 1
        else:
            meta = next(iter(metas))
            meta_repeats += meta_declarations[buff_id] - 1
        meta_rows.append((buff_id, *meta))

    total_skipped = sum(skipped.values())
    total_meta_skipped = sum(meta_skipped.values())
    with open(OUT, "w") as f:
        f.write(HEADER.format(
            count=len(rows),
            considered=considered,
            skipped=total_skipped,
            repeats=repeats,
            superseded=superseded,
            # No leading indent: rustdoc reads a 4-space-indented block in
            # a doc comment as a Rust code sample and tries to compile it.
            skip_table="\n".join(
                f"//! - {n} {reason}" for reason, n in skipped.most_common()
            ),
            meta_count=len(meta_rows),
            meta_repeats=meta_repeats,
            meta_superseded=meta_superseded,
            meta_skipped=total_meta_skipped,
            meta_skip_table="\n".join(
                f"//! - {n} {reason}" for reason, n in meta_skipped.most_common()
            ),
        ))
        for buff_id, url in rows:
            f.write(f'    ({buff_id}, "{url}"),\n')
        f.write("];\n")
        f.write(META_HEADER.format(meta_count=len(meta_rows)))
        for buff_id, name, stack_type, capacity, klass in meta_rows:
            f.write(
                f"    ({buff_id}, {rust_str(name)}, BuffStackType::{stack_type}, "
                f"{capacity}, {rust_str(klass)}),\n"
            )
        f.write("];\n")

    for label, transcribed, reps, sup, counter in (
        ("icons", len(rows), repeats, superseded, skipped),
        ("metadata", len(meta_rows), meta_repeats, meta_superseded, meta_skipped),
    ):
        total = sum(counter.values())
        print(f"{label}: considered {considered} = transcribed {transcribed} "
              f"+ repeat declarations {reps} + superseded by a later build {sup} "
              f"+ skipped {total}")
        for reason, n in counter.most_common():
            print(f"  skipped {n}: {reason}")
        assert considered == transcribed + reps + sup + total, \
            f"{label} accounting must balance"


HEADER = '''//! Buff icons and metadata, from the GW2EI buff table.
//!
//! GENERATED by `scripts/gen_buff_icon_catalog.py` -- do not hand-edit.
//! Re-run it and `git diff` to verify these tables against GW2EI's source.
//!
//! Boons and conditions reach us through the log's skill table -- id 717
//! is Protection, and arcdps reports it exactly as it reports a skill --
//! but ArenaNet's `/v2/skills` endpoint has no record of them, so
//! [`super::skill_icons`] cannot supply their art. [`BUFF_ICONS`] covers
//! that gap; the two are complements, and where both have an entry the
//! API is the better source.
//!
//! [`BUFF_META`] is the second half, and the reason this module is no
//! longer only about art: `analysis::buffs::name`/`stacking` need a NAME
//! and a stack type for every buff a log can carry, not just for the 12
//! boons, 14 conditions and 2 control effects the three narrow static
//! tables cover. Without it a sigil, relic, food buff or trait buff
//! reaches the catalog nameless, and a consumer that selects rows by name
//! -- axibridge's Sigil/Relic Uptime section matches `/sigil|relic/` --
//! can never see it.
//!
//! The two tables are accounted SEPARATELY because their absences do not
//! coincide: a definition whose icon symbol is ambiguous still names
//! itself and still states its stack type.
//!
//! The generator's accounting for [`BUFF_ICONS`]:
//!
//! considered {considered} = transcribed {count} + repeat declarations
//! {repeats} + superseded by a later build {superseded} + skipped {skipped}
//!
//! A repeat is the same id re-declared for another build window agreeing on
//! the icon. A superseded declaration lost to a later one: ArenaNet reuses
//! ids across balance patches -- 873 was Retaliation until May 2021 and has
//! been Resolution since -- and the newest build window is the one a log
//! parsed today matches. The rest are skipped:
//!
{skip_table}
//!
//! and for [`BUFF_META`]:
//!
//! considered {considered} = transcribed {meta_count} + repeat declarations
//! {meta_repeats} + superseded by a later build {meta_superseded} + skipped
//! {meta_skipped}
//!
{meta_skip_table}
//!
//! Entries in both are sorted by id so lookups can binary-search.

use super::buffs::BuffStackType;

/// The icon URL for buff `id`, or `None` when GW2EI has no unambiguous
/// art for it.
///
/// Unlike [`super::skill_icons::icon`] this borrows rather than
/// allocating: GW2EI's links span four different hosts, so there is no
/// shared prefix to factor out and the whole URL is stored as-is.
pub fn icon(id: u32) -> Option<&'static str> {{
    BUFF_ICONS
        .binary_search_by_key(&id, |&(bid, _)| bid)
        .ok()
        .map(|i| BUFF_ICONS[i].1)
}}

/// GW2EI's definition for buff `id`, or `None` when its table has no
/// unambiguous one.
///
/// This is a FALLBACK for [`super::buffs::name`] and
/// [`super::buffs::stacking`], consulted only after the three narrow
/// static tables (boons, conditions, control effects) miss. Those three
/// are hand-verified against their own calibrations and stay
/// authoritative; this one covers everything else a log can carry.
pub fn meta(id: u32) -> Option<BuffMeta> {{
    BUFF_META
        .binary_search_by_key(&id, |&(bid, _, _, _, _)| bid)
        .ok()
        .map(|i| {{
            let (_, name, stack_type, capacity, classification) = BUFF_META[i];
            BuffMeta {{ name, stack_type, capacity, classification }}
        }})
}}

/// One row of [`BUFF_META`], named rather than a 4-tuple because three of
/// its fields are consulted independently by different callers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuffMeta {{
    pub name: &'static str,
    pub stack_type: BuffStackType,
    pub capacity: u32,
    /// GW2EI's `BuffClassification` (`EIData/Buffs/Buff.cs:15-30`), as
    /// written. `"Hidden"` is the one value with behaviour attached: see
    /// [`BuffMeta::is_hidden`].
    pub classification: &'static str,
}}

impl BuffMeta {{
    /// GW2EI drops a hidden buff from `buffUptimes` outright
    /// (`JsonModels/JsonActors/JsonPlayerBuilder.cs:266-269`). These are
    /// internal state markers -- "Tome of Justice Open", "Dual Fire
    /// Attunement" -- that a player holds for real but that no uptime
    /// table should list.
    pub fn is_hidden(self) -> bool {{
        self.classification == "Hidden"
    }}
}}

/// `(buff_id, icon_url)`, sorted by id.
pub static BUFF_ICONS: &[(u32, &str)] = &[
'''

META_HEADER = '''
/// `(buff_id, name, stack_type, capacity, classification)`, sorted by id,
/// for the {meta_count} buffs GW2EI defines unambiguously.
///
/// `capacity` is the CONSTRUCTOR value. arcdps' own `BUFF_INFO` row wins
/// over it wherever the log carries one -- the preference order
/// `simulate_boons_with_inputs` and `target_conditions::capacity_and_kind`
/// already use, for the reason MBUFFSIM measured: several real capacities
/// sit far above the static tables' values.
pub static BUFF_META: &[(u32, &str, BuffStackType, u32, &str)] = &[
'''

if __name__ == "__main__":
    main()
