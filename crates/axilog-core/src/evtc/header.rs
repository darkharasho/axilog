use super::{EvtcError, HEADER_SIZE};

#[derive(Debug, Clone)]
pub struct RawHeader { pub build: String, pub revision: u8, pub boss_id: u16 }

impl RawHeader {
    /// Whether this log's arcdps build is on/after the
    /// `BuffAppliesAndRemovesAsStateChanges` / `ResultEnumRework` threshold
    /// (GW2EI's `ArcDPSBuilds`, `20260501`) -- see
    /// `analysis::buffs::events`'s module doc for the full version-split
    /// explanation. Post-threshold builds emit buff apply/remove/initial
    /// rows as dedicated statechange event kinds instead of the older
    /// `is_statechange == 0` combat-event shape this project's
    /// `events::extract_buff_events`/`support::apply` currently decode --
    /// so this project's boon/support metrics read all-zero on such a log
    /// (see `is_post_buff_rework` / `analysis::analyze`'s warning surface).
    pub fn is_post_buff_rework(&self) -> bool {
        is_post_buff_rework(&self.build)
    }

    /// Strictly after GW2EI's `ArcDPSBuilds.ProperConfusionDamageSimulation`
    /// (`20210529`, `GW2EIEvtcParser/ParserHelpers/ArcDPSEnums.cs:12`) --
    /// the build gate on `CombatData.HasStackIDs`
    /// (`ParsedData/CombatData.cs:610`), which in turn gates the
    /// `StackingConditionalLoss` `RemovedDuration` band aid
    /// (`EIData/Buffs/BuffsContainer.cs:197`). Note GW2EI's comparison is
    /// STRICT `>`, not `>=`. Malformed builds are treated as "before"
    /// (`false`), same conservative rule as
    /// [`RawHeader::is_post_buff_rework`].
    pub fn has_proper_confusion_damage_simulation(&self) -> bool {
        let b = &self.build;
        b.len() == 8 && b.bytes().all(|c| c.is_ascii_digit()) && b.as_str() > "20210529"
    }

    /// On/after GW2EI's `ArcDPSBuilds.BuffExtensionOverstackValueChanged`
    /// (`20231107`, `ArcDPSEnums.cs:22`): from this build a `BUFF_INITIAL`
    /// row's `buff_dmg` carries the stack's ORIGINAL as-cast duration while
    /// `value` carries what is left of it, so GW2EI's
    /// `BuffApplyEvent.OriginalAppliedDuration` reads `buff_dmg` rather than
    /// `value` (`ParsedData/CombatEvents/BuffEvents/BuffApplies/
    /// BuffApplyEvent.cs:21-28`). The difference is its `activeTime`, which
    /// the `StackingConditionalLoss` band aid subtracts
    /// (`BuffsContainer.cs:243`).
    pub fn has_buff_extension_overstack_value_changed(&self) -> bool {
        let b = &self.build;
        b.len() == 8 && b.bytes().all(|c| c.is_ascii_digit()) && b.as_str() >= "20231107"
    }
}

/// Free-function form of `RawHeader::is_post_buff_rework`, for callers that
/// only have the build string (e.g. tests). `build` is the raw 8-byte
/// "yyyymmdd" arcdps build string (`RawHeader::build`) -- a plain string
/// compare against `"20260501"` is correct for this fixed-width, zero-padded
/// numeric format (no need to actually parse it as a date). Malformed builds
/// (wrong length, or containing anything other than ASCII digits -- e.g. a
/// truncated/garbled header) are treated as pre-rework (`false`) rather than
/// guessed either way, since we have no reliable signal for them.
pub fn is_post_buff_rework(build: &str) -> bool {
    if build.len() != 8 || !build.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    build >= "20260501"
}

pub fn decode_header(buf: &[u8]) -> Result<RawHeader, EvtcError> {
    if buf.len() < HEADER_SIZE {
        return Err(EvtcError::Truncated { need: HEADER_SIZE, at: 0, have: buf.len() });
    }
    if &buf[0..4] != b"EVTC" { return Err(EvtcError::BadMagic); }
    let build = String::from_utf8_lossy(&buf[4..12]).trim_end_matches('\0').to_string();
    let revision = buf[12];
    let boss_id = u16::from_le_bytes([buf[13], buf[14]]);
    Ok(RawHeader { build, revision, boss_id })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> Vec<u8> {
        // "EVTC" + "20260114" + rev 1 + boss_id 1 (LE u16) + skip 0
        let mut b = Vec::new();
        b.extend_from_slice(b"EVTC");
        b.extend_from_slice(b"20260114");
        b.push(1);                       // revision
        b.extend_from_slice(&1u16.to_le_bytes()); // boss id
        b.push(0);                       // skip
        b
    }
    #[test]
    fn parses_header_fields() {
        let h = decode_header(&sample()).unwrap();
        assert_eq!(h.build, "20260114");
        assert_eq!(h.revision, 1);
        assert_eq!(h.boss_id, 1);
    }
    /// Final-review fix wave: pre-threshold build is NOT post-rework.
    #[test]
    fn is_post_buff_rework_false_before_threshold() {
        assert!(!is_post_buff_rework("20260114"));
        assert!(!RawHeader { build: "20260114".into(), revision: 1, boss_id: 1 }.is_post_buff_rework());
    }

    /// The threshold build itself, and anything after it, count as post-rework.
    #[test]
    fn is_post_buff_rework_true_on_and_after_threshold() {
        assert!(is_post_buff_rework("20260501"));
        assert!(is_post_buff_rework("20260601"));
        assert!(is_post_buff_rework("20990101"));
    }

    /// Malformed build strings (wrong length, non-digit bytes) must not
    /// panic or be guessed either way -- treated as pre-rework.
    #[test]
    fn is_post_buff_rework_false_for_malformed_build() {
        assert!(!is_post_buff_rework(""));
        assert!(!is_post_buff_rework("2026051")); // 7 chars
        assert!(!is_post_buff_rework("202605011")); // 9 chars
        assert!(!is_post_buff_rework("2026050x"));
    }

    #[test]
    fn rejects_bad_magic() {
        let mut b = sample(); b[0] = b'X';
        assert!(decode_header(&b).is_err());
    }
    #[test]
    fn rejects_short_buffer() {
        assert!(decode_header(b"EVTC").is_err());
    }
}
