# Device Request Logging (setup + log endpoints)

**Branch:** `device-request-logging`

## What

`POST /api/log` and `GET /api/setup` should log the actual content of what a
device sent via `info!()`, so an operator watching server output (no DB
access needed) can see what happened on a specific request. Originated from
[ideas/setup-and-log-endpoint-debugging](../ideas/setup-and-log-endpoint-debugging.md);
this plan covers only the "log the request info" slice of that idea — no new
DB tables, retention policy, or export/redaction feature.

Today:
- `POST /api/log` (`src/device/api.rs:280-318`) logs only
  `"Received {N} log(s) from device"` — the submitted log entries'
  actual content (`message`, `wake_reason`, `battery_voltage`, etc.) is
  persisted to `device_logs` but never appears in server output.
- `GET /api/setup` (`src/device/api.rs:321-399`) already dumps raw request
  headers via `info!`, but never logs the *parsed* device fields. Header
  parsing failures are silently swallowed (`.and_then(|x| x.parse().ok())`),
  so e.g. a malformed `Battery-Voltage` header becomes `None` with nothing in
  the logs to show that happened — which is exactly the "why did setup
  succeed/fail" gap the idea doc describes.

## Which files

- `src/device/api.rs`
  - `log_handler` (~line 280): add per-entry `info!` logging of each
    submitted `DeviceLogEntry`'s content.
  - `setup_handler` (~line 321): add `info!` logging of the parsed device
    fields (not just raw headers).
  - Existing `#[cfg(all(test, feature = "server"))] mod tests` in the same
    file: add coverage per the TDD process.

## How

**`log_handler`**: keep the existing count line, and add one `info!` per
entry logging its full content:

```rust
info!("Received {} log(s) from device", payload.logs.len());
for entry in &payload.logs {
    info!("Log entry: {:?}", entry);
}
```

`DeviceLogEntry` already derives `Debug` — no new formatting code needed.
Logged before the access-token lookup, same as today's count line, so
content is visible even if the device turns out to be unauthorized.

**`setup_handler`**: replace the current MAC/Model/FriendlyID-only success
line with one that logs every parsed field:

```rust
info!(
    "Setup request - MAC: {:?}, Model: {:?}, FriendlyID: {}, FW: {:?}, Battery: {:?}, RSSI: {:?}, Width: {:?}, Height: {:?}",
    device.mac_address, device.model, device.friendly_id, device.fw_version,
    device.battery_voltage, device.rssi, device.width, device.height
);
```

This surfaces silent parse failures (a header that failed to parse shows up
as `None` here) without changing the existing raw-header dump above it.

No changes to `display_handler` (out of scope — user asked for log + setup
only), no new DB tables, no redaction/export feature, no retention policy.

## Testing approach

Logging output isn't currently covered by any test in this codebase (no
`tracing-test`/subscriber-capture dependency, no established pattern). Given
the mandatory TDD process, the plan is:
- Use the `tracing::subscriber::set_default` + a small in-memory capture
  layer (or the `tracing-test` crate if adding a dev-dependency is
  acceptable) to assert the expected fields appear in emitted log lines for
  both handlers.
- If that infrastructure proves disproportionate to the change, fall back to
  characterization tests that exercise the handlers end-to-end (as the
  existing `display_handler_*` tests do) and assert on the *response*/DB
  side effects, treating the `info!` calls themselves as the "mechanical
  mirror" exception (near-identical logging pattern to what already exists
  in this file) — flagged here rather than decided silently, per process.

## Open questions / tradeoffs

- Exact test strategy above (capture-layer vs. characterization) — will
  confirm once I check whether a log-capture crate is already available or
  needs adding as a dev-dependency.
- `setup_handler`'s new line uses `{:?}` on `Option<String>`/`Option<f64>`
  etc., which prints `Some("x")` / `None` rather than bare values — matches
  the existing codebase's ad hoc `{:?}` logging style elsewhere in this
  file, so kept consistent rather than hand-formatting each field.
