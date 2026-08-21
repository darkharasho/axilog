#!/usr/bin/env python3
"""Regenerate `analysis::skill_icons` from the official GW2 API.

axilog computes a skill's name from the log's own skill table, which is all
an arcdps log carries. Icons and auto-attack classification are NOT in the
log -- they live in ArenaNet's skill database -- so they are EXTRACTED here
rather than guessed, the same discipline `gen_damage_mod_catalog.py` applies
to the GW2EI damage-modifier definitions.

Two INDEPENDENT tables are produced, each with its own accounting:

  SKILL_ICONS  `(id, icon_signature, icon_file_id, auto_attack)`
  SKILL_NAMES  `(id, name)`

They are deliberately not one table. A skill can have art but no name
(92 of them) or a name but no usable art (46), and folding the two into
one row would force a sentinel or an `Option` on every one of the ~4,700
entries to express an absence that affects ~3% of them. Split, each table
states exactly what it knows and its `considered == transcribed + skipped`
balance means something on its own.

The fields, all read straight off `/v2/skills`:

  icon         `skill.icon`, verbatim.
  name         `skill.name`, trimmed. An arcdps log carries skill names
               only for what its client had cached at capture time, and
               writes nothing (or a bare numeric placeholder) for the
               rest -- `analysis::skill_map::resolve_name` falls back to
               this table before it resorts to `"Skill <id>"`. Not a
               replacement for the log's own name: where the log names a
               skill, the log wins, so this table cannot move a name that
               already resolves.
  auto_attack  `skill.slot == "Weapon_1"`. GW2 has no `autoAttack` field;
               the first weapon slot IS the auto-attack chain, and that
               positional rule is the same one GW2EI applies. Skills with
               NO slot at all (transforms, shared/bundle entries, most
               non-equippable skills) get `None`, not `false` -- absence of
               a slot means the question does not apply, and answering
               `false` would assert something the API never said.

Nothing here guesses. A skill missing an icon, or carrying one that does
not match the render-service URL shape, raises `Skip` and lands in the
skipped table WITH its reason instead of producing a wrong entry. The
accounting printed at the end -- `considered == transcribed + skipped` --
is the machine-diff behind the catalog's completeness claim.

Icons are stored as `(signature, file_id)` rather than as whole URLs: every
one of them is `https://render.guildwars2.com/file/<SIG>/<FILE_ID>.png`,
verified for all of them at generation time, so keeping the shared prefix
4,700 times over would be pure payload. `skill_icons::icon` rebuilds it.

Usage:

    python3 scripts/gen_skill_icon_catalog.py [cached-skills.json]

then `git diff`: a clean tree means the committed catalog is exactly what
the current GW2 API returns. With no argument it fetches from the API;
pass a previously-fetched `/v2/skills` array to regenerate offline.
Standard library only.
"""

import collections
import json
import os
import re
import sys
import time
import urllib.request

API = "https://api.guildwars2.com/v2/skills"
BATCH = 200
ICON_RE = re.compile(r"^https://render\.guildwars2\.com/file/([0-9A-F]{40})/(\d+)\.png$")

OUT = os.path.normpath(
    os.path.join(
        os.path.dirname(os.path.abspath(__file__)),
        "..", "crates", "axilog-core", "src", "analysis", "skill_icons.rs",
    )
)


class Skip(Exception):
    """A skill that cannot be transcribed, carrying the reason why."""


def get(url, attempts=3):
    for attempt in range(attempts):
        try:
            with urllib.request.urlopen(url, timeout=45) as r:
                return json.load(r)
        except Exception:
            if attempt == attempts - 1:
                raise
            time.sleep(2)


def fetch_all():
    ids = get(API)
    out = []
    for i in range(0, len(ids), BATCH):
        batch = ",".join(str(x) for x in ids[i:i + BATCH])
        out.extend(get(f"{API}?ids={batch}"))
        print(f"  fetched {len(out)}/{len(ids)}", file=sys.stderr)
    return out


def transcribe(skill):
    icon = skill.get("icon")
    if not icon:
        raise Skip("no icon in the API record")
    m = ICON_RE.match(icon)
    if not m:
        raise Skip("icon URL does not match the render-service shape")
    slot = skill.get("slot")
    auto = None if slot is None else (slot == "Weapon_1")
    return m.group(1), int(m.group(2)), auto


def transcribe_name(skill):
    """The skill's display name, or `Skip` with why it cannot be used.

    A name is rejected on exactly the conditions that make it useless as a
    display name -- absent, blank, or a bare numeric string. That last one
    mirrors `skill_map::resolve_name`'s own rejection rule: an all-digits
    name is what the game writes when it has no name to give, and letting
    it through would replace one placeholder with another.
    """
    name = str(skill.get("name") or "").strip()
    if not name:
        raise Skip("no name in the API record")
    if name.isdigit():
        raise Skip("name is a bare numeric placeholder")
    return name


def rust_str(value):
    """A Rust string literal. Names carry quotes and backslashes."""
    return '"' + value.replace("\\", "\\\\").replace('"', '\\"') + '"'


def main():
    skills = json.load(open(sys.argv[1])) if len(sys.argv) > 1 else fetch_all()

    rows, skipped = [], collections.Counter()
    name_rows, name_skipped = [], collections.Counter()
    for skill in sorted(skills, key=lambda s: s["id"]):
        try:
            sig, file_id, auto = transcribe(skill)
            rows.append((skill["id"], sig, file_id, auto))
        except Skip as e:
            skipped[str(e)] += 1
        try:
            name_rows.append((skill["id"], transcribe_name(skill)))
        except Skip as e:
            name_skipped[str(e)] += 1

    # No leading indent on a skip table: rustdoc reads a 4-space-indented
    # block in a doc comment as a Rust code sample and tries to compile it.
    def skip_table(counter):
        return "\n".join(f"//! - {n} {reason}" for reason, n in counter.most_common())

    with open(OUT, "w") as f:
        f.write(HEADER.format(
            count=len(rows),
            considered=len(skills),
            skipped=sum(skipped.values()),
            skip_table=skip_table(skipped),
            name_count=len(name_rows),
            name_skipped=sum(name_skipped.values()),
            name_skip_table=skip_table(name_skipped),
        ))
        for skill_id, sig, file_id, auto in rows:
            auto_lit = {None: "None", True: "Some(true)", False: "Some(false)"}[auto]
            f.write(f'    ({skill_id}, "{sig}", {file_id}, {auto_lit}),\n')
        f.write("];\n")
        f.write(NAMES_HEADER.format(name_count=len(name_rows)))
        for skill_id, name in name_rows:
            f.write(f"    ({skill_id}, {rust_str(name)}),\n")
        f.write("];\n")

    for label, transcribed, counter in (
        ("icons", len(rows), skipped),
        ("names", len(name_rows), name_skipped),
    ):
        total = sum(counter.values())
        print(f"{label}: considered {len(skills)} = transcribed {transcribed} + skipped {total}")
        for reason, n in counter.most_common():
            print(f"  skipped {n}: {reason}")
        assert len(skills) == transcribed + total, f"{label} accounting must balance"


HEADER = '''//! Skill icons, names and auto-attack classification, from the official
//! GW2 API.
//!
//! GENERATED by `scripts/gen_skill_icon_catalog.py` -- do not hand-edit.
//! Re-run it and `git diff` to verify these tables against the live API.
//!
//! An arcdps log carries skill NAMES -- badly, and only for what the
//! capturing client had cached -- and nothing else about a skill. Icons
//! and auto-attack status are never in the log at all. All three come
//! from ArenaNet's database here, extracted rather than inferred.
//!
//! Two INDEPENDENT tables, because the absences do not coincide: a skill
//! can have art but no name, or a name but no usable art. Folding them
//! into one row would put an `Option` on every entry to express something
//! true of ~3% of them, and would collapse two completeness claims into
//! one that means less than either.
//!
//! The generator's accounting for [`SKILL_ICONS`]:
//!
//! considered {considered} = transcribed {count} + skipped {skipped}
//!
{skip_table}
//!
//! and for [`SKILL_NAMES`]:
//!
//! considered {considered} = transcribed {name_count} + skipped {name_skipped}
//!
{name_skip_table}
//!
//! Entries in both are sorted by id so lookups can binary-search.

/// The render service every icon lives on. Factored out of the table
/// because it is identical for all {count} entries.
const RENDER_PREFIX: &str = "https://render.guildwars2.com/file/";

/// The icon URL for `id`, or `None` when the GW2 API has no icon for it.
///
/// Rebuilt from the stored `(signature, file_id)` -- see the module doc.
pub fn icon(id: u32) -> Option<String> {{
    lookup(id).map(|&(_, sig, file_id, _)| format!("{{RENDER_PREFIX}}{{sig}}/{{file_id}}.png"))
}}

/// Whether `id` is an auto-attack, or `None` when the question does not
/// apply (the API gives it no weapon/utility slot) or the skill is unknown.
///
/// Two different absences collapse to `None` here on purpose: callers
/// treat "we cannot say" identically either way, and neither is `false`.
pub fn auto_attack(id: u32) -> Option<bool> {{
    lookup(id).and_then(|&(_, _, _, auto)| auto)
}}

/// The GW2 API's display name for `id`, or `None` when the API has no
/// usable name for it (it has none at all, or the skill is not an API
/// skill -- boons, conditions and arcdps pseudo-skills are not).
///
/// This is a FALLBACK for [`super::skill_map::resolve_name`], never a
/// replacement: a skill the log's own table names keeps that name.
pub fn name(id: u32) -> Option<&'static str> {{
    SKILL_NAMES
        .binary_search_by_key(&id, |&(sid, _)| sid)
        .ok()
        .map(|i| SKILL_NAMES[i].1)
}}

fn lookup(id: u32) -> Option<&'static (u32, &'static str, u32, Option<bool>)> {{
    SKILL_ICONS
        .binary_search_by_key(&id, |&(sid, _, _, _)| sid)
        .ok()
        .map(|i| &SKILL_ICONS[i])
}}

/// `(skill_id, icon_signature, icon_file_id, auto_attack)`.
///
/// `auto_attack` is `None` when the API gives the skill no slot at all --
/// the question does not apply, which is not the same as `Some(false)`.
pub static SKILL_ICONS: &[(u32, &str, u32, Option<bool>)] = &[
'''

NAMES_HEADER = '''
/// `(skill_id, display_name)`, for the {name_count} API skills that have a
/// usable one.
///
/// Independent of [`SKILL_ICONS`] -- see the module doc for why the two
/// are not one table.
pub static SKILL_NAMES: &[(u32, &str)] = &[
'''

if __name__ == "__main__":
    main()
