## Project Context

axilog is a cross-platform, CLI-first reimplementation of Elite Insights for parsing GW2 arcdps combat logs, part of the axi suite. It has a reusable Rust parsing core with planned Python/Node SDKs, matches standard Elite Insights functionality, and follows the arcdps spec more closely — notably down contribution, CCs over time, and full timeline support.

## Goals

- Cross-platform CLI parser for GW2 arcdps logs as the first-class interface
- Reusable Rust parsing core shared across CLI and SDKs
- Planned Python and Node SDKs on top of the Rust core
- Full parity with standard Elite Insights functionality
- Closer adherence to the arcdps spec: down contribution and CCs over time (full timeline support)

## Out of scope

- Not tied to a single OS / Windows-only like the original EI

## Suggested stack

- **Rust** — Fast, cross-platform parsing core reused by the CLI and all SDKs
- **Python SDK** — Bindings over the Rust core for scripting/analysis users
- **Node SDK** — Bindings over the Rust core for JS/TS integrations
