# Prometheus Time-Range Query Support

Added **range** queries alongside the existing instant queries. A template can
now configure a PromQL expression plus a window (`duration`, e.g. `1h`) and
resolution (`step`, e.g. `60s`); the renderer fetches the full time series via
Prometheus' `query_range` and exposes it to Liquid under `prometheus_range.<name>`
as raw `{t, value}` points plus per-series scaling helpers
(`min`/`max`/`first`/`last`/`count`) so authors can draw trends and graphs.

Key pieces:
- `range_queries` table + `RangeQuery` model, with CRUD, cascade-delete, and
  clone-on-template-copy mirroring `prometheus_queries`.
- Pure time helpers (`parse_duration_secs`, `range_window`) keep the window math
  deterministic and clock-free, tested without hitting the network.
- Editor section with Duration/Step inputs; both the editor preview and the
  "Available Template Variables" panel show a **summary** per series (the panel
  collapses the points array to one row) rather than dumping thousands of samples.
- Introduced the first in-memory DB test harness (`db::test_support::init_test_db`).

Graph-rendering helpers (a sparkline/chart filter) were split out as a separate
idea: `docs/projects/ideas/prometheus-graph-rendering.md`.
