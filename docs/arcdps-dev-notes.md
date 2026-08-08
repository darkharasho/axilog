# arcdps developer guidance log

Running log of implementation guidance relayed from the arcdps developer. Each entry gets a
status and is folded into a milestone task when applicable. Newest last.

| # | Guidance | Status |
|---|----------|--------|
| 1 | Use `CBTS_WVWTEAMS` to configure WvW team ids from the log rather than hardcoding red/blue/green tables. | In progress — M2 Task 2b (dynamic primary source; static table demoted to fallback for old logs). |
| 2 | Track `CBTS_STUNBREAK` alongside CC metrics. | Planned — M2 Task 3 (per-player stun_breaks + removed stun duration; EI `defenses.stunBreak`). |
| 3 | Content-local ids (`n_contentlocal`: EFFECT=0, MARKER=1, SKILL=2, SPECIES_NOT_GADGET=3, TEAM=4, EMOTE=5, TRANSFORMATION=6) are session-local — map to stable GUIDs via `CBTS_IDTOGUID`. | In progress — M2 Task 2b decodes IDTOGUID into `RawLog.guid_map`; TEAM exposed now, SKILL/SPECIES retained for M3 buff/skill identity. |
| 4 | `CBTS_MARKER` provides above-target markers; EI probably has the squad-marker GUIDs in an enum somewhere. | Planned — M2 Task 7 (decode marker statechange, resolve marker GUIDs via IDTOGUID MARKER mappings, cross-reference EI's squad-marker GUID enum, expose per-agent marker in native schema). |

## Notes

- These features are part of axilog's differentiation: arcdps-spec data that EI's JSON does not
  (or only partially) surfaces. Native schema exposes them first-class; the EI adapter emits only
  what EI's shape supports.
- Verify every enum value and payload layout against the arcdps EVTC reference
  (https://www.deltaconnected.com/arcdps/evtc/README.txt) at implementation time — do not trust
  memory or third-party writeups.
