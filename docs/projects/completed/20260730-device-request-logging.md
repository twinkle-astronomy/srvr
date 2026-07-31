# Device Setup & Log Endpoint Debugging

Operators couldn't previously see what a device sent during setup or what
its submitted logs actually contained without querying the database
directly — `POST /api/log` and `GET /api/setup` now log that content via
`info!()`. Originated from
[ideas/setup-and-log-endpoint-debugging](../ideas/) (idea file removed on
completion; scope was narrowed at implementation time to "log the request
info" — no new DB tables, retention policy, or export/redaction feature).
The new logging also surfaced and led to fixing two real device-compatibility
bugs against actual hardware in production.

## What shipped

- **Request content logging** (`src/device/api.rs`): `POST /api/log` logs
  each submitted `DeviceLogEntry`'s full content via `info!`, not just a
  count. `GET /api/setup` logs all parsed device fields (`fw_version`,
  `battery_voltage`, `rssi`, `width`, `height`), not just
  MAC/Model/FriendlyID — surfacing silently-swallowed header parse failures
  as `None`.
- **`GET /api/setup/`** (trailing slash) now routes to the same handler as
  `/api/setup` — axum treats the two as distinct routes by default.
- **Real device compatibility fix**, found via the new logging against a
  live device: some real-world firmware sends only `ID` + `FW-Version` on
  setup — no `model`, `Width`, or `Height`. Those `devices` columns are
  `NOT NULL`, so registration (and every subsequent `/api/display` poll)
  failed. Setup now defaults a missing `model` to `"unknown"` and missing
  `width`/`height` to `800x480` (TRMNL OG resolution); the poll/update path
  uses SQL `COALESCE` instead of an unconditional overwrite, so a later poll
  that omits one of these headers can't erase a value the device already
  reported.
- **Test infrastructure**: `CapturingWriter`, a `tracing::MakeWriter` backed
  by a `Vec<u8>` buffer, lets tests assert on actual `info!`/`error!` log
  content instead of just "doesn't panic." Building it surfaced a real
  flakiness source, fixed alongside it — see retrospective.

## Retrospective

**What worked**

- The feature validated itself almost immediately: the first real device to
  hit the new setup logging surfaced a genuine field bug (missing
  model/dimensions headers) that had presumably been failing silently
  before, with zero database digging required — exactly the workflow the
  idea doc asked for.
- TDD caught a subtlety that would otherwise have shipped a useless test:
  the first draft of the setup-logging test asserted on the FW-Version
  header value, which passed even *without* the fix, because the existing
  raw header dump already echoed it back. Switching to an unparseable
  `Battery-Voltage` (parsed field becomes `None` only in the new log line)
  made the test actually distinguish "logs the raw request" from "logs what
  was parsed."
- The user cut a broad set of architecture questions (retention policy,
  redaction/export feature, DB-backed activity trail across setup/poll/log)
  down to one sentence of concrete scope. Trusting that correction over the
  original idea doc's four bullets kept the change small and shippable the
  same day, and the narrower scope still resolved a real production issue.

**What caused friction, surprise, or rework**

- A flaky test with no visible logic bug: `tracing::subscriber::set_default`
  is thread-local and should be airtight, but under `cargo test`'s default
  parallel execution the capturing tests failed intermittently (always
  passed under `--test-threads=1`, ~1-in-3 failure rate at default
  parallelism). Root cause was non-obvious — `tracing`'s per-callsite
  `Interest` cache is a *process-wide* optimization: an unrelated parallel
  test hitting the same call site with no subscriber active could cache
  "not interested," silently suppressing that call site for every thread
  afterward regardless of a later test's own `set_default` override. A
  first fix attempt (a `Mutex` serializing the two capturing tests against
  each other) didn't work and had to be discarded — the interference came
  from ~120 *other*, unrelated tests sharing the same call sites, not from
  the two capturing tests racing each other. The actual fix (one permissive
  global default installed once, before any test's handlers run) is
  documented in place at `db::test_support::ensure_global_tracing_default`.
- This codebase had no established pattern for asserting on log output —
  `CapturingWriter` had to be built from scratch mid-implementation.

**What to change**

- No process changes proposed this round. The `CapturingWriter` pattern and
  the Interest-cache root cause are documented directly in code
  (`src/db.rs`'s `ensure_global_tracing_default`) for the next person who
  needs to test tracing output in this codebase.
