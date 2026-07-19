# Firmware OTA Updates

Admins upload TRMNL firmware binaries (`.bin`), tag each with the hardware
`model` it targets, and mark one release "active" per model. Devices already
send `model` and `fw_version` on every `/api/display` poll; when a device's
reported version doesn't match the active release for its model — and the
device has opted in via a per-device toggle (default **off**) — the server
fills in `update_firmware`/`firmware_url` in the poll response, and the
device fetches the binary from a new signed `GET /firmware/download` route.
Rollback is just "activate an older release again." Originated from
[ideas/firmware-update](../ideas/) (idea file removed on completion).

## What shipped

- **Data model**: `firmware_releases` table (`model`, `version`, `filename`,
  `size_bytes`, `binary` BLOB, `active`), with two unique indexes —
  `(model, version)` rejects duplicate uploads, and a partial index on
  `(model) WHERE active = 1` makes "at most one active release per model" a
  DB-level invariant, not just application logic. `devices` gets one new
  column, `firmware_updates_enabled` (default `0`).
- **Device-facing**: a pure `decide_firmware_update()` in
  [src/device/api.rs](../../../src/device/api.rs) — takes the enabled flag,
  the device's reported version, and the active release; returns `Some`
  only on a genuine version mismatch for an opted-in device. Opted-out
  devices skip the release lookup entirely (the handler runs on every poll
  of every device). `GET /firmware/download` shares a
  `check_signed_request` helper with `/render/screen.bmp` (same
  device+timestamp scoped HMAC; no new device auth mechanism).
- **Admin API** ([src/api/firmware.rs](../../../src/api/firmware.rs)):
  list/upload/activate/delete. Upload is the codebase's first
  binary/multipart endpoint — `axum::extract::Multipart` with
  `DefaultBodyLimit::max(16MiB)` on that route (axum's `Bytes`-based
  extractors otherwise cap at 2MB). Uploaded filenames are sanitized
  because they're later interpolated into a `Content-Disposition` header.
  Duplicate `(model, version)` → 409 via the unique index; activation of an
  unknown id → 404 (rows_affected check in the transaction); deletion is
  atomic on `active = 0` so a concurrent activation can't lose the active
  release.
- **Admin UI** (`/firmware`,
  [src/frontend/pages/firmware.rs](../../../src/frontend/pages/firmware.rs)):
  upload form (model/version/file), releases grouped by model with
  Activate/Delete. Device detail page gets a `FirmwareUpdatesToggle`
  mirroring `MaxCompatibilityToggle`.
- **Version auto-fill from the binary**: `parse_esp_app_version()` reads the
  `esp_app_desc_t` struct ESP-IDF embeds in app images. Espressif documents
  a "fixed offset" of 32 bytes, but a real ESP32-S3 + Rust/embassy build
  places it at 64 (extra leading segment), so the parser scans a bounded
  window for the descriptor magic instead of trusting one offset. Detection
  is best-effort and *visible*: the UI reports "Detected version X" or
  "Couldn't auto-detect", the field stays editable, and a typed value is
  never clobbered by a later file selection.

## Retrospective

**What worked**

- DB-level invariants doubled as TDD specs: writing
  `activate_firmware_release` broken-on-purpose, the failing test's error
  *was* the partial unique index complaining — the database named the
  violated invariant, no hand-written assertion needed.
- Extracting `decide_firmware_update` as a pure function: four
  branch-covering unit tests with zero DB/HTTP setup, plus two integration
  tests through the full handler for the wiring.
- Live browser verification repeatedly caught what unit tiers could not:
  the delete-409 being unreachable through the UI (button disabled by
  design), the `sr-only` toggle needing a `<label>` click, and — most
  tellingly — a *failed fix*: remounting the file input via a Dioxus `key`
  bump compiled and looked right, but the new E2E assertion showed the DOM
  element never reset; the working fix (clear via `web_sys`) was proven in
  the same run. "Tests green" twice meant "not actually working" here.
- A structured pre-PR review pass (10 angles) after everything was green
  found 12 real issues, 8 worth fixing — including a rendering bug
  (duplicate model groups after upload) that every existing test missed
  because the E2E happy path only ever uploaded into an empty list.

**What caused friction, surprise, or rework**

- **Verified docs still weren't ground truth.** The ESP-IDF offset was
  checked against Espressif's official documentation before implementing —
  and the documented "fixed offset" was still wrong for the user's real
  firmware. The parser failed *silently*, which read as "the code isn't
  running at all." Two lessons: (a) for binary-format claims, a real
  artifact beats documentation; the fix landed with the actual header bytes
  as a regression fixture in both test tiers. (b) A silent no-op on a
  user-visible path is indistinguishable from a bug — the reworked UI
  reports detection outcomes explicitly.
- No project `verify`/`run` skill existed, so browser verification was a
  cold start (stale-WASM-bundle trap, `reqwest` needing the `multipart`
  dev-feature, `sr-only` click pattern, `wait_for_text`'s single-quote
  XPath limitation, in-browser `DataTransfer` file synthesis because the
  chrome container shares no filesystem). All captured in
  `.claude/skills/verify/SKILL.md`.
- `js-sys` needed declaring explicitly despite being present transitively —
  Cargo doesn't expose a transitive dependency's symbols.
- Substring text-waits in E2E are a trap: `wait_for_text("Active")` matched
  the "Activate" button and made the wait vacuous (a latent race). Exact
  `normalize-space(.)='…'` matches where the needle is a prefix of other UI
  text.

**What to change (confirmed and applied)**

Both proposals were confirmed and are now rules in
[development-process.md](../../development-process.md#rules):

- *Surface fallback outcomes on user-visible flows* — a silent `None`
  looks identical to dead code, and cost a debugging round-trip here.
- *Fixture real artifacts, don't re-synthesize them* — when a bug is
  reproduced from a real artifact (binary, wire capture), check the
  artifact's bytes in as the regression fixture rather than rebuilding it
  from the same assumptions that produced the bug.
