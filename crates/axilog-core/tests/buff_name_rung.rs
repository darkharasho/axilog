//! Where the buff tables sit in `skill_map::resolve_name`, and what that
//! costs and buys.
//!
//! A buff id reaches `resolve_name` like any other skill id -- through the
//! log's skill table -- and when the capturing client had no name cached
//! for it, every skill-shaped rung misses, because none of them is a buff
//! table. `analysis::buffs::name` composes the four this crate already
//! owns, so the answer was in the process the whole time.
//!
//! Two questions, two tests:
//!
//! - [`buff_rung_cannot_displace_a_real_skill_name`] pins the guarantee.
//!   The rung ranks below the log table, `pseudo_name`, the API catalog
//!   and the override table, so for every id those sources name, the
//!   shipped chain must still return THEIR name, not the buff table's.
//!
//! - [`buff_rung_outranks_the_symbol_rung`] pins the ordering decision.
//!   Unlike its neighbours this rung is deliberately not additive: it
//!   sits ABOVE `skill_symbol_names` and displaces it wherever the two
//!   disagree, because the symbol rung ships de-camel-cased C# identifiers
//!   and the buff table ships GW2's real display strings.

use axilog_core::analysis::{buff_icons, buffs, skill_icons, skill_map, skill_symbol_names};

/// The rung is last-resort with respect to every source that carries a
/// REAL skill name. Walk every id the buff tables know and check that
/// wherever a higher rung also knows it, the higher rung still wins.
#[test]
fn buff_rung_cannot_displace_a_real_skill_name() {
    let mut checked = 0usize;
    for &(id, ..) in buff_icons::BUFF_META {
        let Some(buff_name) = buffs::name(id) else { continue };

        // The API catalog ranks above this rung.
        if let Some(api) = skill_icons::name(id) {
            assert_eq!(
                skill_map::resolve_name(id, None),
                api,
                "id {id}: the API catalog ranks above the buff rung, but the buff \
                 name {buff_name:?} won"
            );
            checked += 1;
        }

        // So does the log's own skill table, whatever it says.
        let from_log = skill_map::resolve_name(id, Some("Log Table Name"));
        assert_eq!(
            from_log, "Log Table Name",
            "id {id}: a usable log name must outrank the buff name {buff_name:?}"
        );
    }
    assert!(checked > 0, "no id was covered by both the API catalog and the buff tables");
}

/// The half that is NOT additive, stated as an assertion so a future
/// reorder has to argue with it. Where `BUFF_META` and the symbol table
/// disagree, the shipped chain returns the buff table's name.
#[test]
fn buff_rung_outranks_the_symbol_rung() {
    let mut disagreements = 0usize;
    for &(id, ..) in buff_icons::BUFF_META {
        let (Some(buff), Some(symbol)) = (buffs::name(id), skill_symbol_names::name(id)) else {
            continue;
        };
        if buff == symbol {
            continue;
        }
        // Only meaningful where no higher rung answers first.
        if skill_icons::name(id).is_some() {
            continue;
        }
        disagreements += 1;
        assert_eq!(
            skill_map::resolve_name(id, None),
            buff,
            "id {id}: expected the buff table's display name over the symbol {symbol:?}"
        );
    }
    assert!(
        disagreements > 100,
        "expected the two tables to disagree on many ids, saw {disagreements} -- if a \
         catalog resync collapsed the overlap this test no longer pins anything"
    );
}

/// The id that motivated the rung: a core boon rendering as its own
/// number. Guards the specific regression, not just the shape.
#[test]
fn resolution_is_named_not_numbered() {
    assert_eq!(skill_map::resolve_name(873, None), "Resolution");
    assert_eq!(skill_map::resolve_name(873, Some("")), "Resolution");
    assert_eq!(skill_map::resolve_name(873, Some("873")), "Resolution");
}
