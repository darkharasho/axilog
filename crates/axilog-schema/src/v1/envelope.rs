use serde::Serialize;
use std::collections::BTreeMap;

/// Every block name the 1.0 schema defines. Fixed by spec #1 so spec #2
/// fills reserved slots rather than renegotiating the container; adding a
/// name is additive under the 1.x rules, renaming one is a major bump.
pub const BLOCK_NAMES: [&str; 15] = [
    "cc",
    "conditions",
    "contribution",
    "damage",
    "damage_mods",
    "defenses",
    "healing",
    "hit_stats",
    "minions",
    "missiles",
    "replay",
    "rotation",
    "series",
    "support",
    "boons",
];

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
        Coverage(BLOCK_NAMES.iter().map(|n| (*n, CoverageState::NotComputed)).collect())
    }

    pub fn set(&mut self, block: &'static str, state: CoverageState) {
        debug_assert!(BLOCK_NAMES.contains(&block), "unknown block name {block}");
        self.0.insert(block, state);
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

    #[test]
    fn coverage_starts_with_every_known_block_not_computed() {
        let c = Coverage::new();
        let v = serde_json::to_value(&c).expect("serializable");
        let obj = v.as_object().expect("coverage is an object");
        assert_eq!(obj.len(), BLOCK_NAMES.len(), "coverage must name every block");
        for name in BLOCK_NAMES {
            assert_eq!(obj[name], "not_computed", "block {name} must default to not_computed");
        }
    }

    #[test]
    fn coverage_states_serialize_as_documented_snake_case() {
        let mut c = Coverage::new();
        c.set("damage", CoverageState::Present);
        c.set("series", CoverageState::Empty);
        c.set("replay", CoverageState::Unsupported);
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
}
