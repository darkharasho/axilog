//! The SDK type stubs must not drift behind the 1.0 container.
//!
//! Both stubs are HAND-transcribed from the Rust schema
//! (`crates/axilog-py/axilog.pyi`, `crates/axilog-node/types.d.ts`), which
//! means nothing but discipline kept them current -- and discipline lost.
//! A 2026-08-16 audit against a real document found 9 Python types and 8
//! TypeScript types missing fields, including two whole blocks
//! (`conditions`, `minions`) that the Python stub still described as
//! "reserved for spec #2, always not_computed". `minions` was even
//! self-documented as a known gap in `types.d.ts`. That is exactly the
//! failure mode this test exists to make loud.
//!
//! The check is deliberately NAME-level, not type-level: every field name
//! the container can serialize must appear somewhere in each stub. What it
//! catches is the thing that actually went wrong -- a field lands in the
//! schema and no one transcribes it. What it does NOT catch is a name
//! transcribed onto the WRONG type, or a required/optional mismatch; a
//! faithful check of those needs a real Python/TypeScript parser, which is
//! not worth a build dependency here. Reviewers still own placement.
//!
//! It reads `v1-keyset.golden.txt` rather than rebuilding the document,
//! which makes the coupling explicit and cheap: that golden is already
//! CI-gated and already regenerated (`UPDATE_GOLDEN=1`) whenever the shape
//! moves, so a new field cannot reach `main` without passing through this
//! test on the way.

const GOLDEN: &str = include_str!("v1-keyset.golden.txt");
const PYI: &str = include_str!("../../axilog-py/axilog.pyi");
const DTS: &str = include_str!("../../axilog-node/types.d.ts");

/// Field names that are real wire keys but are NOT expected to appear
/// verbatim in a stub, with the reason each is exempt. Anything else must
/// be transcribed.
const EXEMPT: &[(&str, &str)] = &[
    // The container serializes maps keyed by entity/skill/buff/modifier id.
    // `v1_shape`'s walker already collapses those to `<id>`, and a stub
    // spells them as an index signature (`Dict[str, X]` / `Record<string,
    // X>`), so there is no literal name to look for.
    ("<id>", "an id-keyed map slot, not a field name"),
    // Same reasoning, for the one dynamic map keyed by something other
    // than an id: `blocks.damage_mods.personal` is keyed by SPEC name.
    ("<spec>", "a spec-keyed map slot, not a field name"),
];

/// Wire keys the key-set golden cannot see, because they are emitted only
/// for a PvE encounter and that golden is built from the committed WvW
/// fixture.
///
/// All four are `skip_serializing_if = "Option::is_none"` on
/// `v1::EncounterOut`, and `None` is exactly what a WvW log produces --
/// so they are absent from `v1-keyset.golden.txt` by construction, and
/// without this list the stub check would pass while both SDK stubs stayed
/// silent about PvE entirely. That is the same "a field lands in the schema
/// and no one transcribes it" failure this file exists to catch; it just
/// arrives through a blind spot in the golden rather than through neglect.
///
/// The real fix is a PvE key-set golden (the fixtures for it are committed,
/// `fixtures/pve/`). Until then, this list is the seam -- keep it in sync
/// with `v1::EncounterOut`'s optional PvE fields.
const PVE_ONLY_WIRE_FIELDS: &[&str] =
    &["encounter_name", "trigger_id", "sub_category", "success"];

/// Every distinct leaf field name the 1.0 container can emit, from the
/// committed key-set golden plus [`PVE_ONLY_WIRE_FIELDS`].
fn wire_field_names() -> Vec<String> {
    let mut names: Vec<String> = GOLDEN
        .lines()
        .filter_map(|line| line.trim().rsplit('.').next())
        // `foo[]` is the array marker the golden's walker appends to a
        // path segment; the field itself is `foo`.
        .map(|seg| seg.trim_end_matches("[]").to_string())
        .filter(|seg| !seg.is_empty())
        .filter(|seg| !EXEMPT.iter().any(|(name, _)| name == seg))
        .chain(PVE_ONLY_WIRE_FIELDS.iter().map(|s| s.to_string()))
        .collect();
    names.sort();
    names.dedup();
    names
}

/// A stub "declares" a name if it appears as a word. Substring matching
/// would let `hits` satisfy `connected_hits`, so the match is bounded on
/// both sides by a non-identifier character.
fn declares(stub: &str, name: &str) -> bool {
    let is_ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    stub.match_indices(name).any(|(i, _)| {
        let before_ok = i == 0 || !stub[..i].chars().next_back().is_some_and(is_ident);
        let after = i + name.len();
        let after_ok = after >= stub.len() || !stub[after..].chars().next().is_some_and(is_ident);
        before_ok && after_ok
    })
}

fn assert_stub_is_current(stub: &str, label: &str, path: &str) {
    let missing: Vec<String> =
        wire_field_names().into_iter().filter(|n| !declares(stub, n)).collect();
    assert!(
        missing.is_empty(),
        "{label} ({path}) is missing {} field name(s) the 1.0 container emits:\n  {}\n\n\
         Each of these appears in tests/v1-keyset.golden.txt, so the schema really does \
         serialize it. Transcribe it into the stub (matching the Rust field's \
         `skip_serializing_if` for required-vs-optional), then re-run.",
        missing.len(),
        missing.join("\n  ")
    );
}

#[test]
fn python_stub_declares_every_wire_field() {
    assert_stub_is_current(PYI, "the Python stub", "crates/axilog-py/axilog.pyi");
}

#[test]
fn node_stub_declares_every_wire_field() {
    assert_stub_is_current(DTS, "the Node stub", "crates/axilog-node/types.d.ts");
}

/// Guards the guard: `declares` must not accept a name just because a
/// LONGER field contains it. Without the word boundary, `hits` would be
/// satisfied by `connected_hits` and the whole check would rot into a
/// substring search that almost always passes.
#[test]
fn declares_requires_a_whole_word_not_a_substring() {
    assert!(declares("    hits: int", "hits"));
    assert!(declares("  connected_hits?: number", "connected_hits"));
    assert!(!declares("  connected_hits?: number", "hits"));
    assert!(!declares("  total_downed: int", "downed"));
    assert!(declares("total: number\nmin: number", "min"));
}
