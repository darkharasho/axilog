//! MBUFFSIM Task 1 — a LITERAL transcription of GW2EI's `BuffSimulatorNoID`
//! family, for DIAGNOSIS ONLY.
//!
//! Nothing in `src/` uses this. It exists so Task 1 can answer "is the gap
//! in the simulator, or upstream of it?" by running the SAME extracted
//! event stream through (a) `analysis::buffs::simulator` and (b) a
//! statement-for-statement port of the C#, and comparing both against
//! GW2EI's own `buffUptimes[]` numbers. Whichever of the two lands on EI's
//! number tells you which side the divergence is on, and diffing the two
//! ports names the rule.
//!
//! Ported files (all under `GW2EIEvtcParser/EIData/Buffs/BuffSimulators/`):
//! `AbstractBuffSimulator.cs` (`Simulate`), `BuffSimulatorNoID/
//! {BuffSimulator,BuffSimulatorDuration,BuffSimulatorIntensity}.cs`,
//! `BuffSimulatorNoID/EffectStackingLogic/{StackingLogic,QueueLogic,
//! OverrideLogic,ForceOverrideLogic}.cs`, `BuffStackItem.cs`, plus the
//! graph-value rule from `EIData/Actors/ActorsHelper/
//! SingleActorBuffsHelper.cs:1006-1020` + `BuffSimulationItems/
//! BuffSimulationItem{,Duration,Intensity}.cs`.

#![allow(dead_code)]

use axilog_core::analysis::buffs::events::{BuffEvent, BuffEventKind};

/// `ParserHelper.BuffSimulatorDelayConstant`.
const DELAY: i64 = 15;

/// `BuffStackItem`.
#[derive(Debug, Clone)]
struct Item {
    duration: i64,
    /// `Extensions` — only the values matter for a stack-count timeline.
    extensions: Vec<i64>,
}

impl Item {
    fn new(duration: i64) -> Self {
        Item { duration, extensions: Vec::new() }
    }
    /// `BuffStackItem.TotalDuration`.
    fn total(&self) -> i64 {
        self.duration + self.extensions.iter().sum::<i64>()
    }
    /// `BuffStackItem.Shift(startShift, durationShift)` — `Start` is not
    /// modelled (nothing downstream of a stack-count timeline reads it).
    fn shift(&mut self, duration_shift: i64) {
        self.duration -= duration_shift;
        if self.duration == 0 && !self.extensions.is_empty() {
            self.duration = self.extensions.remove(0);
        }
    }
}

/// `ArcDPSEnums.BuffStackType`, restricted to what the NoID family
/// dispatches on (`BuffSimulator.cs:24-43`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackType {
    /// -> `QueueLogic`, `BuffSimulatorDuration`.
    Queue,
    /// -> `ForceOverrideLogic`, `BuffSimulatorDuration`.
    Force,
    /// -> `OverrideLogic`, `BuffSimulatorIntensity`.
    Override,
}

/// One emitted `GenerationSimulation` item, reduced to what a graph needs:
/// `[start, end)` and the value `BuffSimulationItem.ToSegment()` would put
/// on it (`SingleActorBuffsHelper.cs:1010`, `GetActiveStacks()`).
#[derive(Debug, Clone, Copy)]
pub struct Seg {
    pub start: u64,
    pub end: u64,
    /// `GetActiveStacks()`: `BuffSimulationItemDuration` returns a CONSTANT
    /// `1` (`BuffSimulationItemDuration.cs:23-26`);
    /// `BuffSimulationItemIntensity` returns `GetStacks()`
    /// (`BuffSimulationItemIntensity.cs:55-58`).
    pub graph_value: u32,
    /// `GetStacks()` — the raw held-stack count, for reference.
    pub held: u32,
}

pub struct Sim {
    stacks: Vec<Item>,
    capacity: usize,
    stack_type: StackType,
    now: u64,
    pub segs: Vec<Seg>,
}

impl Sim {
    pub fn new(capacity: u32, stack_type: StackType) -> Self {
        Sim {
            stacks: Vec::new(),
            capacity: capacity.max(1) as usize,
            stack_type,
            now: 0,
            segs: Vec::new(),
        }
    }

    fn is_intensity(&self) -> bool {
        self.stack_type == StackType::Override
    }

    /// `StackingLogic.IsFull` / `ForceOverrideLogic.IsFull`.
    fn is_full(&self) -> bool {
        match self.stack_type {
            StackType::Force => self.stacks.len() == 1,
            _ => self.stacks.len() == self.capacity,
        }
    }

    /// `OverrideLogic.BinarySearchBuffStackItem` + `OverrideLogic.Add`
    /// (sorted insert, ties insert AFTER the equal element: `return
    /// ++midIndex`).
    fn override_add(&mut self, item: Item) {
        if self.stacks.is_empty() {
            self.stacks.push(item);
            return;
        }
        let td = item.total();
        let idx = binary_search(&self.stacks, td, 0, self.stacks.len() - 1);
        if idx > self.stacks.len() - 1 {
            self.stacks.push(item);
        } else {
            self.stacks.insert(idx, item);
        }
    }

    /// `BuffSimulator.Add`.
    fn add(&mut self, duration: i64, added_active: bool) {
        let item = Item::new(duration);
        if !self.is_full() {
            match self.stack_type {
                // `OverrideLogic.Add` — sorted insert.
                StackType::Override => self.override_add(item),
                // `StackingLogic.Add` — append, `Sort` is a no-op for both
                // `QueueLogic` and `ForceOverrideLogic`.
                _ => self.stacks.push(item),
            }
        } else {
            match self.stack_type {
                StackType::Override => {
                    // `OverrideLogic.FindLowestValue`: drop `stacks[0]`
                    // (the globally smallest `TotalDuration`, by the sorted
                    // invariant), then sorted-insert the new one.
                    if !self.stacks.is_empty() {
                        self.stacks.remove(0);
                        self.override_add(item);
                    }
                }
                StackType::Force => {
                    // `ForceOverrideLogic.FindLowestValue`: `stacks[0] = toAdd`.
                    if !self.stacks.is_empty() {
                        self.stacks[0] = item;
                    }
                }
                StackType::Queue => {
                    // `QueueLogic.FindLowestValue`: evict the smallest
                    // `TotalDuration` among the NON-active stacks.
                    if self.stacks.len() > 1 {
                        let (i, _) = self
                            .stacks
                            .iter()
                            .enumerate()
                            .skip(1)
                            .min_by_key(|(_, s)| s.total())
                            .unwrap();
                        self.stacks[i] = item;
                    }
                }
            }
        }
        if added_active {
            // `QueueLogic.Activate(stacks, item)`: move it to index 0.
            // `OverrideLogic`/`ForceOverrideLogic` do NOT override
            // `Activate(List, BuffStackItem)` -> `StackingLogic`'s empty
            // default, so this is a no-op for them.
            if self.stack_type == StackType::Queue {
                // The just-added item is the last appended one unless the
                // queue was full (then it replaced a middle slot). GW2EI
                // holds the reference; we approximate by moving the slot we
                // just wrote. For an append that is the tail.
                if let Some(last) = self.stacks.pop() {
                    self.stacks.insert(0, last);
                }
            }
        }
    }

    /// `BuffSimulator.Remove`.
    fn remove_single(&mut self, removed_duration: i64) {
        for i in 0..self.stacks.len() {
            if (removed_duration - self.stacks[i].total()).abs() < DELAY {
                self.stacks.remove(i);
                return;
            }
        }
    }

    /// `BuffSimulatorDuration.Extend` / `BuffSimulatorIntensity.Extend`.
    fn extend(&mut self, extension: i64, old_value: i64) {
        let full = self.is_full();
        if (!self.stacks.is_empty() && old_value > 0) || full {
            if self.stacks.is_empty() {
                return;
            }
            let idx = if self.is_intensity() {
                self.stacks
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, s)| (s.total() - old_value).abs())
                    .map(|(i, _)| i)
                    .unwrap()
            } else {
                0
            };
            self.stacks[idx].extensions.push(extension);
        } else {
            // `Add(oldValue + extension, ..., addedActive: true, ...)` on
            // the duration side; the intensity side also ends up at `Add`.
            self.add(old_value + extension, !self.is_intensity());
        }
    }

    /// `BuffSimulatorDuration.Update` / `BuffSimulatorIntensity.Update`,
    /// including the `GenerationSimulation` item each recursion emits.
    fn update(&mut self, time_passed: i64) {
        if self.stacks.is_empty() || time_passed <= 0 {
            self.now = (self.now as i64 + time_passed.max(0)) as u64;
            return;
        }
        let held = self.stacks.len() as u32;
        let diff = if self.is_intensity() {
            // `diff = Math.Min(BuffStack.Min(x => x.Duration), timePassed)`
            self.stacks.iter().map(|s| s.duration).min().unwrap().min(time_passed)
        } else {
            // `timeDiff = activeStack.Duration - timePassed; if (timeDiff <
            // 0) diff = activeStack.Duration;`
            self.stacks[0].duration.min(time_passed)
        };
        let left_over = time_passed - diff;
        let graph_value = if self.is_intensity() { held } else { 1 };
        self.segs.push(Seg {
            start: self.now,
            end: (self.now as i64 + diff) as u64,
            graph_value,
            held,
        });
        self.now = (self.now as i64 + diff) as u64;
        if self.is_intensity() {
            for s in self.stacks.iter_mut() {
                s.shift(diff);
            }
            self.stacks.retain(|s| s.duration != 0);
        } else {
            self.stacks[0].shift(diff);
            if self.stacks[0].duration == 0 {
                self.stacks.remove(0);
            }
        }
        self.update(left_over);
    }

    /// `AbstractBuffSimulator.Simulate`.
    pub fn simulate(&mut self, events: &[BuffEvent], log_start: u64, log_end: u64) {
        let mut time_prev = events.first().map(|e| e.time.min(log_start)).unwrap_or(log_start);
        self.now = time_prev;
        for e in events {
            self.update(e.time as i64 - time_prev as i64);
            match e.kind {
                BuffEventKind::Apply { duration_ms, is_shields } => {
                    // `BuffApplyEvent.UpdateSimulator`: `addedActive =
                    // _addedActive || (forceStackType4ToBeActive &&
                    // StackType == StackingConditionalLoss)`. The NoID
                    // simulator always passes `forceStackType4ToBeActive =
                    // true` (`BuffSimulator.UpdateSimulator`), but
                    // `Activate` is a no-op for `OverrideLogic`, so only
                    // `is_shields` can matter here.
                    self.add(duration_ms as i64, is_shields);
                }
                BuffEventKind::RemoveSingle { removed_duration_ms } => {
                    self.remove_single(removed_duration_ms as i64)
                }
                BuffEventKind::RemoveAll => self.stacks.clear(),
                BuffEventKind::Extend { extended_ms, new_duration_ms } => {
                    // `BuffExtensionEvent.UpdateSimulator`: extensions
                    // below 1ms never reach the simulator.
                    if extended_ms >= 1 {
                        self.extend(
                            extended_ms as i64,
                            new_duration_ms as i64 - extended_ms as i64,
                        );
                    }
                }
            }
            time_prev = e.time;
        }
        self.update(log_end as i64 - time_prev as i64);
        // `Trim(logEnd)` + `GenerationSimulation.RemoveAll(x => x.Duration
        // <= 0)`.
        for s in self.segs.iter_mut() {
            if s.end > log_end {
                s.end = log_end;
            }
        }
        self.segs.retain(|s| s.end > s.start);
    }

    /// `(presence %, time-weighted mean graph value)` over
    /// `[log_start, log_end)`, using the GRAPH value (`ToSegment()` ->
    /// `GetActiveStacks()`), which is what
    /// `BuffsTrackerSingle.GetStack(time)` reads.
    pub fn graph_uptime(&self, log_start: u64, log_end: u64) -> (f64, f64) {
        let w = (log_end - log_start) as f64;
        let (mut pres, mut integral) = (0u128, 0u128);
        for s in &self.segs {
            let a = s.start.max(log_start);
            let b = s.end.min(log_end);
            if b <= a || s.graph_value == 0 {
                continue;
            }
            pres += (b - a) as u128;
            integral += (b - a) as u128 * s.graph_value as u128;
        }
        (100.0 * pres as f64 / w, integral as f64 / w)
    }

    /// Same, but over `GetStacks()` (the raw held count) — for duration
    /// buffs this is what `analysis::buffs::simulator` reports and EI does
    /// NOT.
    pub fn held_uptime(&self, log_start: u64, log_end: u64) -> (f64, f64) {
        let w = (log_end - log_start) as f64;
        let (mut pres, mut integral) = (0u128, 0u128);
        for s in &self.segs {
            let a = s.start.max(log_start);
            let b = s.end.min(log_end);
            if b <= a || s.held == 0 {
                continue;
            }
            pres += (b - a) as u128;
            integral += (b - a) as u128 * s.held as u128;
        }
        (100.0 * pres as f64 / w, integral as f64 / w)
    }
}

/// `OverrideLogic.BinarySearchBuffStackItem`, transcribed including its
/// `return ++midIndex` tie rule.
fn binary_search(stacks: &[Item], total: i64, min_index: usize, max_index: usize) -> usize {
    if stacks[min_index].total() > total {
        return min_index;
    }
    if stacks[max_index].total() < total {
        return max_index + 1;
    }
    if min_index > max_index {
        return min_index;
    }
    let mid = (min_index + max_index) / 2;
    if total == stacks[mid].total() {
        return mid + 1;
    }
    if total < stacks[mid].total() {
        if mid == 0 {
            return min_index;
        }
        binary_search(stacks, total, min_index, mid - 1)
    } else {
        binary_search(stacks, total, mid + 1, max_index)
    }
}
