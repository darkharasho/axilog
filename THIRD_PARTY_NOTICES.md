# Third-party notices

axilog is an independent, original implementation (MIT — see [LICENSE](LICENSE)). It is not a
fork of any other parser. However, portions of this repository are **derived from** or
**verified against** the following projects, and their license terms are reproduced here as
required.

## GW2 Elite Insights Parser (GW2EI)

<https://github.com/baaron4/GW2-Elite-Insights-Parser> — MIT License, Copyright (c) 2018 baaron4.

axilog's relationship to GW2EI, precisely:

- **Semantics arbiter (not copied):** axilog's analysis passes are original Rust
  implementations. GW2EI's source was read to establish the *semantics* of Elite
  Insights-compatible output fields (what counts as a "connected" hit, how the replay polling
  grid is anchored, how buff stacks evict, …), and axilog's implementations are then calibrated
  against GW2EI's *output* on real logs. Source citations (`File.cs:line`) throughout axilog's
  comments document which GW2EI rule each implementation reproduces.
- **Derived data tables:** several constant tables are direct transcriptions from GW2EI source
  and are therefore derived portions under the MIT license: the damage-modifier definition
  catalog (`crates/axilog-core/src/analysis/damage_mods/catalog/`, regenerable via
  `scripts/gen_damage_mod_catalog.py`), the profession icon-URL table
  (`crates/axilog-core/src/icons.rs`), the WvW map-geometry table
  (`crates/axilog-core/src/wvw/maps.rs`), the condition-classification id set
  (`crates/axilog-core/src/analysis/condition_catalog.rs`), and per-buff capacity/stack-type
  classifications in `crates/axilog-core/src/analysis/buffs/` and
  `crates/axilog-core/src/analysis/damage_mods/`.
- **A test-only reference port:** `crates/axilog-core/tests/common/eiref.rs` is a labeled,
  literal transcription of GW2EI's `BuffSimulatorNoID` family, used exclusively as a diagnosis
  oracle in tests (nothing in shipped `src/` uses it).
- **Output-format compatibility:** axilog's `--format ei-json` reproduces the Elite
  Insights/dps.report JSON shape for interoperability.

### GW2EI license text (MIT)

```
MIT License

Copyright (c) 2018 baaron4

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## arcdps

arcdps is closed-source; axilog contains no arcdps code. The EVTC wire format is implemented
from arcdps's publicly documented format README (facts/format documentation), and certain
calculation methodologies (notably down-contribution) were re-expressed from guidance relayed
by the arcdps developer — methodology, not code.
