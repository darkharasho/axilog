use serde::Serialize;
use std::collections::BTreeMap;

/// Block names the 1.0 schema defines, as an enum to make typos compile errors.
/// Adding a name is additive under the 1.x rules, renaming one is a major bump.
#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum BlockName {
    Boons,
    Cc,
    Conditions,
    Contribution,
    Damage,
    DamageMods,
    Defenses,
    Healing,
    HitStats,
    Minions,
    Missiles,
    Replay,
    Rotation,
    SelfEffects,
    Series,
    SquadBuffs,
    Support,
}

impl BlockName {
    pub const ALL: [BlockName; 17] = [
        BlockName::Boons,
        BlockName::Cc,
        BlockName::Conditions,
        BlockName::Contribution,
        BlockName::Damage,
        BlockName::DamageMods,
        BlockName::Defenses,
        BlockName::Healing,
        BlockName::HitStats,
        BlockName::Minions,
        BlockName::Missiles,
        BlockName::Replay,
        BlockName::Rotation,
        BlockName::SelfEffects,
        BlockName::Series,
        BlockName::SquadBuffs,
        BlockName::Support,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            BlockName::Boons => "boons",
            BlockName::Cc => "cc",
            BlockName::Conditions => "conditions",
            BlockName::Contribution => "contribution",
            BlockName::Damage => "damage",
            BlockName::DamageMods => "damage_mods",
            BlockName::Defenses => "defenses",
            BlockName::Healing => "healing",
            BlockName::HitStats => "hit_stats",
            BlockName::Minions => "minions",
            BlockName::Missiles => "missiles",
            BlockName::Replay => "replay",
            BlockName::Rotation => "rotation",
            BlockName::SelfEffects => "self_effects",
            BlockName::Series => "series",
            BlockName::SquadBuffs => "squad_buffs",
            BlockName::Support => "support",
        }
    }
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct AxilogMeta {
    /// The FORMAT contract version. Moves independently of `version`.
    pub schema: &'static str,
    /// The binary that produced this document (`CARGO_PKG_VERSION`).
    pub version: String,
    /// The input log's file NAME. Never a path -- paths are
    /// environment-specific and routinely carry a user name, which the PII
    /// policy scrubs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_from: Option<String>,
}

/// Why a block is or is not in `blocks`.
///
/// Without this a consumer cannot distinguish "absent because the compute
/// gate was off" from "absent because the log had nothing" -- an ambiguity
/// that turns a missing flag into silently-reported zeros.
#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CoverageState {
    /// Computed, and `blocks` carries it.
    Present,
    /// The compute gate for it was off.
    NotComputed,
    /// Computed, and there was genuinely nothing to report.
    Empty,
    /// This log's era or encounter kind cannot produce it.
    Unsupported,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Coverage(BTreeMap<&'static str, CoverageState>);

impl Coverage {
    pub fn new() -> Self {
        Coverage(BlockName::ALL.iter().map(|b| (b.as_str(), CoverageState::NotComputed)).collect())
    }

    pub fn set(&mut self, block: BlockName, state: CoverageState) {
        self.0.insert(block.as_str(), state);
    }

    pub fn get(&self, block: &str) -> Option<CoverageState> {
        self.0.get(block).copied()
    }
}

impl Default for Coverage {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warn,
    Error,
}

/// A structured, user-facing analysis warning.
///
/// The legacy `Report::warnings` is `Vec<String>`, which no consumer can act
/// on programmatically. `code` is a closed, documented set: adding a code is
/// additive, changing one's meaning is a break.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct WarningOut {
    pub code: String,
    pub severity: Severity,
    pub message: String,
    /// The entity this warning is about, when it is about one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn coverage_starts_with_every_known_block_not_computed() {
        let c = Coverage::new();
        let v = serde_json::to_value(&c).expect("serializable");
        let obj = v.as_object().expect("coverage is an object");
        assert_eq!(obj.len(), BlockName::ALL.len(), "coverage must name every block");
        for block in BlockName::ALL {
            let name = block.as_str();
            assert_eq!(obj[name], "not_computed", "block {name} must default to not_computed");
        }
    }

    #[test]
    fn coverage_states_serialize_as_documented_snake_case() {
        let mut c = Coverage::new();
        c.set(BlockName::Damage, CoverageState::Present);
        c.set(BlockName::Series, CoverageState::Empty);
        c.set(BlockName::Replay, CoverageState::Unsupported);
        let v = serde_json::to_value(&c).expect("serializable");
        assert_eq!(v["damage"], "present");
        assert_eq!(v["series"], "empty");
        assert_eq!(v["replay"], "unsupported");
        assert_eq!(v["boons"], "not_computed");
    }

    #[test]
    fn meta_omits_generated_from_when_absent() {
        let m = AxilogMeta { schema: "1.0", version: "0.3.2".into(), generated_from: None };
        let v = serde_json::to_value(&m).expect("serializable");
        assert_eq!(v["schema"], "1.0");
        assert_eq!(v["version"], "0.3.2");
        assert!(v.get("generated_from").is_none(), "absent optional fields are omitted, never null");
    }

    #[test]
    fn a_warning_carries_a_machine_readable_code_and_optional_entity() {
        let w = WarningOut {
            code: "blank_account_agent".into(),
            severity: Severity::Info,
            message: "one agent has a blank account".into(),
            entity_id: Some(37),
        };
        let v = serde_json::to_value(&w).expect("serializable");
        assert_eq!(v["code"], "blank_account_agent");
        assert_eq!(v["severity"], "info");
        assert_eq!(v["entity_id"], 37);

        let w = WarningOut { entity_id: None, ..w };
        let v = serde_json::to_value(&w).expect("serializable");
        assert!(v.get("entity_id").is_none(), "entity_id is omitted when the warning is not per-entity");
    }

    #[test]
    fn block_name_enum_makes_typos_compile_errors_and_strings_stay_unique() {
        assert_eq!(BlockName::ALL.len(), 17, "all 17 known blocks are enumerated");

        let strings: BTreeSet<&'static str> = BlockName::ALL.iter().map(|b| b.as_str()).collect();
        assert_eq!(strings.len(), 17, "all block names serialize to unique strings; no duplicates allowed");
    }
}
