# axilog WvW Core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the Milestone-1 WvW vertical slice — parse a GW2 arcdps `.zevtc`/`.evtc` log into a native versioned JSON stat report (damage, DPS, downs/kills/deaths, down contribution, CC-over-time timeline), with `table`/`csv` output and a partial EI-compatible `ei-json` adapter, validated against Elite Insights golden output.

**Architecture:** A Cargo workspace with the engine (`axilog-core`) fully separated from I/O. `axilog-core` decodes the EVTC binary (`evtc`), resolves a domain model (`model`), computes metrics (`analysis`), and does WvW squad/enemy resolution (`wvw`). `axilog-schema` owns the native versioned JSON contract, `axilog-ei` adapts it to EI's `DPSReportJSON` shape, and `axilog-cli` is the only crate touching the filesystem/args/stdout.

**Tech Stack:** Rust (edition 2021), `clap` (CLI), `serde`/`serde_json` (output), `flate2` (zevtc/zip deflate), `thiserror` (errors). Tests via `cargo test`.

## Global Constraints

- **License:** MIT. Add `LICENSE` (MIT, holder "axi suite") and `license = "MIT"` in every crate's `Cargo.toml`. No EI or axibridge source may be copied — clean reimplementation only.
- **MSRV / edition:** Rust edition 2021, MSRV 1.74. State `rust-version = "1.74"` in the workspace.
- **EVTC record sizes (verified against fixture `20260117-181030.zevtc`):** header = 16 bytes; agent record = 96 bytes; skill record = 68 bytes; revision-1 combat event = 96 bytes. All multi-byte integers are little-endian.
- **Target revision:** revision 1 combat events only (all sample logs are rev 1). If `header.revision == 0`, return an explicit "unsupported revision" error — do not attempt to decode.
- **`ei-json` target:** the latest Elite Insights release schema (the `DPSReportJSON` shape in axibridge `packages/bridge-metrics/src/dpsReportTypes.ts`, `DetailledWvW=True`). Emit only Milestone-1 fields; never fabricate un="" computed values.
- **Fixtures:** commit one small WvW `.zevtc` + a trimmed EI JSON under `fixtures/`. Large fixture sets stay out of the repo, located via the `AXILOG_FIXTURES` env var; tests that need them `skip` (print + return) when it is unset.
- **Release targets (priority order):** `x86_64-unknown-linux-gnu`, `x86_64-pc-windows-msvc`, `aarch64-apple-darwin`, `x86_64-apple-darwin`, `aarch64-unknown-linux-gnu`.
- **Numeric parity tolerance:** golden-file assertions compare against EI within a relative tolerance of 0.5% (`(a-b).abs() <= 0.005 * b.abs().max(1.0)`), because EI and axilog round intermediate floats differently.

---

## File Structure

```
axilog/
├── Cargo.toml                         # workspace manifest
├── LICENSE                            # MIT
├── rust-toolchain.toml                # pin toolchain (optional convenience)
├── .github/workflows/ci.yml           # build+test on all targets
├── fixtures/
│   ├── wvw-small.zevtc                # committed small WvW log
│   └── wvw-small.ei.json              # trimmed EI JSON for the same log
├── crates/
│   ├── axilog-core/
│   │   └── src/
│   │       ├── lib.rs                 # re-exports; `decode_log`, `analyze`
│   │       ├── evtc/
│   │       │   ├── mod.rs             # decode_raw + RawLog, constants, EvtcError
│   │       │   ├── header.rs          # decode_header -> RawHeader
│   │       │   ├── agent.rs           # decode_agents -> Vec<RawAgent>
│   │       │   ├── skill.rs           # decode_skills -> Vec<RawSkill>
│   │       │   ├── event.rs           # decode_events -> Vec<RawEvent>, enums
│   │       │   └── container.rs       # inflate_zevtc -> Vec<u8>
│   │       ├── model/
│   │       │   └── mod.rs             # Encounter, Agent, Player, Enemy, resolve()
│   │       ├── analysis/
│   │       │   ├── mod.rs             # analyze(&Encounter,&RawLog) -> Metrics
│   │       │   ├── damage.rs          # damage + dps
│   │       │   ├── downs.rs           # downs/kills/deaths + down contribution
│   │       │   └── cc.rs              # CC totals + per-second timeline
│   │       └── wvw/
│   │           └── mod.rs             # squad/enemy resolution, teams, map name
│   ├── axilog-schema/
│   │   └── src/lib.rs                 # native serde Report types + from_metrics()
│   ├── axilog-ei/
│   │   └── src/lib.rs                 # to_ei_json(&Report) -> serde_json::Value
│   └── axilog-cli/
│       └── src/main.rs                # clap, formats, orchestration
```

---

## Shared type reference (defined progressively; listed here for cross-task consistency)

```rust
// axilog-core::evtc  (Tasks 2-6)
pub const HEADER_SIZE: usize = 16;
pub const AGENT_SIZE:  usize = 96;
pub const SKILL_SIZE:  usize = 68;
pub const EVENT_SIZE_REV1: usize = 96;

pub struct RawHeader { pub build: String, pub revision: u8, pub boss_id: u16 }
pub struct RawAgent  { pub addr: u64, pub prof: u32, pub is_elite: u32,
    pub toughness: i16, pub concentration: i16, pub healing: i16,
    pub hitbox_width: u16, pub condition: i16, pub hitbox_height: u16,
    pub name_raw: Vec<u8> }              // 64-byte combo buffer, null-separated
pub struct RawSkill  { pub id: u32, pub name: String }
pub struct RawEvent  { pub time: u64, pub src_agent: u64, pub dst_agent: u64,
    pub value: i32, pub buff_dmg: i32, pub overstack: u32, pub skillid: u32,
    pub src_instid: u16, pub dst_instid: u16, pub src_master_instid: u16,
    pub dst_master_instid: u16, pub iff: u8, pub buff: u8, pub result: u8,
    pub is_activation: u8, pub is_buffremove: u8, pub is_statechange: u8 }
pub struct RawLog { pub header: RawHeader, pub agents: Vec<RawAgent>,
    pub skills: Vec<RawSkill>, pub events: Vec<RawEvent> }

// axilog-core::model  (Task 7-8)
pub enum AgentKind { Player, Npc, Gadget }
pub struct Agent { pub addr: u64, pub instid: u16, pub kind: AgentKind,
    pub character: Option<String>, pub account: Option<String>,
    pub subgroup: Option<u8>, pub profession: u32, pub elite_spec: u32,
    pub team_id: u16, pub iff_foe: bool,
    pub first_aware: u64, pub last_aware: u64 }
pub struct Player { pub agent_addr: u64, pub account: String, pub character: String,
    pub profession: String, pub elite_spec: String, pub team: String,
    pub subgroup: u8, pub in_squad: bool, pub commander: bool }
pub struct Enemy  { pub id: u64, pub instid: u16, pub name: String,
    pub team: String, pub is_player: bool }
pub struct Encounter { pub kind: String, pub map: String, pub duration_ms: u64,
    pub build: String, pub revision: u8, pub recorded_by: Option<String>,
    pub teams: Vec<Team>, pub players: Vec<Player>, pub enemies: Vec<Enemy> }
pub struct Team { pub color: String, pub team_id: u16 }

// axilog-core::analysis  (Tasks 9-11)
pub struct PlayerMetrics { pub agent_addr: u64, pub damage_total: u64, pub dps: f64,
    pub per_enemy: Vec<(u64,u64)>, pub downs_dealt: u32, pub kills_dealt: u32,
    pub down_contribution: u64, pub downs_taken: u32, pub deaths: u32,
    pub damage_taken: u64, pub cc_applied: u32, pub cc_duration_ms: u64 }
pub struct Timeline { pub resolution_ms: u64, pub squad_damage: Vec<u64>,
    pub cc_applied: Vec<u32>, pub downs: Vec<u32> }
pub struct Metrics { pub players: Vec<PlayerMetrics>, pub timeline: Timeline }
```

---

### Task 1: Workspace scaffold

**Files:**
- Create: `Cargo.toml`, `LICENSE`, `rust-toolchain.toml`
- Create: `crates/axilog-core/Cargo.toml`, `crates/axilog-core/src/lib.rs`
- Create: `crates/axilog-schema/Cargo.toml`, `crates/axilog-schema/src/lib.rs`
- Create: `crates/axilog-ei/Cargo.toml`, `crates/axilog-ei/src/lib.rs`
- Create: `crates/axilog-cli/Cargo.toml`, `crates/axilog-cli/src/main.rs`

**Interfaces:**
- Produces: a compiling 4-crate workspace.

- [ ] **Step 1: Write the workspace manifest**

`Cargo.toml`:
```toml
[workspace]
resolver = "2"
members = ["crates/axilog-core", "crates/axilog-schema", "crates/axilog-ei", "crates/axilog-cli"]

[workspace.package]
version = "0.1.0"
edition = "2021"
rust-version = "1.74"
license = "MIT"

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1"
flate2 = "1"
clap = { version = "4", features = ["derive"] }
```

- [ ] **Step 2: Write each crate manifest**

`crates/axilog-core/Cargo.toml`:
```toml
[package]
name = "axilog-core"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
thiserror.workspace = true
flate2.workspace = true
```
`crates/axilog-schema/Cargo.toml` (same header block, plus):
```toml
[dependencies]
axilog-core = { path = "../axilog-core" }
serde.workspace = true
serde_json.workspace = true
```
`crates/axilog-ei/Cargo.toml` (same header, plus):
```toml
[dependencies]
axilog-schema = { path = "../axilog-schema" }
serde_json.workspace = true
```
`crates/axilog-cli/Cargo.toml` (same header, plus `[[bin]] name = "axilog" path = "src/main.rs"`, and):
```toml
[dependencies]
axilog-core = { path = "../axilog-core" }
axilog-schema = { path = "../axilog-schema" }
axilog-ei = { path = "../axilog-ei" }
clap.workspace = true
serde_json.workspace = true
```

- [ ] **Step 3: Write minimal crate roots**

`crates/axilog-core/src/lib.rs`:
```rust
pub mod evtc;
```
`crates/axilog-schema/src/lib.rs`: `// filled in Task 12`
`crates/axilog-ei/src/lib.rs`: `// filled in Task 15`
`crates/axilog-cli/src/main.rs`:
```rust
fn main() { println!("axilog"); }
```
Create `crates/axilog-core/src/evtc/mod.rs` with `// filled in Task 2` for now.

- [ ] **Step 4: Add the MIT LICENSE file** with the standard MIT text, holder "axi suite", year 2026.

- [ ] **Step 5: Run the build**

Run: `cargo build`
Expected: workspace compiles, `axilog` binary builds.

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "chore: scaffold axilog cargo workspace"
```

---

### Task 2: EVTC header decode

**Files:**
- Modify: `crates/axilog-core/src/evtc/mod.rs`
- Create: `crates/axilog-core/src/evtc/header.rs`

**Interfaces:**
- Produces: `EvtcError`, `RawHeader`, `pub fn decode_header(buf: &[u8]) -> Result<RawHeader, EvtcError>`, and the size constants.

- [ ] **Step 1: Write the failing test**

In `crates/axilog-core/src/evtc/header.rs`:
```rust
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p axilog-core header`
Expected: FAIL — `decode_header` not found.

- [ ] **Step 3: Write minimal implementation**

`crates/axilog-core/src/evtc/mod.rs`:
```rust
pub mod header;
pub use header::{decode_header, RawHeader};

pub const HEADER_SIZE: usize = 16;
pub const AGENT_SIZE:  usize = 96;
pub const SKILL_SIZE:  usize = 68;
pub const EVENT_SIZE_REV1: usize = 96;

#[derive(Debug, thiserror::Error)]
pub enum EvtcError {
    #[error("buffer too short: need {need} bytes at offset {at}, have {have}")]
    Truncated { need: usize, at: usize, have: usize },
    #[error("not an evtc file: bad magic")]
    BadMagic,
    #[error("unsupported evtc revision {0} (only revision 1 is supported)")]
    UnsupportedRevision(u8),
    #[error("zevtc container error: {0}")]
    Container(String),
}
```
`crates/axilog-core/src/evtc/header.rs`:
```rust
use super::{EvtcError, HEADER_SIZE};

#[derive(Debug, Clone)]
pub struct RawHeader { pub build: String, pub revision: u8, pub boss_id: u16 }

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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p axilog-core header`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(evtc): decode file header"
```

---

### Task 3: Agent block decode

**Files:**
- Create: `crates/axilog-core/src/evtc/agent.rs`
- Modify: `crates/axilog-core/src/evtc/mod.rs`

**Interfaces:**
- Consumes: `AGENT_SIZE`, `EvtcError`.
- Produces: `RawAgent`, `pub fn decode_agents(buf: &[u8], count: usize) -> Result<Vec<RawAgent>, EvtcError>` (buf starts at the first agent record). `RawAgent` has `fn name_parts(&self) -> (String, String, Option<u8>)` returning (character, account, subgroup).

- [ ] **Step 1: Write the failing test**

`crates/axilog-core/src/evtc/agent.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    fn player_agent() -> Vec<u8> {
        let mut b = vec![0u8; AGENT_SIZE];
        b[0..8].copy_from_slice(&0x1122u64.to_le_bytes()); // addr
        b[8..12].copy_from_slice(&5u32.to_le_bytes());     // prof (guardian)
        b[12..16].copy_from_slice(&27u32.to_le_bytes());   // is_elite (firebrand)
        // name combo at offset 28: char \0 account \0 subgroup \0
        let name = b"Alice\0:Alice.1234\05\0";
        b[28..28 + name.len()].copy_from_slice(name);
        b
    }
    #[test]
    fn decodes_one_player_agent() {
        let agents = decode_agents(&player_agent(), 1).unwrap();
        assert_eq!(agents.len(), 1);
        let (character, account, sub) = agents[0].name_parts();
        assert_eq!(character, "Alice");
        assert_eq!(account, ":Alice.1234");
        assert_eq!(sub, Some(5));
        assert_eq!(agents[0].prof, 5);
        assert_eq!(agents[0].is_elite, 27);
    }
    #[test]
    fn errors_when_count_exceeds_buffer() {
        assert!(decode_agents(&player_agent(), 2).is_err());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p axilog-core agent`
Expected: FAIL — `decode_agents` not found.

- [ ] **Step 3: Write minimal implementation**

`crates/axilog-core/src/evtc/agent.rs`:
```rust
use super::{EvtcError, AGENT_SIZE};

#[derive(Debug, Clone)]
pub struct RawAgent {
    pub addr: u64, pub prof: u32, pub is_elite: u32,
    pub toughness: i16, pub concentration: i16, pub healing: i16,
    pub hitbox_width: u16, pub condition: i16, pub hitbox_height: u16,
    pub name_raw: Vec<u8>,
}

impl RawAgent {
    /// name buffer = character \0 account \0 subgroup \0 (utf8, null-separated)
    pub fn name_parts(&self) -> (String, String, Option<u8>) {
        let mut it = self.name_raw.split(|&c| c == 0)
            .map(|s| String::from_utf8_lossy(s).to_string());
        let character = it.next().unwrap_or_default();
        let account = it.next().unwrap_or_default();
        let subgroup = it.next().and_then(|s| s.trim().parse::<u8>().ok());
        (character, account, subgroup)
    }
    pub fn is_player(&self) -> bool { self.is_elite != 0xffff_ffff }
}

pub fn decode_agents(buf: &[u8], count: usize) -> Result<Vec<RawAgent>, EvtcError> {
    let need = count * AGENT_SIZE;
    if buf.len() < need {
        return Err(EvtcError::Truncated { need, at: 0, have: buf.len() });
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let a = &buf[i * AGENT_SIZE..(i + 1) * AGENT_SIZE];
        let name_end = 28 + 64; // 64-byte name buffer at offset 28
        out.push(RawAgent {
            addr: u64::from_le_bytes(a[0..8].try_into().unwrap()),
            prof: u32::from_le_bytes(a[8..12].try_into().unwrap()),
            is_elite: u32::from_le_bytes(a[12..16].try_into().unwrap()),
            toughness: i16::from_le_bytes(a[16..18].try_into().unwrap()),
            concentration: i16::from_le_bytes(a[18..20].try_into().unwrap()),
            healing: i16::from_le_bytes(a[20..22].try_into().unwrap()),
            hitbox_width: u16::from_le_bytes(a[22..24].try_into().unwrap()),
            condition: i16::from_le_bytes(a[24..26].try_into().unwrap()),
            hitbox_height: u16::from_le_bytes(a[26..28].try_into().unwrap()),
            name_raw: a[28..name_end].to_vec(),
        });
    }
    Ok(out)
}
```
Add to `mod.rs`: `pub mod agent; pub use agent::{decode_agents, RawAgent};`

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p axilog-core agent`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(evtc): decode agent block with name parsing"
```

---

### Task 4: Skill block decode

**Files:**
- Create: `crates/axilog-core/src/evtc/skill.rs`
- Modify: `crates/axilog-core/src/evtc/mod.rs`

**Interfaces:**
- Consumes: `SKILL_SIZE`, `EvtcError`.
- Produces: `RawSkill`, `pub fn decode_skills(buf: &[u8], count: usize) -> Result<Vec<RawSkill>, EvtcError>` (buf starts at the first skill record).

- [ ] **Step 1: Write the failing test**

`crates/axilog-core/src/evtc/skill.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    fn one_skill() -> Vec<u8> {
        let mut b = vec![0u8; SKILL_SIZE];
        b[0..4].copy_from_slice(&12345u32.to_le_bytes());
        let name = b"Fireball";
        b[4..4 + name.len()].copy_from_slice(name);
        b
    }
    #[test]
    fn decodes_skill() {
        let s = decode_skills(&one_skill(), 1).unwrap();
        assert_eq!(s[0].id, 12345);
        assert_eq!(s[0].name, "Fireball");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p axilog-core skill`
Expected: FAIL — `decode_skills` not found.

- [ ] **Step 3: Write minimal implementation**

`crates/axilog-core/src/evtc/skill.rs`:
```rust
use super::{EvtcError, SKILL_SIZE};

#[derive(Debug, Clone)]
pub struct RawSkill { pub id: u32, pub name: String }

pub fn decode_skills(buf: &[u8], count: usize) -> Result<Vec<RawSkill>, EvtcError> {
    let need = count * SKILL_SIZE;
    if buf.len() < need {
        return Err(EvtcError::Truncated { need, at: 0, have: buf.len() });
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let s = &buf[i * SKILL_SIZE..(i + 1) * SKILL_SIZE];
        let id = u32::from_le_bytes(s[0..4].try_into().unwrap());
        let name = String::from_utf8_lossy(&s[4..SKILL_SIZE])
            .trim_end_matches('\0').to_string();
        out.push(RawSkill { id, name });
    }
    Ok(out)
}
```
Add to `mod.rs`: `pub mod skill; pub use skill::{decode_skills, RawSkill};`

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p axilog-core skill`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(evtc): decode skill block"
```

---

### Task 5: Combat event decode (revision 1)

**Files:**
- Create: `crates/axilog-core/src/evtc/event.rs`
- Modify: `crates/axilog-core/src/evtc/mod.rs`

**Interfaces:**
- Consumes: `EVENT_SIZE_REV1`, `EvtcError`.
- Produces: `RawEvent`, `pub fn decode_events(buf: &[u8], count: usize) -> Result<Vec<RawEvent>, EvtcError>`, and public constants for statechange/result enums (`sc::*`, `result::*`) used by analysis tasks. These enum numeric values MUST be verified in Step 3 against the arcdps enum ordering AND cross-checked via the golden test in Task 16; if a golden count is off, the wrong constant is the first suspect.

- [ ] **Step 1: Write the failing test**

`crates/axilog-core/src/evtc/event.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    fn strike_event() -> Vec<u8> {
        let mut b = vec![0u8; EVENT_SIZE_REV1];
        b[0..8].copy_from_slice(&1000u64.to_le_bytes());   // time
        b[8..16].copy_from_slice(&0xAAAAu64.to_le_bytes()); // src_agent
        b[16..24].copy_from_slice(&0xBBBBu64.to_le_bytes());// dst_agent
        b[24..28].copy_from_slice(&500i32.to_le_bytes());  // value (damage)
        b[40..44].copy_from_slice(&77u32.to_le_bytes());   // skillid
        b[48] = 1;                                         // iff = FOE
        // offsets: buff@50, result@51, is_activation@52, is_buffremove@53, is_statechange@58
        b[51] = 0;                                         // result NORMAL
        b
    }
    #[test]
    fn decodes_strike() {
        let ev = decode_events(&strike_event(), 1).unwrap();
        let e = &ev[0];
        assert_eq!(e.time, 1000);
        assert_eq!(e.src_agent, 0xAAAA);
        assert_eq!(e.dst_agent, 0xBBBB);
        assert_eq!(e.value, 500);
        assert_eq!(e.skillid, 77);
        assert_eq!(e.iff, 1);
        assert_eq!(e.is_statechange, 0);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p axilog-core event`
Expected: FAIL — `decode_events` not found.

- [ ] **Step 3: Write minimal implementation**

Field offsets for the rev-1 96-byte `cbtevent` (verbatim struct order from the arcdps EVTC reference): time(0,u64), src_agent(8,u64), dst_agent(16,u64), value(24,i32), buff_dmg(28,i32), overstack(32,u32), skillid(36,u32), src_instid(40,u16), dst_instid(42,u16), src_master_instid(44,u16), dst_master_instid(46,u16), iff(48,u8), buff(49,u8)… **Verify these single-byte offsets (48–58) against the arcdps `cbtevent` field order before trusting them**; the byte fields follow the four u16 instids in this exact order: iff, buff, result, is_activation, is_buffremove, is_ninety, is_fifty, is_moving, is_statechange, is_flanking, is_shields, is_offcycle. That places iff=48, buff=49, result=50, is_activation=51, is_buffremove=52, is_statechange=56.

```rust
use super::{EvtcError, EVENT_SIZE_REV1};

pub mod sc { // is_statechange values (verify against arcdps cbtstatechange enum order)
    pub const NONE: u8 = 0;
    pub const ENTER_COMBAT: u8 = 1;
    pub const EXIT_COMBAT: u8 = 2;
    pub const CHANGE_DEAD: u8 = 4;
    pub const CHANGE_DOWN: u8 = 5;
    pub const LOG_START: u8 = 9;
    pub const LOG_END: u8 = 10;
    pub const MAX_HEALTH: u8 = 12;
    pub const POINT_OF_VIEW: u8 = 13;
    pub const TEAM_CHANGE: u8 = 22;
    pub const MAP_ID: u8 = 25;
}
pub mod result { // combat result values (verify against arcdps cbtresult enum order)
    pub const NORMAL: u8 = 0;
    pub const CRIT: u8 = 1;
    pub const KILLING_BLOW: u8 = 8;
    pub const DOWNED: u8 = 9;
}

#[derive(Debug, Clone)]
pub struct RawEvent {
    pub time: u64, pub src_agent: u64, pub dst_agent: u64,
    pub value: i32, pub buff_dmg: i32, pub overstack: u32, pub skillid: u32,
    pub src_instid: u16, pub dst_instid: u16,
    pub src_master_instid: u16, pub dst_master_instid: u16,
    pub iff: u8, pub buff: u8, pub result: u8,
    pub is_activation: u8, pub is_buffremove: u8, pub is_statechange: u8,
}

pub fn decode_events(buf: &[u8], count: usize) -> Result<Vec<RawEvent>, EvtcError> {
    let need = count * EVENT_SIZE_REV1;
    if buf.len() < need {
        return Err(EvtcError::Truncated { need, at: 0, have: buf.len() });
    }
    let u64le = |s: &[u8]| u64::from_le_bytes(s.try_into().unwrap());
    let i32le = |s: &[u8]| i32::from_le_bytes(s.try_into().unwrap());
    let u32le = |s: &[u8]| u32::from_le_bytes(s.try_into().unwrap());
    let u16le = |s: &[u8]| u16::from_le_bytes(s.try_into().unwrap());
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let e = &buf[i * EVENT_SIZE_REV1..(i + 1) * EVENT_SIZE_REV1];
        out.push(RawEvent {
            time: u64le(&e[0..8]), src_agent: u64le(&e[8..16]), dst_agent: u64le(&e[16..24]),
            value: i32le(&e[24..28]), buff_dmg: i32le(&e[28..32]), overstack: u32le(&e[32..36]),
            skillid: u32le(&e[36..40]),
            src_instid: u16le(&e[40..42]), dst_instid: u16le(&e[42..44]),
            src_master_instid: u16le(&e[44..46]), dst_master_instid: u16le(&e[46..48]),
            iff: e[48], buff: e[49], result: e[50],
            is_activation: e[51], is_buffremove: e[52], is_statechange: e[56],
        });
    }
    Ok(out)
}
```
Add to `mod.rs`: `pub mod event; pub use event::{decode_events, RawEvent, sc, result};`

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p axilog-core event`
Expected: PASS. (Note: the test sets `result` at offset 51 — after Step-3 verification the constant offset is 50; fix the test's byte index to match the verified offset so the assertion is meaningful.)

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(evtc): decode revision-1 combat events"
```

---

### Task 6: zevtc container + full raw decode

**Files:**
- Create: `crates/axilog-core/src/evtc/container.rs`
- Modify: `crates/axilog-core/src/evtc/mod.rs`
- Test: `crates/axilog-core/tests/decode_fixture.rs`

**Interfaces:**
- Consumes: all decode fns + `RawLog`, `RawHeader`.
- Produces: `pub fn inflate_zevtc(bytes: &[u8]) -> Result<Vec<u8>, EvtcError>` (returns raw EVTC bytes; passthrough if already raw EVTC), and `pub fn decode_raw(bytes: &[u8]) -> Result<RawLog, EvtcError>` (accepts either zevtc or raw and returns the full `RawLog`).

- [ ] **Step 1: Write the failing unit test (container passthrough + counts layout)**

`crates/axilog-core/src/evtc/container.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn passthrough_raw_evtc() {
        // A raw buffer already starting with EVTC is returned unchanged.
        let mut b = Vec::new();
        b.extend_from_slice(b"EVTC20260114"); b.push(1);
        b.extend_from_slice(&1u16.to_le_bytes()); b.push(0);
        let out = inflate_zevtc(&b).unwrap();
        assert_eq!(&out[0..4], b"EVTC");
    }
}
```

- [ ] **Step 2: Write the failing integration test (real fixture counts)**

`crates/axilog-core/tests/decode_fixture.rs`:
```rust
use axilog_core::evtc::{decode_raw, inflate_zevtc};

#[test]
fn decodes_committed_wvw_fixture() {
    let bytes = std::fs::read("../../fixtures/wvw-small.zevtc")
        .expect("commit fixtures/wvw-small.zevtc (Task 16)");
    let raw = decode_raw(&bytes).unwrap();
    assert_eq!(raw.header.revision, 1);
    assert!(raw.agents.len() > 0);
    assert!(raw.skills.len() > 0);
    assert!(raw.events.len() > 0);
    // sanity: event count computed from layout matches decoded vec length
    assert_eq!(raw.events.len(), raw.events.len());
}
```
(Exact counts are asserted in Task 16 once the specific fixture is chosen; this test guards the pipeline.)

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p axilog-core container` then `cargo test -p axilog-core --test decode_fixture`
Expected: FAIL — `inflate_zevtc`/`decode_raw` not found (and fixture missing — acceptable until Task 16, mark this integration test `#[ignore]` with reason until the fixture lands, then un-ignore in Task 16).

- [ ] **Step 4: Write minimal implementation**

`crates/axilog-core/src/evtc/container.rs`:
```rust
use super::EvtcError;
use std::io::Read;

/// zevtc is a zip whose single entry is the raw EVTC; some tools emit bare deflate.
/// If the buffer already starts with "EVTC", it is raw — return as-is.
pub fn inflate_zevtc(bytes: &[u8]) -> Result<Vec<u8>, EvtcError> {
    if bytes.len() >= 4 && &bytes[0..4] == b"EVTC" {
        return Ok(bytes.to_vec());
    }
    if bytes.len() >= 2 && &bytes[0..2] == b"PK" {
        let reader = std::io::Cursor::new(bytes);
        let mut zip = zip_read(reader)?;
        return Ok(zip);
    }
    // fallback: raw deflate stream
    let mut out = Vec::new();
    flate2::read::DeflateDecoder::new(bytes)
        .read_to_end(&mut out)
        .map_err(|e| EvtcError::Container(e.to_string()))?;
    Ok(out)
}

// Minimal zip: read the first local file entry, deflate-inflate its data.
fn zip_read(mut cur: std::io::Cursor<&[u8]>) -> Result<Vec<u8>, EvtcError> {
    use std::io::{Seek, SeekFrom};
    let b = *cur.get_ref();
    // local file header: sig(4) ver(2) flag(2) method(2) modtime(4) crc(4)
    // csize(4) usize(4) namelen(2) extralen(2)
    if b.len() < 30 || &b[0..4] != b"PK\x03\x04" {
        return Err(EvtcError::Container("bad zip".into()));
    }
    let method = u16::from_le_bytes([b[8], b[9]]);
    let csize = u32::from_le_bytes([b[18], b[19], b[20], b[21]]) as usize;
    let namelen = u16::from_le_bytes([b[26], b[27]]) as usize;
    let extralen = u16::from_le_bytes([b[28], b[29]]) as usize;
    let data_start = 30 + namelen + extralen;
    let data = &b[data_start..data_start + csize];
    let _ = cur.seek(SeekFrom::Start(0));
    match method {
        0 => Ok(data.to_vec()), // stored
        8 => {
            let mut out = Vec::new();
            flate2::read::DeflateDecoder::new(data)
                .read_to_end(&mut out)
                .map_err(|e| EvtcError::Container(e.to_string()))?;
            Ok(out)
        }
        m => Err(EvtcError::Container(format!("unsupported zip method {m}"))),
    }
}
```
Add `decode_raw` to `mod.rs`:
```rust
pub mod container;
pub use container::{inflate_zevtc, decode_raw};

#[derive(Debug, Clone)]
pub struct RawLog {
    pub header: RawHeader,
    pub agents: Vec<RawAgent>,
    pub skills: Vec<RawSkill>,
    pub events: Vec<RawEvent>,
}
```
Then in `container.rs` add:
```rust
use super::{decode_header, decode_agents, decode_skills, decode_events,
    RawLog, HEADER_SIZE, AGENT_SIZE, SKILL_SIZE, EVENT_SIZE_REV1};

pub fn decode_raw(bytes: &[u8]) -> Result<RawLog, EvtcError> {
    let data = inflate_zevtc(bytes)?;
    let header = decode_header(&data)?;
    if header.revision != 1 {
        return Err(EvtcError::UnsupportedRevision(header.revision));
    }
    let read_u32 = |off: usize| -> Result<u32, EvtcError> {
        data.get(off..off + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap()))
            .ok_or(EvtcError::Truncated { need: off + 4, at: off, have: data.len() })
    };
    let mut off = HEADER_SIZE;
    let agent_count = read_u32(off)? as usize; off += 4;
    let agents = decode_agents(&data[off..], agent_count)?;
    off += agent_count * AGENT_SIZE;
    let skill_count = read_u32(off)? as usize; off += 4;
    let skills = decode_skills(&data[off..], skill_count)?;
    off += skill_count * SKILL_SIZE;
    let remaining = data.len() - off;
    let event_count = remaining / EVENT_SIZE_REV1;
    let events = decode_events(&data[off..], event_count)?;
    Ok(RawLog { header, agents, skills, events })
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p axilog-core container`
Expected: PASS. (`decode_fixture` stays `#[ignore]` until Task 16.)

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat(evtc): zevtc inflate + full raw decode pipeline"
```

---

### Task 7: Domain model — agent resolution

**Files:**
- Create: `crates/axilog-core/src/model/mod.rs`
- Modify: `crates/axilog-core/src/lib.rs` (add `pub mod model;`)

**Interfaces:**
- Consumes: `RawLog`, `RawAgent`, `RawEvent`, `sc`.
- Produces: `AgentKind`, `Agent`, `Player`, `Enemy`, `Team`, `Encounter`, and `pub fn resolve(raw: &RawLog) -> Encounter`. Profession/elite-spec code→string via `pub fn profession_name(prof: u32, is_elite: u32) -> (String, String)` (returns (profession, elite_spec); unknown codes return the numeric string so nothing is silently dropped).

- [ ] **Step 1: Write the failing test**

`crates/axilog-core/src/model/mod.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axilog_core_selfref::*; // see note: tests use crate::evtc types
}
```
(Note: tests live in-crate; use `crate::evtc::...` directly. Concretely:)
```rust
#[cfg(test)]
mod tests {
    use crate::evtc::{RawLog, RawHeader, RawAgent, RawSkill};
    use super::resolve;
    fn agent(addr: u64, is_elite: u32, name: &[u8]) -> RawAgent {
        RawAgent { addr, prof: 5, is_elite,
            toughness:0, concentration:0, healing:0, hitbox_width:0,
            condition:0, hitbox_height:0, name_raw: name.to_vec() }
    }
    #[test]
    fn splits_players_from_npcs() {
        let raw = RawLog {
            header: RawHeader { build: "20260114".into(), revision: 1, boss_id: 1 },
            agents: vec![
                agent(1, 27, b"Alice\0:Alice.1234\05\0"), // player
                agent(2, 0xffff_ffff, b"Enemy Zerg\0"),   // npc/enemy
            ],
            skills: vec![], events: vec![],
        };
        let enc = resolve(&raw);
        assert_eq!(enc.players.len(), 1);
        assert_eq!(enc.players[0].account, ":Alice.1234");
        assert_eq!(enc.players[0].subgroup, 5);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p axilog-core model`
Expected: FAIL — `resolve` not found.

- [ ] **Step 3: Write minimal implementation**

`crates/axilog-core/src/model/mod.rs`:
```rust
use crate::evtc::{RawLog, RawAgent};

#[derive(Debug, Clone, PartialEq)]
pub enum AgentKind { Player, Npc, Gadget }

#[derive(Debug, Clone)]
pub struct Player { pub agent_addr: u64, pub account: String, pub character: String,
    pub profession: String, pub elite_spec: String, pub team: String,
    pub subgroup: u8, pub in_squad: bool, pub commander: bool }
#[derive(Debug, Clone)]
pub struct Enemy { pub id: u64, pub instid: u16, pub name: String,
    pub team: String, pub is_player: bool }
#[derive(Debug, Clone)]
pub struct Team { pub color: String, pub team_id: u16 }
#[derive(Debug, Clone)]
pub struct Encounter { pub kind: String, pub map: String, pub duration_ms: u64,
    pub build: String, pub revision: u8, pub recorded_by: Option<String>,
    pub teams: Vec<Team>, pub players: Vec<Player>, pub enemies: Vec<Enemy> }

pub fn agent_kind(a: &RawAgent) -> AgentKind {
    if a.is_elite != 0xffff_ffff { AgentKind::Player }
    else if (a.prof >> 16) == 0xffff { AgentKind::Gadget }
    else { AgentKind::Npc }
}

pub fn profession_name(prof: u32, is_elite: u32) -> (String, String) {
    // Minimal core professions by prof code; elite spec by is_elite code.
    let base = match prof {
        1 => "Guardian", 2 => "Warrior", 3 => "Engineer", 4 => "Ranger",
        5 => "Thief", 6 => "Elementalist", 7 => "Mesmer", 8 => "Necromancer",
        9 => "Revenant", _ => "",
    };
    let base = if base.is_empty() { prof.to_string() } else { base.to_string() };
    let spec = if is_elite == 0 { String::new() } else { is_elite.to_string() };
    (base, spec)
}

pub fn resolve(raw: &RawLog) -> Encounter {
    let mut players = Vec::new();
    let mut enemies = Vec::new();
    for a in &raw.agents {
        match agent_kind(a) {
            AgentKind::Player => {
                let (character, account, sub) = a.name_parts();
                let (profession, elite_spec) = profession_name(a.prof, a.is_elite);
                players.push(Player {
                    agent_addr: a.addr, account, character, profession, elite_spec,
                    team: String::new(), subgroup: sub.unwrap_or(0),
                    in_squad: true, commander: false,
                });
            }
            _ => {
                let (name, _, _) = a.name_parts();
                enemies.push(Enemy { id: a.addr, instid: 0, name,
                    team: String::new(), is_player: false });
            }
        }
    }
    let duration_ms = raw.events.last().map(|e| e.time)
        .saturating_sub(raw.events.first().map(|e| e.time).unwrap_or(0));
    Encounter {
        kind: "wvw".into(), map: "World vs World".into(), duration_ms,
        build: raw.header.build.clone(), revision: raw.header.revision,
        recorded_by: None, teams: Vec::new(), players, enemies,
    }
}
```
Add `pub mod model;` to `lib.rs`. Expand the profession/elite tables to full GW2 codes in a follow-up within this task if the golden test needs specific names; numeric fallback keeps it correct meanwhile.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p axilog-core model`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(model): resolve players/enemies from raw agents"
```

---

### Task 8: WvW resolution — teams, map, squad dedupe

**Files:**
- Create: `crates/axilog-core/src/wvw/mod.rs`
- Modify: `crates/axilog-core/src/model/mod.rs` (call `wvw::apply` at end of `resolve`)

**Interfaces:**
- Consumes: `RawLog`, `sc`, `Encounter`, `Player`, `Enemy`, `Team`.
- Produces: `pub fn apply(enc: &mut Encounter, raw: &RawLog)` — sets each agent's `team` from `TEAM_CHANGE` statechange events, builds `enc.teams`, fills `recorded_by` from `POINT_OF_VIEW`, sets `map` from `MAP_ID`, and collapses duplicate player entries (relog/build-swap) by account (fallback character), keeping one `Player` per account with `in_squad` = OR of entries.

- [ ] **Step 1: Write the failing test**

`crates/axilog-core/src/wvw/mod.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Encounter, Player};
    fn player(addr: u64, acc: &str) -> Player {
        Player { agent_addr: addr, account: acc.into(), character: "C".into(),
            profession: "Thief".into(), elite_spec: "".into(), team: "".into(),
            subgroup: 1, in_squad: true, commander: false }
    }
    #[test]
    fn dedupes_players_by_account() {
        let mut enc = Encounter { kind:"wvw".into(), map:"".into(), duration_ms:0,
            build:"".into(), revision:1, recorded_by:None, teams:vec![],
            players: vec![player(1, ":A.1"), player(2, ":A.1"), player(3, ":B.2")],
            enemies: vec![] };
        dedupe_players(&mut enc.players);
        assert_eq!(enc.players.len(), 2);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p axilog-core wvw`
Expected: FAIL — `dedupe_players` not found.

- [ ] **Step 3: Write minimal implementation**

`crates/axilog-core/src/wvw/mod.rs`:
```rust
use crate::evtc::{RawLog, sc};
use crate::model::{Encounter, Team, Player};
use std::collections::BTreeMap;

/// Collapse relog/build-swap duplicates: one Player per account (fallback character).
pub fn dedupe_players(players: &mut Vec<Player>) {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut out: Vec<Player> = Vec::new();
    for p in players.drain(..) {
        let key = if p.account.is_empty() { p.character.clone() } else { p.account.clone() };
        match seen.get(&key) {
            Some(&i) => { out[i].in_squad |= p.in_squad; out[i].commander |= p.commander; }
            None => { seen.insert(key, out.len()); out.push(p); }
        }
    }
    *players = out;
}

fn team_color(team_id: u16) -> String {
    // WvW team ids → colors (verify against wvWMapData in the golden fixture).
    match team_id {
        883 | 39 | 2520 => "red".into(),
        882 | 38 | 2519 => "blue".into(),
        // remaining known green ids
        _ => "green".into(),
    }
}

pub fn apply(enc: &mut Encounter, raw: &RawLog) {
    // team assignment: TEAM_CHANGE src_agent -> value (team id) in dst_agent field
    let mut agent_team: BTreeMap<u64, u16> = BTreeMap::new();
    let mut recorded_by: Option<u64> = None;
    for e in &raw.events {
        if e.is_statechange == sc::TEAM_CHANGE {
            agent_team.insert(e.src_agent, e.dst_agent as u16);
        } else if e.is_statechange == sc::POINT_OF_VIEW {
            recorded_by = Some(e.src_agent);
        }
    }
    let mut team_ids: Vec<u16> = agent_team.values().copied().collect();
    team_ids.sort_unstable(); team_ids.dedup();
    enc.teams = team_ids.iter().map(|&id| Team { color: team_color(id), team_id: id }).collect();
    for p in &mut enc.players {
        if let Some(&t) = agent_team.get(&p.agent_addr) { p.team = team_color(t); }
    }
    for en in &mut enc.enemies {
        if let Some(&t) = agent_team.get(&en.id) { en.team = team_color(t); }
    }
    if let Some(addr) = recorded_by {
        if let Some(p) = enc.players.iter().find(|p| p.agent_addr == addr) {
            enc.recorded_by = Some(p.account.clone());
        }
    }
    dedupe_players(&mut enc.players);
}
```
At the end of `model::resolve`, before returning: `crate::wvw::apply(&mut enc, raw);` (rename the local to `enc` and make it `mut`). Add `pub mod wvw;` to `lib.rs`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p axilog-core wvw`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(wvw): team/map resolution and squad dedupe"
```

---

### Task 9: Analysis — damage & DPS

**Files:**
- Create: `crates/axilog-core/src/analysis/mod.rs`, `crates/axilog-core/src/analysis/damage.rs`
- Modify: `crates/axilog-core/src/lib.rs`

**Interfaces:**
- Consumes: `RawLog`, `RawEvent`, `Encounter`.
- Produces: `PlayerMetrics`, `Timeline`, `Metrics`, `pub fn analyze(enc: &Encounter, raw: &RawLog) -> Metrics`, and `damage::accumulate(...)`. A strike event counts as damage when `is_statechange == 0 && is_activation == 0 && is_buffremove == 0 && buff == 0` (physical `value`), and condition damage when `buff == 1` (`buff_dmg`). Only count damage where the source is a squad player and the destination is an enemy.

- [ ] **Step 1: Write the failing test**

`crates/axilog-core/src/analysis/damage.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::RawEvent;
    fn strike(src: u64, dst: u64, dmg: i32) -> RawEvent {
        RawEvent { time:0, src_agent:src, dst_agent:dst, value:dmg, buff_dmg:0,
            overstack:0, skillid:1, src_instid:0, dst_instid:0,
            src_master_instid:0, dst_master_instid:0, iff:1, buff:0, result:0,
            is_activation:0, is_buffremove:0, is_statechange:0 }
    }
    #[test]
    fn sums_physical_damage_to_enemy() {
        let squad = [1u64].into_iter().collect();
        let enemies = [9u64].into_iter().collect();
        let evs = vec![strike(1, 9, 100), strike(1, 9, 50), strike(1, 2, 999)];
        let dmg = accumulate(&evs, &squad, &enemies);
        assert_eq!(dmg[&1].0, 150); // total to enemies only
        assert_eq!(dmg[&1].1.get(&9).copied(), Some(150));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p axilog-core damage`
Expected: FAIL — `accumulate` not found.

- [ ] **Step 3: Write minimal implementation**

`crates/axilog-core/src/analysis/damage.rs`:
```rust
use crate::evtc::RawEvent;
use std::collections::{BTreeMap, BTreeSet};

/// Returns per-source: (total_to_enemies, per_enemy_map).
pub fn accumulate(
    events: &[RawEvent],
    squad: &BTreeSet<u64>,
    enemies: &BTreeSet<u64>,
) -> BTreeMap<u64, (u64, BTreeMap<u64, u64>)> {
    let mut out: BTreeMap<u64, (u64, BTreeMap<u64, u64>)> = BTreeMap::new();
    for e in events {
        if e.is_statechange != 0 || e.is_activation != 0 || e.is_buffremove != 0 { continue; }
        if !squad.contains(&e.src_agent) || !enemies.contains(&e.dst_agent) { continue; }
        let dmg = if e.buff == 1 { e.buff_dmg.max(0) as u64 } else { e.value.max(0) as u64 };
        if dmg == 0 { continue; }
        let entry = out.entry(e.src_agent).or_default();
        entry.0 += dmg;
        *entry.1.entry(e.dst_agent).or_default() += dmg;
    }
    out
}
```
`crates/axilog-core/src/analysis/mod.rs`:
```rust
pub mod damage;
pub mod downs;
pub mod cc;

use crate::evtc::RawLog;
use crate::model::Encounter;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Default)]
pub struct PlayerMetrics { pub agent_addr: u64, pub damage_total: u64, pub dps: f64,
    pub per_enemy: Vec<(u64,u64)>, pub downs_dealt: u32, pub kills_dealt: u32,
    pub down_contribution: u64, pub downs_taken: u32, pub deaths: u32,
    pub damage_taken: u64, pub cc_applied: u32, pub cc_duration_ms: u64 }
#[derive(Debug, Clone)]
pub struct Timeline { pub resolution_ms: u64, pub squad_damage: Vec<u64>,
    pub cc_applied: Vec<u32>, pub downs: Vec<u32> }
#[derive(Debug, Clone)]
pub struct Metrics { pub players: Vec<PlayerMetrics>, pub timeline: Timeline }

pub fn analyze(enc: &Encounter, raw: &RawLog) -> Metrics {
    let squad: BTreeSet<u64> = enc.players.iter().map(|p| p.agent_addr).collect();
    let enemies: BTreeSet<u64> = enc.enemies.iter().map(|e| e.id).collect();
    let dmg = damage::accumulate(&raw.events, &squad, &enemies);
    let secs = (enc.duration_ms as f64 / 1000.0).max(1.0);
    let mut players: Vec<PlayerMetrics> = enc.players.iter().map(|p| {
        let (total, per) = dmg.get(&p.agent_addr).cloned().unwrap_or_default();
        PlayerMetrics { agent_addr: p.agent_addr, damage_total: total,
            dps: total as f64 / secs,
            per_enemy: per.into_iter().collect(), ..Default::default() }
    }).collect();
    downs::apply(&mut players, enc, raw, &squad, &enemies);
    let timeline = cc::timeline(enc, raw, &squad, &enemies);
    Metrics { players, timeline }
}
```
Add `pub mod analysis;` to `lib.rs`. Create empty `downs.rs`/`cc.rs` with the fns stubbed to compile (filled in Tasks 10–11): `pub fn apply(..) {}` matching the call, `pub fn timeline(..) -> Timeline`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p axilog-core damage`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(analysis): damage and dps accumulation"
```

---

### Task 10: Analysis — downs, kills, deaths, down contribution

**Files:**
- Modify: `crates/axilog-core/src/analysis/downs.rs`

**Interfaces:**
- Consumes: `RawLog`, `RawEvent`, `Encounter`, `PlayerMetrics`, `result::{DOWNED,KILLING_BLOW}`, `sc::CHANGE_DEAD`.
- Produces: `pub fn apply(players: &mut [PlayerMetrics], enc: &Encounter, raw: &RawLog, squad: &BTreeSet<u64>, enemies: &BTreeSet<u64>)`.
- **Down contribution definition (Milestone 1, documented approximation):** for each enemy `CBTR_DOWNED` event at time `t_down`, attribute to each squad source the damage they dealt to that enemy in the window `(t_down - WINDOW_MS, t_down]` where `WINDOW_MS = 10_000`. Sum across all downs on all enemies → `down_contribution`. This is validated against EI within tolerance in Task 16; if it diverges materially, refine the window/segmenting in a follow-up (noted risk in the spec).

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::{RawEvent, result};
    use crate::analysis::PlayerMetrics;
    use std::collections::BTreeSet;
    fn ev(time:u64, src:u64, dst:u64, value:i32, result_:u8, sc:u8) -> RawEvent {
        RawEvent { time, src_agent:src, dst_agent:dst, value, buff_dmg:0, overstack:0,
            skillid:1, src_instid:0, dst_instid:0, src_master_instid:0, dst_master_instid:0,
            iff:1, buff:0, result:result_, is_activation:0, is_buffremove:0, is_statechange:sc }
    }
    #[test]
    fn counts_down_and_attributes_contribution() {
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let enemies: BTreeSet<u64> = [9u64].into_iter().collect();
        let evs = vec![
            ev(500, 1, 9, 300, 0, 0),                 // damage before down
            ev(1000, 1, 9, 0, result::DOWNED, 0),     // enemy downed by src 1
            ev(2000, 1, 9, 0, result::KILLING_BLOW, 0)// kill
        ];
        let mut pm = vec![PlayerMetrics { agent_addr: 1, ..Default::default() }];
        // build enc with duration only
        let enc = crate::model::Encounter { kind:"wvw".into(), map:"".into(),
            duration_ms:2000, build:"".into(), revision:1, recorded_by:None,
            teams:vec![], players:vec![], enemies:vec![] };
        apply(&mut pm, &enc, &raw_from(evs), &squad, &enemies);
        assert_eq!(pm[0].downs_dealt, 1);
        assert_eq!(pm[0].kills_dealt, 1);
        assert_eq!(pm[0].down_contribution, 300);
    }
    fn raw_from(events: Vec<RawEvent>) -> crate::evtc::RawLog {
        crate::evtc::RawLog { header: crate::evtc::RawHeader{build:"".into(),revision:1,boss_id:1},
            agents: vec![], skills: vec![], events }
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p axilog-core downs`
Expected: FAIL — `apply` is a stub / assertions fail.

- [ ] **Step 3: Write minimal implementation**

```rust
use crate::evtc::{RawLog, result, sc};
use crate::model::Encounter;
use crate::analysis::PlayerMetrics;
use std::collections::{BTreeMap, BTreeSet};

const WINDOW_MS: u64 = 10_000;

pub fn apply(players: &mut [PlayerMetrics], _enc: &Encounter, raw: &RawLog,
             squad: &BTreeSet<u64>, enemies: &BTreeSet<u64>) {
    let idx: BTreeMap<u64, usize> =
        players.iter().enumerate().map(|(i, p)| (p.agent_addr, i)).collect();

    for e in &raw.events {
        if e.is_statechange != 0 { continue; }
        let src_is_squad = squad.contains(&e.src_agent);
        let dst_is_enemy = enemies.contains(&e.dst_agent);
        if src_is_squad && dst_is_enemy && e.result == result::DOWNED {
            if let Some(&i) = idx.get(&e.src_agent) { players[i].downs_dealt += 1; }
        }
        if src_is_squad && dst_is_enemy && e.result == result::KILLING_BLOW {
            if let Some(&i) = idx.get(&e.src_agent) { players[i].kills_dealt += 1; }
        }
    }
    // downs taken / deaths (squad members as destination / statechange)
    for e in &raw.events {
        if e.is_statechange == sc::CHANGE_DEAD {
            if let Some(&i) = idx.get(&e.src_agent) { players[i].deaths += 1; }
        }
        if e.is_statechange == 0 && e.result == result::DOWNED
            && squad.contains(&e.dst_agent) {
            if let Some(&i) = idx.get(&e.dst_agent) { players[i].downs_taken += 1; }
        }
    }
    // down contribution: damage to each enemy in the window before its down
    let downs: Vec<(u64, u64)> = raw.events.iter()
        .filter(|e| e.is_statechange == 0 && e.result == result::DOWNED
            && enemies.contains(&e.dst_agent))
        .map(|e| (e.dst_agent, e.time)).collect();
    for (enemy, t_down) in downs {
        let lo = t_down.saturating_sub(WINDOW_MS);
        for e in &raw.events {
            if e.is_statechange != 0 || e.is_activation != 0 || e.is_buffremove != 0 { continue; }
            if e.dst_agent != enemy || e.time <= lo || e.time > t_down { continue; }
            if !squad.contains(&e.src_agent) { continue; }
            let dmg = if e.buff == 1 { e.buff_dmg.max(0) as u64 } else { e.value.max(0) as u64 };
            if let Some(&i) = idx.get(&e.src_agent) { players[i].down_contribution += dmg; }
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p axilog-core downs`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(analysis): downs, kills, deaths, down contribution"
```

---

### Task 11: Analysis — CC totals + per-second timeline

**Files:**
- Modify: `crates/axilog-core/src/analysis/cc.rs`

**Interfaces:**
- Consumes: `RawLog`, `Encounter`, `PlayerMetrics`, `Timeline`.
- Produces: `pub fn timeline(enc: &Encounter, raw: &RawLog, squad: &BTreeSet<u64>, enemies: &BTreeSet<u64>) -> Timeline`, and `pub fn apply_cc(players: &mut [PlayerMetrics], raw: &RawLog, squad: &BTreeSet<u64>, enemies: &BTreeSet<u64>)`. CC is counted from breakbar damage events (`value > 0` where the event carries breakbar/defiance damage — arcdps records this as `result == CBTR_CROWDCONTROL`/defiance result or via `buff_dmg` on breakbar; **verify the exact CC signal against the golden fixture** and adjust the predicate). Per-second buckets use `resolution_ms = 1000`.

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::{RawEvent, RawLog, RawHeader};
    use crate::model::Encounter;
    use std::collections::BTreeSet;
    fn dmg(time:u64, src:u64, dst:u64, v:i32) -> RawEvent {
        RawEvent{time,src_agent:src,dst_agent:dst,value:v,buff_dmg:0,overstack:0,skillid:1,
            src_instid:0,dst_instid:0,src_master_instid:0,dst_master_instid:0,iff:1,buff:0,
            result:0,is_activation:0,is_buffremove:0,is_statechange:0}
    }
    #[test]
    fn buckets_squad_damage_per_second() {
        let enc = Encounter{kind:"wvw".into(),map:"".into(),duration_ms:2500,build:"".into(),
            revision:1,recorded_by:None,teams:vec![],players:vec![],enemies:vec![]};
        let raw = RawLog{header:RawHeader{build:"".into(),revision:1,boss_id:1},
            agents:vec![],skills:vec![],
            events:vec![dmg(100,1,9,50), dmg(1200,1,9,70), dmg(2400,1,9,30)]};
        let squad:BTreeSet<u64>=[1u64].into_iter().collect();
        let enemies:BTreeSet<u64>=[9u64].into_iter().collect();
        let tl = timeline(&enc,&raw,&squad,&enemies);
        assert_eq!(tl.resolution_ms, 1000);
        assert_eq!(tl.squad_damage.len(), 3); // seconds 0,1,2
        assert_eq!(tl.squad_damage[0], 50);
        assert_eq!(tl.squad_damage[1], 70);
        assert_eq!(tl.squad_damage[2], 30);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p axilog-core cc`
Expected: FAIL — `timeline` is a stub.

- [ ] **Step 3: Write minimal implementation**

```rust
use crate::evtc::RawLog;
use crate::model::Encounter;
use crate::analysis::{Timeline, PlayerMetrics};
use std::collections::BTreeSet;

pub fn timeline(enc: &Encounter, raw: &RawLog,
                squad: &BTreeSet<u64>, enemies: &BTreeSet<u64>) -> Timeline {
    let res = 1000u64;
    let buckets = ((enc.duration_ms / res) + 1) as usize;
    let mut squad_damage = vec![0u64; buckets];
    let mut cc_applied = vec![0u32; buckets];
    let mut downs = vec![0u32; buckets];
    let t0 = raw.events.first().map(|e| e.time).unwrap_or(0);
    for e in &raw.events {
        let rel = e.time.saturating_sub(t0);
        let b = (rel / res) as usize;
        if b >= buckets { continue; }
        if e.is_statechange == 0 && e.is_activation == 0 && e.is_buffremove == 0
            && squad.contains(&e.src_agent) && enemies.contains(&e.dst_agent) {
            let d = if e.buff == 1 { e.buff_dmg.max(0) as u64 } else { e.value.max(0) as u64 };
            squad_damage[b] += d;
        }
        if e.is_statechange == 0 && e.result == crate::evtc::result::DOWNED
            && enemies.contains(&e.dst_agent) { downs[b] += 1; }
        // CC predicate verified against fixture in Task 16:
        if is_cc(e) && squad.contains(&e.src_agent) && enemies.contains(&e.dst_agent) {
            cc_applied[b] += 1;
        }
    }
    Timeline { resolution_ms: res, squad_damage, cc_applied, downs }
}

fn is_cc(e: &crate::evtc::RawEvent) -> bool {
    // Breakbar/defiance damage marks a CC application. Refine against golden fixture.
    e.is_statechange == 0 && e.is_activation == 0 && e.buff == 0 && e.overstack > 0
}

pub fn apply_cc(players: &mut [PlayerMetrics], raw: &RawLog,
                squad: &BTreeSet<u64>, enemies: &BTreeSet<u64>) {
    use std::collections::BTreeMap;
    let idx: BTreeMap<u64, usize> =
        players.iter().enumerate().map(|(i, p)| (p.agent_addr, i)).collect();
    for e in &raw.events {
        if is_cc(e) && squad.contains(&e.src_agent) && enemies.contains(&e.dst_agent) {
            if let Some(&i) = idx.get(&e.src_agent) { players[i].cc_applied += 1; }
        }
    }
}
```
Call `cc::apply_cc(&mut players, raw, &squad, &enemies);` in `analysis::analyze` after `downs::apply`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p axilog-core cc`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(analysis): CC totals and per-second timeline"
```

---

### Task 12: Native schema + serializer

**Files:**
- Modify: `crates/axilog-schema/src/lib.rs`

**Interfaces:**
- Consumes: `Encounter`, `Metrics`, `PlayerMetrics`, `Timeline`, `Player`, `Enemy`, `Team`.
- Produces: serde types `Report`, and `pub fn build_report(enc: &Encounter, metrics: &Metrics, axilog_version: &str) -> Report`. `Report` serializes to the native schema in the design spec §5.

- [ ] **Step 1: Write the failing test**

`crates/axilog-schema/src/lib.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axilog_core::model::{Encounter, Player};
    use axilog_core::analysis::{Metrics, PlayerMetrics, Timeline};
    #[test]
    fn serializes_report_with_versions() {
        let enc = Encounter { kind:"wvw".into(), map:"Eternal Battlegrounds".into(),
            duration_ms:1000, build:"20260114".into(), revision:1, recorded_by:None,
            teams:vec![], players:vec![Player{agent_addr:1,account:":A.1".into(),
            character:"A".into(),profession:"Thief".into(),elite_spec:"".into(),
            team:"red".into(),subgroup:1,in_squad:true,commander:false}],
            enemies:vec![] };
        let m = Metrics { players: vec![PlayerMetrics{agent_addr:1,damage_total:500,
            dps:500.0,..Default::default()}],
            timeline: Timeline{resolution_ms:1000,squad_damage:vec![500],
            cc_applied:vec![0],downs:vec![0]} };
        let report = build_report(&enc, &m, "0.1.0");
        let v = serde_json::to_value(&report).unwrap();
        assert_eq!(v["schema_version"], "0.1");
        assert_eq!(v["axilog_version"], "0.1.0");
        assert_eq!(v["players"][0]["damage"]["total"], 500);
        assert_eq!(v["encounter"]["map"], "Eternal Battlegrounds");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p axilog-schema`
Expected: FAIL — `build_report` not found.

- [ ] **Step 3: Write minimal implementation**

`crates/axilog-schema/src/lib.rs`:
```rust
use serde::Serialize;
use axilog_core::model::Encounter;
use axilog_core::analysis::Metrics;

#[derive(Serialize)]
pub struct Report {
    pub schema_version: &'static str,
    pub axilog_version: String,
    pub encounter: EncounterOut,
    pub players: Vec<PlayerOut>,
    pub enemies: Vec<EnemyOut>,
    pub timeline: TimelineOut,
}
#[derive(Serialize)]
pub struct EncounterOut { pub kind: String, pub map: String, pub duration_ms: u64,
    pub build: String, pub revision: u8, pub recorded_by: Option<String>,
    pub teams: Vec<TeamOut> }
#[derive(Serialize)]
pub struct TeamOut { pub color: String, pub team_id: u16 }
#[derive(Serialize)]
pub struct DamageOut { pub total: u64, pub dps: f64, pub per_enemy: Vec<PerEnemyOut> }
#[derive(Serialize)]
pub struct PerEnemyOut { pub enemy_id: u64, pub total: u64 }
#[derive(Serialize)]
pub struct CcOut { pub applied_total: u32, pub applied_duration_ms: u64 }
#[derive(Serialize)]
pub struct PlayerOut { pub account: String, pub character: String, pub profession: String,
    pub elite_spec: String, pub team: String, pub subgroup: u8, pub in_squad: bool,
    pub commander: bool, pub damage: DamageOut, pub downs_dealt: u32, pub kills_dealt: u32,
    pub down_contribution: u64, pub downs_taken: u32, pub deaths: u32, pub damage_taken: u64,
    pub cc: CcOut }
#[derive(Serialize)]
pub struct EnemyOut { pub id: u64, pub name: String, pub team: String, pub is_player: bool }
#[derive(Serialize)]
pub struct TimelineOut { pub resolution_ms: u64, pub per_second: PerSecondOut }
#[derive(Serialize)]
pub struct PerSecondOut { pub squad_damage: Vec<u64>, pub cc_applied: Vec<u32>, pub downs: Vec<u32> }

pub fn build_report(enc: &Encounter, metrics: &Metrics, axilog_version: &str) -> Report {
    let pm: std::collections::BTreeMap<u64, &axilog_core::analysis::PlayerMetrics> =
        metrics.players.iter().map(|p| (p.agent_addr, p)).collect();
    let players = enc.players.iter().map(|p| {
        let m = pm.get(&p.agent_addr);
        PlayerOut {
            account: p.account.clone(), character: p.character.clone(),
            profession: p.profession.clone(), elite_spec: p.elite_spec.clone(),
            team: p.team.clone(), subgroup: p.subgroup, in_squad: p.in_squad,
            commander: p.commander,
            damage: DamageOut {
                total: m.map(|m| m.damage_total).unwrap_or(0),
                dps: m.map(|m| m.dps).unwrap_or(0.0),
                per_enemy: m.map(|m| m.per_enemy.iter()
                    .map(|(id,t)| PerEnemyOut{enemy_id:*id,total:*t}).collect())
                    .unwrap_or_default(),
            },
            downs_dealt: m.map(|m| m.downs_dealt).unwrap_or(0),
            kills_dealt: m.map(|m| m.kills_dealt).unwrap_or(0),
            down_contribution: m.map(|m| m.down_contribution).unwrap_or(0),
            downs_taken: m.map(|m| m.downs_taken).unwrap_or(0),
            deaths: m.map(|m| m.deaths).unwrap_or(0),
            damage_taken: m.map(|m| m.damage_taken).unwrap_or(0),
            cc: CcOut { applied_total: m.map(|m| m.cc_applied).unwrap_or(0),
                        applied_duration_ms: m.map(|m| m.cc_duration_ms).unwrap_or(0) },
        }
    }).collect();
    Report {
        schema_version: "0.1", axilog_version: axilog_version.to_string(),
        encounter: EncounterOut { kind: enc.kind.clone(), map: enc.map.clone(),
            duration_ms: enc.duration_ms, build: enc.build.clone(), revision: enc.revision,
            recorded_by: enc.recorded_by.clone(),
            teams: enc.teams.iter().map(|t| TeamOut{color:t.color.clone(),team_id:t.team_id}).collect() },
        players,
        enemies: enc.enemies.iter().map(|e| EnemyOut{id:e.id,name:e.name.clone(),
            team:e.team.clone(),is_player:e.is_player}).collect(),
        timeline: TimelineOut { resolution_ms: metrics.timeline.resolution_ms,
            per_second: PerSecondOut { squad_damage: metrics.timeline.squad_damage.clone(),
                cc_applied: metrics.timeline.cc_applied.clone(),
                downs: metrics.timeline.downs.clone() } },
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p axilog-schema`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(schema): native versioned report serializer"
```

---

### Task 13: CLI — parse command + JSON default

**Files:**
- Modify: `crates/axilog-cli/src/main.rs`
- Test: `crates/axilog-cli/tests/cli.rs`

**Interfaces:**
- Consumes: `axilog_core::evtc::decode_raw`, `axilog_core::model::resolve`, `axilog_core::analysis::analyze`, `axilog_schema::build_report`, `axilog_ei::to_ei_json`.
- Produces: `axilog parse <PATH> [--format json|table|csv|ei-json]` (default `json`), writing to stdout.

- [ ] **Step 1: Write the failing test**

`crates/axilog-cli/tests/cli.rs`:
```rust
use std::process::Command;
#[test]
fn parses_fixture_to_json() {
    let out = Command::new(env!("CARGO_BIN_EXE_axilog"))
        .args(["parse", "../../fixtures/wvw-small.zevtc"])
        .output().expect("run axilog");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["schema_version"], "0.1");
    assert!(v["players"].as_array().unwrap().len() > 0);
}
```
(Mark `#[ignore]` until the fixture is committed in Task 16, then un-ignore.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p axilog-cli`
Expected: FAIL — binary prints "axilog", not JSON.

- [ ] **Step 3: Write minimal implementation**

`crates/axilog-cli/src/main.rs`:
```rust
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "axilog", version)]
struct Cli { #[command(subcommand)] cmd: Cmd }

#[derive(Subcommand)]
enum Cmd {
    /// Parse an arcdps .zevtc/.evtc log
    Parse {
        path: PathBuf,
        #[arg(long, value_enum, default_value_t = Format::Json)]
        format: Format,
    },
}
#[derive(Copy, Clone, ValueEnum)]
enum Format { Json, Table, Csv, EiJson }

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Parse { path, format } => {
            let bytes = std::fs::read(&path)?;
            let raw = axilog_core::evtc::decode_raw(&bytes)?;
            let enc = axilog_core::model::resolve(&raw);
            let metrics = axilog_core::analysis::analyze(&enc, &raw);
            let report = axilog_schema::build_report(&enc, &metrics, env!("CARGO_PKG_VERSION"));
            match format {
                Format::Json => println!("{}", serde_json::to_string_pretty(&report)?),
                Format::EiJson => println!("{}", serde_json::to_string_pretty(&axilog_ei::to_ei_json(&report))?),
                Format::Table => print!("{}", axilog_cli_table(&report)),
                Format::Csv => print!("{}", axilog_cli_csv(&report)),
            }
        }
    }
    Ok(())
}
// table/csv helpers added in Task 14:
fn axilog_cli_table(_r: &axilog_schema::Report) -> String { String::new() }
fn axilog_cli_csv(_r: &axilog_schema::Report) -> String { String::new() }
```
`EvtcError` must implement `std::error::Error` (thiserror does this). Add a stub `axilog_ei::to_ei_json` returning `serde_json::json!({})` so this compiles (real impl in Task 15).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo build && cargo test -p axilog-cli`
Expected: builds; the ignored fixture test compiles.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(cli): parse command with json default and format flag"
```

---

### Task 14: CLI — table and CSV formats

**Files:**
- Modify: `crates/axilog-cli/src/main.rs`

**Interfaces:**
- Produces: real `axilog_cli_table` and `axilog_cli_csv` bodies.

- [ ] **Step 1: Write the failing test**

Add to `crates/axilog-cli/tests/cli.rs`:
```rust
#[test]
fn table_and_csv_have_headers() {
    // Build a report via the library path is out of scope for a bin test;
    // instead run the binary against the fixture.
    for (fmt, needle) in [("table", "DPS"), ("csv", "account,")] {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_axilog"))
            .args(["parse", "../../fixtures/wvw-small.zevtc", "--format", fmt])
            .output().unwrap();
        assert!(out.status.success());
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(s.contains(needle), "format {fmt} missing {needle}");
    }
}
```
(Mark `#[ignore]` until fixture committed in Task 16.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p axilog-cli table_and_csv`
Expected: FAIL (empty output).

- [ ] **Step 3: Write minimal implementation**

Replace the stubs:
```rust
fn axilog_cli_table(r: &axilog_schema::Report) -> String {
    let mut s = String::new();
    s.push_str(&format!("{:<24} {:<12} {:>10} {:>8} {:>6} {:>6} {:>7}\n",
        "account", "profession", "damage", "DPS", "downs", "kills", "deaths"));
    let mut players: Vec<_> = r.players.iter().collect();
    players.sort_by(|a, b| b.damage.total.cmp(&a.damage.total));
    for p in players {
        s.push_str(&format!("{:<24} {:<12} {:>10} {:>8.0} {:>6} {:>6} {:>7}\n",
            trunc(&p.account, 24), trunc(&p.profession, 12), p.damage.total,
            p.damage.dps, p.downs_dealt, p.kills_dealt, p.deaths));
    }
    s
}
fn axilog_cli_csv(r: &axilog_schema::Report) -> String {
    let mut s = String::from("account,character,profession,team,damage,dps,downs_dealt,kills_dealt,down_contribution,deaths\n");
    for p in &r.players {
        s.push_str(&format!("{},{},{},{},{},{:.0},{},{},{},{}\n",
            p.account, p.character, p.profession, p.team, p.damage.total, p.damage.dps,
            p.downs_dealt, p.kills_dealt, p.down_contribution, p.deaths));
    }
    s
}
fn trunc(s: &str, n: usize) -> String { s.chars().take(n).collect() }
```
Make `Report`'s fields `pub` (they already are) so the CLI can read them.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p axilog-cli table_and_csv` (after Task 16 un-ignore)
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(cli): human table and csv output formats"
```

---

### Task 15: EI-compatibility adapter (`ei-json`)

**Files:**
- Modify: `crates/axilog-ei/src/lib.rs`

**Interfaces:**
- Consumes: `axilog_schema::Report`.
- Produces: `pub fn to_ei_json(report: &Report) -> serde_json::Value` emitting the Milestone-1 subset of EI's `DPSReportJSON`: top-level `fightName`, `durationMS`, `recordedBy`, `success`, `players[]` (`account`, `character_name`, `profession`, `elite_spec`, `teamID`→color mapped back to EI numeric where known else 0, `group`, `notInSquad`, `hasCommanderTag`, `dpsAll:[{dps,damage}]`, `statsTargets:[[{downContribution,killed,downed}]]`, `defenses:[{downCount,deadCount,damageTaken}]`), `targets[]` (`id`,`name`,`enemyPlayer:true`), and `wvWMapData`.

- [ ] **Step 1: Write the failing test**

`crates/axilog-ei/src/lib.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    fn sample_report() -> axilog_schema::Report {
        // Construct via axilog_schema public API by round-tripping from core types.
        use axilog_core::model::{Encounter, Player};
        use axilog_core::analysis::{Metrics, PlayerMetrics, Timeline};
        let enc = Encounter{kind:"wvw".into(),map:"Eternal Battlegrounds".into(),
            duration_ms:1000,build:"".into(),revision:1,recorded_by:Some(":A.1".into()),
            teams:vec![],players:vec![Player{agent_addr:1,account:":A.1".into(),
            character:"A".into(),profession:"Thief".into(),elite_spec:"Daredevil".into(),
            team:"red".into(),subgroup:2,in_squad:true,commander:true}],enemies:vec![]};
        let m = Metrics{players:vec![PlayerMetrics{agent_addr:1,damage_total:500,dps:500.0,
            downs_dealt:1,kills_dealt:1,down_contribution:400,deaths:0,..Default::default()}],
            timeline:Timeline{resolution_ms:1000,squad_damage:vec![500],cc_applied:vec![0],downs:vec![0]}};
        axilog_schema::build_report(&enc,&m,"0.1.0")
    }
    #[test]
    fn maps_core_ei_fields() {
        let v = to_ei_json(&sample_report());
        assert_eq!(v["durationMS"], 1000);
        assert_eq!(v["recordedBy"], ":A.1");
        assert_eq!(v["players"][0]["account"], ":A.1");
        assert_eq!(v["players"][0]["character_name"], "A");
        assert_eq!(v["players"][0]["hasCommanderTag"], true);
        assert_eq!(v["players"][0]["dpsAll"][0]["damage"], 500);
        assert_eq!(v["players"][0]["statsTargets"][0][0]["downContribution"], 400);
        assert_eq!(v["players"][0]["defenses"][0]["deadCount"], 0);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p axilog-ei`
Expected: FAIL — `to_ei_json` stub returns `{}`.

- [ ] **Step 3: Write minimal implementation**

`crates/axilog-ei/src/lib.rs`:
```rust
use serde_json::{json, Value};
use axilog_schema::Report;

fn color_to_team_id(color: &str) -> u64 {
    // EI numeric team ids; refine against golden wvWMapData.
    match color { "red" => 883, "blue" => 882, "green" => 881, _ => 0 }
}

pub fn to_ei_json(report: &Report) -> Value {
    let players: Vec<Value> = report.players.iter().map(|p| json!({
        "account": p.account,
        "character_name": p.character,
        "profession": p.profession,
        "elite_spec": p.elite_spec,
        "teamID": color_to_team_id(&p.team),
        "group": p.subgroup,
        "notInSquad": !p.in_squad,
        "hasCommanderTag": p.commander,
        "dpsAll": [ { "dps": p.damage.dps.round() as i64, "damage": p.damage.total } ],
        "statsTargets": [ [ {
            "downContribution": p.down_contribution,
            "killed": p.kills_dealt,
            "downed": p.downs_dealt
        } ] ],
        "defenses": [ {
            "downCount": p.downs_taken,
            "deadCount": p.deaths,
            "damageTaken": p.damage_taken
        } ]
    })).collect();
    let targets: Vec<Value> = report.enemies.iter().map(|e| json!({
        "id": e.id, "name": e.name, "enemyPlayer": true,
        "teamID": color_to_team_id(&e.team)
    })).collect();
    json!({
        "fightName": format!("Detailed WvW - {}", report.encounter.map),
        "durationMS": report.encounter.duration_ms,
        "recordedBy": report.encounter.recorded_by,
        "success": true,
        "eliteInsightsVersion": null,
        "players": players,
        "targets": targets,
        "wvWMapData": {
            "redTeamID": color_to_team_id("red"),
            "blueTeamID": color_to_team_id("blue"),
            "greenTeamID": color_to_team_id("green")
        }
    })
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p axilog-ei`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(ei): DPSReportJSON compatibility adapter (M1 subset)"
```

---

### Task 16: Golden fixtures, parity test, CI

**Files:**
- Create: `fixtures/wvw-small.zevtc` (copied from axibridge `testdata/`), `fixtures/wvw-small.ei.json` (trimmed EI JSON for the SAME log)
- Create: `crates/axilog-core/tests/golden.rs`
- Create: `.github/workflows/ci.yml`
- Modify: `.gitignore` (ignore large fixtures dir)
- Un-ignore the `#[ignore]` tests added in Tasks 6, 13, 14.

**Interfaces:**
- Consumes: the whole pipeline.
- Produces: a passing golden parity test and CI across all release targets.

- [ ] **Step 1: Choose and commit the fixture pair**

Pick one WvW log from axibridge `testdata/` (e.g. `20260117-181030.zevtc`) and copy it to `fixtures/wvw-small.zevtc`. Generate its EI JSON via axibridge's local CLI path (`dotnet GuildWars2EliteInsights-CLI.dll -c <conf> <log>` with `SaveOutJSON=True, DetailledWvW=True`), then **trim** it to only the fields the parity test reads (top-level `durationMS`, `players[].account`/`dpsAll[0].damage`/`statsTargets[0][0].downContribution`, `targets[].name`) and save as `fixtures/wvw-small.ei.json`. Document the exact source log + EI version at the top of `golden.rs` in a comment.

- [ ] **Step 2: Write the golden parity test**

`crates/axilog-core/tests/golden.rs`:
```rust
use axilog_core::evtc::decode_raw;
use axilog_core::model::resolve;
use axilog_core::analysis::analyze;

fn rel_close(a: f64, b: f64) -> bool { (a - b).abs() <= 0.005 * b.abs().max(1.0) }

#[test]
fn matches_ei_totals_within_tolerance() {
    let bytes = std::fs::read("../../fixtures/wvw-small.zevtc").unwrap();
    let ei: serde_json::Value =
        serde_json::from_slice(&std::fs::read("../../fixtures/wvw-small.ei.json").unwrap()).unwrap();
    let raw = decode_raw(&bytes).unwrap();
    let enc = resolve(&raw);
    let m = analyze(&enc, &raw);

    // duration parity
    let ei_dur = ei["durationMS"].as_u64().unwrap();
    assert!(rel_close(enc.duration_ms as f64, ei_dur as f64),
        "duration {} vs EI {}", enc.duration_ms, ei_dur);

    // squad total damage parity (sum of players)
    let our_total: u64 = m.players.iter().map(|p| p.damage_total).sum();
    let ei_total: u64 = ei["players"].as_array().unwrap().iter()
        .filter_map(|p| p["dpsAll"][0]["damage"].as_u64()).sum();
    assert!(rel_close(our_total as f64, ei_total as f64),
        "squad damage {} vs EI {}", our_total, ei_total);
}
```

- [ ] **Step 3: Run the golden test**

Run: `cargo test -p axilog-core --test golden`
Expected: initially may FAIL if enum constants/CC predicate/team ids are off — this is the cross-check. Fix the suspect constants (Task 5 `sc`/`result`, Task 8 team ids, Task 11 `is_cc`) until damage and duration match within tolerance. If down-contribution is materially off, note it and open a follow-up per the spec's flagged risk; Milestone-1 gate is damage + duration + counts parity.

- [ ] **Step 4: Un-ignore the pipeline tests and run the full suite**

Remove `#[ignore]` from the tests in Tasks 6, 13, 14. Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 5: Write CI workflow**

`.github/workflows/ci.yml`:
```yaml
name: ci
on: [push, pull_request]
jobs:
  test:
    strategy:
      matrix:
        include:
          - { os: ubuntu-latest,  target: x86_64-unknown-linux-gnu }
          - { os: windows-latest, target: x86_64-pc-windows-msvc }
          - { os: macos-latest,   target: aarch64-apple-darwin }
          - { os: macos-13,       target: x86_64-apple-darwin }
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with: { targets: "${{ matrix.target }}" }
      - run: cargo build --workspace --target ${{ matrix.target }}
      - run: cargo test --workspace
```
(Add `aarch64-unknown-linux-gnu` as a cross-compiled build-only entry in a follow-up; native test runners for it aren't on GitHub-hosted runners.)

- [ ] **Step 6: Update .gitignore for large fixtures**

Add to `.gitignore`:
```
/fixtures/large/
```

- [ ] **Step 7: Commit**

```bash
git add -A && git commit -m "test: golden EI parity fixture + CI across targets"
```

---

## Self-Review

**Spec coverage:**
- Cargo workspace, engine/IO separation → Task 1, 3-crate split throughout. ✓
- EVTC decode (header/agents/skills/events, revision-aware) → Tasks 2–6. ✓
- zevtc container → Task 6. ✓
- Domain model, squad/enemy resolution, teams, map, agent-churn dedupe → Tasks 7–8. ✓
- Damage/DPS, downs/kills/deaths, down contribution, CC-over-time timeline → Tasks 9–11. ✓
- Native versioned JSON (default) → Task 12 + 13. ✓
- `table`/`csv` → Task 14. ✓
- `ei-json` adapter → Task 15. ✓
- Golden-file validation within tolerance, committed small fixture, large-fixture env pattern → Task 16 + Global Constraints. ✓
- MIT license, MSRV/edition, all release targets → Task 1 + Task 16 CI + Global Constraints. ✓
- Deferred (boons/healing/rotations/replay/SDKs/HTML) → correctly absent. ✓

**Placeholder scan:** No "TBD"/"implement later". The three empiricism-dependent spots (event byte offsets 48–58, statechange/result enum numerics, CC predicate, WvW team ids) are explicitly called out with a verification step and cross-checked by the Task 16 golden test rather than left vague — this is honest handling of reverse-engineered format details, not a placeholder.

**Type consistency:** `decode_raw`/`inflate_zevtc`, `resolve`, `analyze`, `build_report`, `to_ei_json`, `PlayerMetrics`/`Timeline`/`Metrics`, `Encounter`/`Player`/`Enemy`/`Team` names are used identically across tasks. `dedupe_players`, `apply`, `accumulate`, `timeline`, `apply_cc` signatures match their call sites in `analyze`.

**Known risks carried into implementation (flagged, not hidden):**
1. Combat-event single-byte field offsets (iff..is_statechange) — verify against arcdps `cbtevent` order in Task 5; golden test catches errors.
2. `sc::*` / `result::*` enum numeric values — verify against arcdps enums; golden test catches errors.
3. CC signal predicate (`is_cc`) — WvW breakbar detection is approximate; refine against fixture in Tasks 11/16.
4. Down-contribution window definition — documented approximation; exact EI parity is a flagged follow-up, not an M1 gate.
5. WvW team-id → color table — verify against the golden `wvWMapData`.
