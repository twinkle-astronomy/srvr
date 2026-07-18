# Firmware OTA Updates — Plan

**Branch:** `firmware-ota-updates`

## What

Let an admin upload TRMNL firmware binaries (`.bin`, per the real [trmnl-firmware](https://github.com/usetrmnl/trmnl-firmware) OTA contract), tag each with the hardware `model` it targets, and mark one release "active" per model. Devices already send their `model` and firmware version (`FW-Version` header → `devices.fw_version`) on every `/api/display` poll; when a device's reported version doesn't match the active release for its model, the server now tells it to update — filling in the `update_firmware`/`firmware_url` fields that already exist in the response shape but are currently hardcoded to `false`/absent.

Rollback is just "activate an older release again" — no separate rollback mechanism. Each device gets an opt-in toggle (`firmware_updates_enabled`, **default off**) so a newly-registered or existing device is never auto-flashed until an admin deliberately enables it; once enabled, it picks up whatever release is currently active for its model on its next poll, and stays in sync with that model's active release until toggled off again. All uploaded binaries are retained indefinitely; admin deletes old ones manually when they want to reclaim space (deleting the currently-active release for a model is blocked — activate a different one first).

This resolves the idea file's open questions:
- **Firmware format** → raw `.bin`, not zip/archive (confirmed against upstream firmware docs/behavior).
- **Compatibility** → free-text `model` tag on each release, matched against `devices.model` (which is itself free text reported by the device — no fixed enum exists in this codebase today).
- **Rollback policy** → activate an older release; no auto-pruning (per your answer).
- **Device authentication** → reuse the existing HMAC URL-signing scheme (`src/hmac.rs`), the same mechanism that already protects `/render/screen.bmp`. No new per-device secret.
- **Size limits** → server-side sanity cap on upload size (proposed 16 MiB — see Open Questions; the real hardware OTA partition limit isn't documented upstream, so this is a guardrail, not a verified hardware ceiling).

## Which files

**New:**
- `migrations/20260718000000_create_firmware_releases.sql` — new table.
- `migrations/20260718000001_add_firmware_updates_enabled_to_devices.sql` — new column.
- `src/api/firmware.rs` — admin JSON API (list/upload/activate/delete releases).
- `src/frontend/pages/firmware.rs` — admin page: upload form, release list per model, activate/delete actions.

**Modified:**
- `src/models/mod.rs` — add `FirmwareRelease` struct; add `firmware_updates_enabled: bool` (defaults `false`) to `Device`.
- `src/db.rs` — `create_firmware_release`, `get_firmware_releases`, `get_active_firmware_release(model)`, `activate_firmware_release(id)`, `delete_firmware_release(id)`, `update_device_firmware_updates_enabled(device_id, enabled)`.
- `src/device/api.rs` — `DisplayResponse` gains `firmware_url: Option<String>`; `display_handler` looks up the active release for the device's model and decides `update_firmware`/`firmware_url`; new `GET /firmware/download` route + handler (added to `device_routes` so it inherits the 30s timeout / connection-close middleware).
- `src/api/devices.rs` — `POST /dashboard/devices/{id}/firmware-updates` (mirrors the existing `POST /dashboard/devices/{id}/compat` pattern exactly: `{enabled: bool}` body → `NO_CONTENT`).
- `src/api/mod.rs` — merge `firmware::router()`.
- `src/frontend/api.rs` / `src/frontend/server_fns.rs` — fetch helpers: `get_firmware_releases`, `upload_firmware_release` (multipart), `activate_firmware_release`, `delete_firmware_release`, `update_device_firmware_updates_enabled`.
- `src/frontend/store.rs` — mirrors `update_device_maximum_compatibility`'s existing store method for the new toggle; store methods for firmware release list/actions.
- `src/frontend/pages/mod.rs`, `src/frontend/mod.rs` — register `FirmwarePage` + `/firmware` route.
- `src/frontend/pages/devices.rs` — add a `FirmwareUpdatesToggle` component next to the existing `MaxCompatibilityToggle` on the device detail page.
- `Cargo.toml` — enable axum's `multipart` feature (not currently on; needed for the binary upload endpoint).
- `docs/models.md` / `docs/api.md` / `docs/templates.md` — no changes expected, but will update if the implementation deviates from documented patterns.

## How

### Data model

```sql
-- firmware_releases
CREATE TABLE firmware_releases (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    model       TEXT NOT NULL,
    version     TEXT NOT NULL,
    filename    TEXT NOT NULL,
    size_bytes  INTEGER NOT NULL,
    binary      BLOB NOT NULL,
    active      INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE UNIQUE INDEX idx_firmware_releases_model_version ON firmware_releases (model, version);
-- enforces "at most one active release per model" at the DB level
CREATE UNIQUE INDEX idx_firmware_releases_one_active_per_model ON firmware_releases (model) WHERE active = 1;
```

The binary is stored as a SQLite `BLOB`, not a file on disk — consistent with how everything else in this codebase persists (`templates.content` as `TEXT`, no existing on-disk storage pattern) and avoids requiring a second persistent volume beyond the SQLite DB file for a self-hosted "single binary + SQLite" deployment. Firmware images are small enough (low single-digit MB) for SQLite BLOBs to be a non-issue.

`devices` gets one new column, off by default so no device auto-updates until an admin opts it in:
```sql
ALTER TABLE devices ADD COLUMN firmware_updates_enabled INTEGER NOT NULL DEFAULT 0;
```

### Device-facing API

`GET /api/display` (`src/device/api.rs`): after resolving `device`, look up `db::get_active_firmware_release(&device.model)`. If a row exists, `device.firmware_updates_enabled` is true, and `release.version != device.fw_version.as_deref().unwrap_or("")`, build a signed download URL the same way `image_url` is already built (`generate_signature_bytes` + `URL_SAFE_NO_PAD`) and set:
```rust
update_firmware: true,
firmware_url: Some(format!("{scheme}://{host}/firmware/download?device_id={}&t={}&sig={}", device.id, timestamp, sig_encoded)),
```
Otherwise `update_firmware: false`, `firmware_url: None`. The signature is device+timestamp scoped (not resource-scoped), so the same `validate_signature` helper already used by `/render/screen.bmp` works unchanged for this new route.

New `GET /firmware/download?device_id&t&sig` handler: validate the signature exactly like `render_screen_handler` does, load the device, load the active release for `device.model`, stream `binary` as the response body with `Content-Type: application/octet-stream` and `Content-Disposition: attachment; filename="{filename}"`. If no active release matches at download time (race: admin changed the active release between poll and download), return 404 — the device will just try again on its next poll.

### Admin API (`src/api/firmware.rs`, mirrors `templates.rs`/`devices.rs` conventions)

- `GET /dashboard/firmware` → `Vec<FirmwareReleaseSummary>` (id, model, version, filename, size_bytes, active, created_at — **no binary bytes**, keep the list light).
- `POST /dashboard/firmware` → `axum::extract::Multipart` with fields `model`, `version`, `file`. Validates: file present and non-empty, size ≤ cap (413 if over), `(model, version)` not already taken (409 via the unique index → `ApiError` conflict mapping). Inserts with `active = 0`.
- `POST /dashboard/firmware/{id}/activate` → transaction: `UPDATE firmware_releases SET active = 0 WHERE model = (SELECT model FROM firmware_releases WHERE id = ?)`, then `UPDATE firmware_releases SET active = 1 WHERE id = ?`.
- `DELETE /dashboard/firmware/{id}` → 409 if the release is currently active ("activate a different version first"), else delete.

`POST /dashboard/devices/{id}/firmware-updates` (`src/api/devices.rs`, mirrors `update_device_compat` verbatim) → `{enabled: bool}` → `db::update_device_firmware_updates_enabled`.

### Frontend

New `/firmware` admin page: upload form (model text input, version text input, file picker), and a list of releases grouped by model, each with "Activate" (disabled if already active) and "Delete" (disabled if active) buttons. This is the first binary file upload in the codebase, so `src/frontend/api.rs` needs a new multipart-capable fetch helper (browser `FormData` + `File`, alongside the existing JSON `get`/`post` helpers) rather than reusing the base64-in-JSON approach used for PNG previews — avoids ~33% base64 bloat and large-string JSON parsing for multi-MB payloads.

Device detail page (`src/frontend/pages/devices.rs`) gets a second toggle, `FirmwareUpdatesToggle`, implemented identically to the existing `MaxCompatibilityToggle` (`:286`, `:720`) — same component shape, different endpoint/field.

### Testing shape (per `docs/testing.md` / TDD process)

- `src/hmac.rs`-based signature reuse needs no new tests (unchanged).
- `display_handler`: new inline tests for the version-mismatch → `update_firmware: true` + `firmware_url` present case; version-match → `false`; `firmware_updates_enabled = false` → always `false` even with a newer active release.
- `firmware::router()`: auth-required 401 check (mirrors the existing pattern), upload → list round-trip, duplicate `(model, version)` → 409, activate flips the flag and deactivates the sibling, delete-while-active → 409.
- `db::update_device_firmware_updates_enabled` is the "mechanical mirror" case explicitly allowed by `development-process.md` — it's a near-verbatim copy of `update_device_maximum_compatibility`, so I'll write it alongside a characterization test rather than strict test-first, and will call that out when it happens.
- Browser E2E: extend the existing tier with an upload → activate → device-toggle flow if time allows; not required for a first pass since the native handler tests already cover the logic-bearing paths.

## Open questions / tradeoffs

1. **Upload size cap** — proposing 16 MiB as a server-side guardrail (real ESP32 OTA partitions are typically 1–4 MB, so this is generous headroom, not a verified device limit). Flag if you want a different number or want it configurable via env var instead of hardcoded.
2. **`(model, version)` matching is exact-string, not semver-ordered** — the server doesn't try to determine "newer," it just checks "does the active release's version differ from what the device reports." This matches the upstream TRMNL behavior described in their docs (compare `FW-Version` header against configured version) and keeps the logic simple, but means version strings must be typed consistently (e.g. always `1.3.0`, not sometimes `v1.3.0`). Worth confirming this is fine rather than wanting semver comparison.
3. **No staged/canary rollout within a model** — activating a release pushes it to every *enabled* device of that model on their next poll simultaneously (matches your chosen "global active-per-model + per-device opt-in" design, opt-in by default-off). If you want "update 1 device first, watch it, then the rest," that's just enabling the toggle on one device before the others — a natural side effect of default-off, not a separate staging feature.
4. **`/firmware/download` failure mode** — a 404 on signature-valid-but-no-active-release is treated as a transient race and left for the device's next poll to resolve; no retry/backoff logic is added server-side (the device firmware itself is expected to handle a failed download gracefully, per the "throttle OTA on failure" behavior in upstream firmware).
