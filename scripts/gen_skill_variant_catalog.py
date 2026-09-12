#!/usr/bin/env python3
"""Regenerate `analysis::skill_variants` from the official GW2 API.

Some skills are several separate skill ids sharing ONE display name, and an
arcdps log names them all identically. This table gives an id a readable
label when the API proves which variant it is. `v1::catalogs` attaches the
label to every catalog entry this table covers; an id it does not cover
carries no label, and a consumer merges same-name unlabelled rows. There is
no id fallback -- a raw id is not a label a player can read.

Two families are covered:

Warrior bursts. A burst is five ids: the weapon-slot skill, the Berserker
(primal burst) variant, and three adrenaline-tier variants. The Berserker id
is labelled `Primal Burst`; the tiers `Adrenaline N` where known.

Attunement variants. A name group where two or more ids carry different
`attunement` values (Deploy Jade Sphere, the elementalist glyphs) labels
each attuned id with its attunement. The unattuned base skill stays
unlabelled.

The partition inside a five-id group, all read straight off `/v2/skills`:

  slot skill   a non-null `cost` AND a non-null `flip_skill` -- the skill
               on the weapon bar that flips to whichever tier the player
               has adrenaline for. Not a tier itself, so skipped.
  Berserker    `specialization == 61` -- the primal burst replacement.
               Not an adrenaline tier either, so skipped.
  the tiers    the remaining three, whose tier is read from the
               `traited_facts` entry with `requires_trait == 1649`
               (Cleansing Ire, which cleanses one condition per bar of
               adrenaline spent, so its `value` IS the tier).

The trait-fact rule was VALIDATED against the one group with independent
ground truth and it FAILED. Harrier's Toss (72911 / 73042 / 73006) is
carried by GW2EI's OverridenSkillNames as Adrenaline Level 1 / 2 / 3; the
requires_trait:1649 fact returns level 1 for all five of its ids.
Breaching Strike behaves the same way; Path to Victory is internally
inconsistent; Bloodthirster has no trait facts at all.

Those four groups are therefore NOT derived here. Harrier's Toss is
hand-transcribed from GW2EI (the only group with an independent source).
The other three emit no tier labels; their tier ids stay unlabelled. Do not "finish the job" by
re-deriving them from this rule -- it is known to lie on exactly this
shape of input.

Nothing here guesses. A group whose three remaining ids do not yield the
distinct tiers 1, 2 and 3 raises `Skip` for every one of them, WITH the
reason, and emits nothing rather than a plausible-looking wrong answer: a
mislabelled `Earthshaker (Adrenaline 3)` is worse than an unlabelled one,
because it is wrong in a way a player would believe. The accounting
printed at the end -- `considered == transcribed + skipped` -- is the
machine-diff behind that claim.

A group whose five-id shape does not partition into 1 slot + 1 Berserker
+ 3 tiers is a HARD ERROR, not a skip: the API has changed shape and the
premise this catalog rests on needs rechecking by a human.

Usage:

    python3 scripts/gen_skill_variant_catalog.py [cached-skills.json]

then `git diff`: a clean tree means the committed catalog is exactly what
the current GW2 API returns. With no argument it fetches from the API;
pass a previously-fetched `/v2/skills` array to regenerate offline.
Standard library only.
"""

import collections
import json
import os
import sys
import time
import urllib.request

API = "https://api.guildwars2.com/v2/skills"
BATCH = 200

#: Cleansing Ire. Its traited fact cleanses one condition per bar of
#: adrenaline the burst consumed, so the fact's `value` is the tier.
CLEANSING_IRE = 1649

#: The Berserker specialization; its primal burst replaces the base burst.
BERSERKER = 61

#: Hand-transcribed from GW2EI's `OverridenSkillNames`. This is the ONE
#: group with a source independent of the trait-fact derivation, and the
#: group that caught the derivation lying -- see the module doc. Do not
#: extend this dict from the API; anything added here needs its own
#: independent source, named.
GW2EI_GROUND_TRUTH = {
    72911: 1,  # Harrier's Toss (Adrenaline Level 1)
    73042: 2,  # Harrier's Toss (Adrenaline Level 2)
    73006: 3,  # Harrier's Toss (Adrenaline Level 3)
}

OUT = os.path.normpath(
    os.path.join(
        os.path.dirname(os.path.abspath(__file__)),
        "..", "crates", "axilog-core", "src", "analysis", "skill_variants.rs",
    )
)


class Skip(Exception):
    """An id that cannot be transcribed, carrying the reason why."""


class ShapeChanged(Exception):
    """The API no longer has the shape this catalog's premise rests on."""


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


def cleansing_ire_tier(skill):
    """The adrenaline tier `skill`'s Cleansing Ire fact implies, or `None`.

    `None` for "the fact is absent", which is not the same as a tier of 0.
    """
    for fact in skill.get("traited_facts") or []:
        if fact.get("requires_trait") == CLEANSING_IRE:
            value = fact.get("value")
            if isinstance(value, int):
                return value
    return None


def partition(group):
    """`(slot, berserker, tiers)` for one five-id burst group.

    Raises `ShapeChanged` rather than guessing when the group does not
    split into exactly one of each plus three tier candidates.
    """
    slot, berserker, tiers = [], [], []
    for skill in group:
        # Berserker FIRST: the primal burst is itself a weapon-slot skill,
        # so several of them (Kill Shot's 42041) carry both a cost and a
        # flip skill and would be mistaken for the base slot skill. The
        # specialization is the unambiguous discriminator; no base burst
        # carries one.
        if skill.get("specialization") == BERSERKER:
            berserker.append(skill)
        elif skill.get("cost") is not None and skill.get("flip_skill") is not None:
            slot.append(skill)
        else:
            tiers.append(skill)
    if len(slot) != 1 or len(berserker) != 1 or len(tiers) != 3:
        raise ShapeChanged(
            f"{group[0]['name']!r} did not partition into 1 slot + 1 Berserker"
            f" + 3 tiers: slot={[s['id'] for s in slot]}"
            f" berserker={[s['id'] for s in berserker]}"
            f" tiers={[s['id'] for s in tiers]}"
        )
    return slot[0], berserker[0], tiers


def transcribe_group(name, tiers):
    """`{id: tier}` for the three tier ids, or `Skip` with why not.

    Ground truth wins outright where it exists. Otherwise the Cleansing
    Ire facts must name the three distinct tiers 1, 2 and 3 -- anything
    else and the whole group is skipped, because a derivation that
    disagrees with itself has not established any of the three.
    """
    if all(sid in GW2EI_GROUND_TRUTH for sid in (s["id"] for s in tiers)):
        return {s["id"]: GW2EI_GROUND_TRUTH[s["id"]] for s in tiers}

    derived = {s["id"]: cleansing_ire_tier(s) for s in tiers}
    if all(t is None for t in derived.values()):
        raise Skip(f"{name}: no Cleansing Ire trait fact on any id in the group")
    if sorted(t for t in derived.values() if t is not None) != [1, 2, 3]:
        raise Skip(
            f"{name}: the Cleansing Ire facts do not name the three distinct tiers"
            " 1/2/3, so the derivation contradicts itself"
        )
    return derived


def attunement_rows(skills):
    """`[(id, attunement)]` for every name group whose ids differ by attunement.

    Only ids that carry an attunement are labelled. Two ids with the SAME
    attunement get the same label -- they are the same variant, and a
    consumer merging on `(name, label)` is right to fold them.
    """
    groups = collections.defaultdict(list)
    for skill in skills:
        groups[skill["name"]].append(skill)
    rows = []
    for name in sorted(groups):
        attuned = [s for s in groups[name] if s.get("attunement")]
        if len({s["attunement"] for s in attuned}) < 2:
            continue
        rows.extend((s["id"], s["attunement"]) for s in attuned)
    return rows


def main():
    skills = json.load(open(sys.argv[1])) if len(sys.argv) > 1 else fetch_all()

    bursts = [s for s in skills if "Burst" in (s.get("categories") or [])]
    groups = collections.defaultdict(list)
    for skill in bursts:
        groups[skill["name"]].append(skill)

    rows, skipped = [], collections.Counter()
    labelled, unlabelled = [], []
    for name in sorted(groups):
        group = sorted(groups[name], key=lambda s: s["id"])
        if len(group) != 5:
            skipped["not a five-id burst group (no name collision to resolve)"] += len(group)
            continue
        slot, berserker, tiers = partition(group)
        skipped["weapon-slot burst skill, not an adrenaline tier"] += 1
        rows.append((berserker["id"], "Primal Burst"))
        try:
            for skill_id, tier in sorted(transcribe_group(name, tiers).items()):
                rows.append((skill_id, f"Adrenaline {tier}"))
            labelled.append(name)
        except Skip as e:
            skipped[str(e)] += len(tiers)
            unlabelled.append((name, str(e)))

    burst_rows = len(rows)
    attuned = attunement_rows(skills)
    rows.extend(attuned)
    rows.sort()
    ids = [skill_id for skill_id, _ in rows]
    if len(ids) != len(set(ids)):
        raise RuntimeError("an id was labelled by both families")

    total = sum(skipped.values())
    if len(bursts) != burst_rows + total:
        raise RuntimeError(
            f"accounting must balance: considered {len(bursts)} != "
            f"transcribed {burst_rows} + skipped {total}"
        )

    # No leading indent on a skip table: rustdoc reads a 4-space-indented
    # block in a doc comment as a Rust code sample and tries to compile it.
    skip_table = "\n".join(f"//! - {n} {reason}" for reason, n in skipped.most_common())

    with open(OUT, "w") as f:
        f.write(HEADER.format(
            count=burst_rows,
            attuned=len(attuned),
            considered=len(bursts),
            skipped=sum(skipped.values()),
            skip_table=skip_table,
            groups=len(groups),
            labelled=len(labelled),
            unlabelled="\n".join(f"//! - {why}" for _n, why in unlabelled),
        ))
        for skill_id, label in rows:
            f.write(f'    ({skill_id}, "{label}"),\n')
        f.write("];\n")
        f.write(FOOTER)

    print(f"considered {len(bursts)} = transcribed {burst_rows} + skipped {total}")
    print(f"attunement labels: {len(attuned)}")
    for reason, n in skipped.most_common():
        print(f"  skipped {n}: {reason}")
    print(f"groups: {len(groups)} burst names, {len(labelled)} labelled, "
          f"{len(unlabelled)} undecidable")
    for _name, why in unlabelled:
        print(f"  no label: {why}")


HEADER = '''//! Readable variant labels for skill ids that share a display name, from
//! the official GW2 API.
//!
//! GENERATED by `scripts/gen_skill_variant_catalog.py` -- do not
//! hand-edit. Re-run it and `git diff` to verify this table against the
//! live API.
//!
//! [`axilog_schema::v1`]'s catalog builder attaches a label to every skill
//! entry this table covers, whether or not its siblings are in the same
//! log, so a label never changes between logs. An id the table does not
//! cover carries NO label -- never the bare id -- and consumers merge
//! same-name unlabelled rows.
//!
//! Two families:
//!
//! - **Warrior bursts.** One burst is five ids sharing one name -- the
//!   weapon-slot skill, the Berserker primal burst, and three adrenaline
//!   tiers. The Berserker id (`specialization == 61`) is `Primal Burst`;
//!   the tiers are `Adrenaline N` where the tier is genuinely known. The
//!   slot skill is unlabelled.
//! - **Attunement variants.** Where two or more same-name ids carry
//!   different `attunement` values (Deploy Jade Sphere, the elementalist
//!   glyphs), each attuned id is labelled with its attunement. Same-name
//!   ids with the same attunement share a label and merge.
//!
//! The tier is read from the `traited_facts` entry with
//! `requires_trait == 1649` (Cleansing Ire, which cleanses one condition
//! per bar of adrenaline spent, so its `value` is the tier).
//!
//! That rule was VALIDATED against the one group with independent ground
//! truth and it FAILED. Harrier's Toss (72911 / 73042 / 73006) is carried
//! by GW2EI's `OverridenSkillNames` as Adrenaline Level 1 / 2 / 3; the
//! `requires_trait: 1649` fact returns level 1 for all five of its ids.
//! Breaching Strike behaves the same way; Path to Victory is internally
//! inconsistent; Bloodthirster has no trait facts at all.
//!
//! Those four groups are therefore NOT derived. Harrier's Toss is
//! hand-transcribed from GW2EI (the only group with an independent
//! source). The other three emit NO tier labels, so their tiers merge.
//! Their absence is the correct answer, not an oversight -- do not
//! "finish the job" by re-deriving them from this rule, which is known to
//! lie on exactly this shape of input:
//!
{unlabelled}
//!
//! The generator's burst accounting (the Berserker ids are transcribed as
//! `Primal Burst`):
//!
//! considered {considered} = transcribed {count} + skipped {skipped}
//!
{skip_table}
//!
//! ({groups} burst display names in all, {labelled} of them tier-labelled.)
//!
//! Attunement labels: {attuned}.
//!
//! Entries are sorted by id so lookups can binary-search.
//!
//! The tests below are part of the generated file: they are the assertions
//! a regeneration has to keep satisfying, so they belong next to the table
//! they pin rather than somewhere a regeneration would not touch.

/// The variant label for skill `id`, or `None` when this table does not
/// cover it -- in which case the entry carries no label at all.
pub fn label(id: u32) -> Option<&'static str> {{
    SKILL_VARIANT_LABELS
        .binary_search_by_key(&id, |&(sid, _)| sid)
        .ok()
        .map(|i| SKILL_VARIANT_LABELS[i].1)
}}

/// `(skill id, variant label)`, sorted by id.
pub const SKILL_VARIANT_LABELS: &[(u32, &str)] = &[
'''

FOOTER = '''
#[cfg(test)]
mod tests {
    use super::*;

    /// The lookup binary-searches, so an unsorted table silently returns
    /// wrong answers rather than failing.
    #[test]
    fn the_table_is_sorted_and_has_no_duplicate_ids() {
        for pair in SKILL_VARIANT_LABELS.windows(2) {
            assert!(pair[0].0 < pair[1].0, "unsorted or duplicated at {:?}", pair[0]);
        }
    }

    /// Ground truth from GW2EI's OverridenSkillNames -- the ONE group with
    /// a source independent of the trait-fact derivation, and the group
    /// where that derivation was caught lying. If this breaks, the
    /// generator has started guessing again.
    #[test]
    fn harriers_toss_matches_the_gw2ei_override_table() {
        assert_eq!(label(72911), Some("Adrenaline 1"));
        assert_eq!(label(73042), Some("Adrenaline 2"));
        assert_eq!(label(73006), Some("Adrenaline 3"));
    }

    /// The group BreakN reported. Pinned so a regenerated table that
    /// reshuffles these three fails loudly instead of silently relabelling
    /// the skill players are looking at.
    #[test]
    fn earthshaker_carries_its_three_adrenaline_tiers() {
        assert_eq!(label(14512), Some("Adrenaline 1"));
        assert_eq!(label(14513), Some("Adrenaline 2"));
        assert_eq!(label(14514), Some("Adrenaline 3"));
    }

    /// The three groups the derivation could not resolve are ABSENT on
    /// purpose; their tier ids carry no label. Asserting the absence
    /// stops a future edit from quietly filling them with a guess.
    ///
    /// Naming the nine ids matters more than the count does: a table that
    /// labelled Bloodthirster from ascending id order and dropped three
    /// legitimate entries elsewhere would still be 27 long.
    #[test]
    fn the_undecidable_groups_carry_no_label() {
        // Bloodthirster has no Cleansing Ire trait facts at all.
        for id in [80221, 80248, 80263] {
            assert_eq!(label(id), None, "Bloodthirster {id} must stay unlabelled");
        }
        // Breaching Strike's facts all say level 1, the same way Harrier's
        // Toss's do -- where GW2EI proves that answer wrong.
        for id in [69245, 69392, 69433] {
            assert_eq!(label(id), None, "Breaching Strike {id} must stay unlabelled");
        }
        // Path to Victory's facts are internally inconsistent.
        for id in [71922, 71950, 72029] {
            assert_eq!(label(id), None, "Path to Victory {id} must stay unlabelled");
        }

        let tiers = SKILL_VARIANT_LABELS
            .iter()
            .filter(|(_, l)| l.starts_with("Adrenaline "))
            .count();
        assert_eq!(tiers, 27, "9 groups x 3 tiers");
    }

    /// Harrier's Toss's Berserker id. The slot id (73024) is unlabelled.
    #[test]
    fn a_berserker_burst_id_is_labelled_primal_burst() {
        assert_eq!(label(73014), Some("Primal Burst"));
        assert_eq!(label(73024), None);
    }

    /// The Catalyst jade sphere ids seen in the committed fixture.
    #[test]
    fn deploy_jade_sphere_is_labelled_by_attunement() {
        assert_eq!(label(62723), Some("Water"));
        assert_eq!(label(62813), Some("Fire"));
        assert_eq!(label(62940), Some("Air"));
    }

    /// Labels are for players: no label may be a bare number.
    #[test]
    fn no_label_is_a_raw_id() {
        for (id, l) in SKILL_VARIANT_LABELS {
            assert!(!l.chars().all(|c| c.is_ascii_digit()), "{id} has a numeric label {l:?}");
        }
    }

    #[test]
    fn an_unknown_id_has_no_label() {
        assert_eq!(label(1), None);
    }
}
'''

if __name__ == "__main__":
    main()
