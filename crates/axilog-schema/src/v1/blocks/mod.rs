//! Statistic blocks. Every block is an aggregate slot plus an entity-keyed
//! map, so a consumer learns the access pattern once.
use serde::Serialize;
use std::collections::BTreeMap;

pub mod damage;

/// The uniform entity-keyed map every block uses.
///
/// Keys serialize as decimal strings (`serde_json`'s integer-key
/// stringification), matching the catalogs.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct ByEntity<T>(pub BTreeMap<u32, T>);

impl<T> Default for ByEntity<T> {
    fn default() -> Self {
        ByEntity(BTreeMap::new())
    }
}

impl<T> ByEntity<T> {
    pub fn insert(&mut self, entity_id: u32, value: T) {
        self.0.insert(entity_id, value);
    }
    pub fn get(&self, entity_id: u32) -> Option<&T> {
        self.0.get(&entity_id)
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

#[cfg(test)]
pub(crate) mod tests_support {
    use crate::v1::entities::{build_entities, EntityIndex};

    /// The shared fixture every block test builds on: the committed
    /// anonymized WvW log, run through the real pipeline. Using the real
    /// fixture rather than hand-built structs is what makes these tests
    /// catch reprojection bugs on realistic shapes (sparse per-target maps,
    /// relogged players, NPC enemies).
    pub fn fixture_report() -> (crate::Report, EntityIndex) {
        let bytes = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/wvw-small.anon.zevtc"
        ))
        .expect("read committed fixture");
        let raw = axilog_core::evtc::decode_raw(&bytes).expect("decode fixture");
        let enc = axilog_core::model::resolve(&raw);
        let metrics = axilog_core::analysis::analyze(&enc, &raw);
        let report =
            crate::build_report(&enc, &metrics, "0.0.0-test", None, None, true, false, false, None);
        let (_, index) = build_entities(&enc, &metrics);
        (report, index)
    }
}
