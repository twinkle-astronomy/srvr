# Firmware Update Support (Idea)

## Overview
Add an over‑the‑air (OTA) workflow that lets the TRMNL e‑ink display server distribute new firmware images to registered devices. 

## Goals
| Goal | Why it matters |
|------|-----------------|
| **Zero‑downtime OTA** | Push fixes or new features to deployed displays without physical access. |
| **Version tracking & rollback** | Keep a history of released versions so devices can revert if an update fails. |
| **Compatibility** | The OTA mechanism must work with the existing open‑source TRMNL firmware found at https://github.com/usetrmnl/trmnl-firmware. |


## Open Questions / Risks
| Topic | Question |
|-------|----------|
| Firmware format | Does the device expect a raw binary, a zip archive, or another container? |
| Device Compatability | How do we ensure devices only get firmware that is compatible? |
| Rollback policy | How many prior versions should be retained for possible rollback? |
| Device authentication | Should we reuse existing login tokens or generate per‑device secrets for reporting? |
| Size limits | What is the maximum firmware size the hardware can safely handle? |

## Next Steps (Ideas)
- Review the TRMNL firmware repository to confirm any required hooks for OTA updates (e.g., signature verification routine).
- Sketch a simple admin UI concept for uploading new firmware builds and viewing release history.
