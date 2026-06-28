# Plan: Prometheus Time-Range Query Support

**Branch:** `prometheus-range-query`

## What & why

Today a template can only run **instant** Prometheus queries — a single value per
series at "now" (`PrometheusQuery::get_render_obj` calls `client.query(...)`).
This project adds **range** queries: a template author configures a PromQL
expression plus a time window and resolution, and the renderer fetches the full
time series over that window (`client.query_range(...)`). The raw series is
exposed to Liquid templates so authors can draw trend lines, sparklines, and bar
charts themselves in SVG.

Per design decisions:
- **Raw series only** — we expose `{timestamp, value}` points; no graphics/filter
  code in this project (a sparkline/chart filter is captured as a separate idea).
- **Separate `range_queries` table** — a parallel concept to instant queries, not
  extra columns on `prometheus_queries`.
- **Configurable window & step per query** — author sets a duration (e.g. `1h`)
  and a step (e.g. `60s`).

## Data model

New table `range_queries`, mirroring `prometheus_queries` plus two fields:

```sql
-- migrations/20260628000000_create_range_queries.sql
CREATE TABLE IF NOT EXISTS range_queries (
    id          INTEGER  PRIMARY KEY AUTOINCREMENT,
    template_id INTEGER  REFERENCES templates(id),
    name        TEXT     NOT NULL,
    addr        TEXT     NOT NULL,
    query       TEXT     NOT NULL,
    duration    TEXT     NOT NULL,   -- e.g. "1h", "30m", "24h" (window = now-duration .. now)
    step        TEXT     NOT NULL,   -- e.g. "60s", "5m" (resolution between points)
    created_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE        INDEX IF NOT EXISTS range_queries_template_id      ON range_queries(template_id);
CREATE UNIQUE INDEX IF NOT EXISTS range_queries_name_template_id ON range_queries(name, template_id);
```

`duration`/`step` are stored as Prometheus-style strings and parsed at execution
time into seconds. A small parser handles `s/m/h/d` suffixes (sufficient for now).

## Model types — `src/models/mod.rs`

```rust
pub struct RangeQuery {            // mirrors PrometheusQuery + duration/step
    pub id: Option<i64>, pub name: String, pub template_id: i64,
    pub addr: String, pub query: String,
    pub duration: String, pub step: String,
    pub created_at: NaiveDateTime, pub updated_at: NaiveDateTime,
}
impl RangeQuery { pub fn new(template_id: i64) -> Self { /* sensible defaults: "1h","60s" */ } }

// Preview types (parallel to PrometheusQueryResult / PrometheusMetricResult)
pub struct RangeQueryResult { pub query_name: String, pub series: Vec<RangeSeries>, pub error: Option<String> }
pub struct RangeSeries {
    pub labels: HashMap<String,String>,
    pub points: Vec<RangePoint>,
    // scaling helpers (DECIDED: include) — data, not rendering. Let template
    // authors map values to a viewport without iterating in Liquid.
    pub min: f64, pub max: f64, pub first: f64, pub last: f64, pub count: usize,
}
pub struct RangePoint { pub t: f64, pub value: f64 }
```

`RenderContext` gains `pub range_queries: Vec<RangeQuery>`.

## Execution — `src/models/server.rs`

`RangeQuery::get_render_obj()` parallels the instant version but calls
`query_range`. **Verify exact 0.8 API when coding** — expected shape:

```rust
let now = Utc::now().timestamp();
let start = now - parse_duration_secs(&self.duration)?;
let step  = parse_duration_secs(&self.step)? as f64;
let matrix = client.query_range(self.query.as_str(), start, now, step)
    .get().await?.data().as_matrix() ...
// each RangeVector -> { labels: metric(), points: samples()->[{t: timestamp(), value: value()}] }
```

Returns `Vec<Object>` shaped for Liquid:
```
prometheus_range.<name>[i].labels.<key>
prometheus_range.<name>[i].points[j].t       # unix seconds
prometheus_range.<name>[i].points[j].value
prometheus_range.<name>[i].{min,max,first,last,count}   # scaling helpers
```

**Time/`now` source (DECIDED).** Do *not* thread the `Clock` trait through the
async render path (it isn't there today; renderer uses `Utc::now()`). Instead
isolate the only time-sensitive logic — the window arithmetic — into a pure,
unit-tested helper, and call it with real time at the edge:

```rust
// pure + deterministic: tested with explicit `now`
fn compute_window(now_secs: i64, duration_secs: i64, step_secs: f64) -> (i64, i64, f64) {
    (now_secs - duration_secs, now_secs, step_secs)
}
// call site uses the existing abstraction for consistency:
let now = RealClock.now_secs();
```

This keeps the risky math deterministic (test #1/#3) without widening the change
into the whole render plumbing.

## Renderer — `src/device/renderer.rs`

In `render_vars`, build `range_data` from `render_context.range_queries` the same
way `prometheus_data` is built (skip series that error), and add
`"prometheus_range": liquid::object!(range_data)` to the returned object.
Add a `RangeQueryError` variant to renderer `Error` if needed.

## DB CRUD — `src/db.rs`

Add, mirroring the `*_prometheus_query` functions:
- `get_range_queries(template_id)`, `create_range_query`, `update_range_query`, `delete_range_query`
- Cascade delete in template teardown (mirror loop at `db.rs:120`)
- Clone on template duplication (mirror `db.rs:454`)

## Server functions — `src/frontend/server_fns.rs`

- `save_range_query`, `delete_range_query`, `get_range_queries_for_template`
- `execute_range_query(RangeQuery) -> RangeQueryResult` (for editor live preview)
- Add `range_queries` to all three `RenderContext` builders (lines ~299, ~328, ~350)

## Editor UI — `src/frontend/pages/template_editor/`

- New `range_queries.rs` `RangeQueries` component, modeled on `prometheus_queries.rs`,
  with extra **Duration** and **Step** inputs. Live preview shows a **summary per
  series — not the raw points**: series count, point count, time span, and
  min/max/last. (DECIDED: never dump the full points list here.)
- Register in `mod.rs` and render it alongside `PrometheusQueries`.

## Template-variables component — summary, not raw data (DECIDED)

`get_template_context` flattens the whole render object via `obj_to_template_var`,
so a range series would explode into ~1440 `prometheus_range.<name>.points[j].value`
rows in the "Available Template Variables" panel. Instead:

- Do **not** recurse into the `points` arrays when building the variables list.
  For each range series emit summary rows only, e.g.
  `prometheus_range.<name>[i]` → `"60 pts · last 1h · min 0.20 max 0.91 last 0.42"`,
  plus the discoverable scaffold paths (`...points[j].t`, `...points[j].value`,
  `...min`, `...max`, …) shown as *shape* rather than enumerated values.
- The real renderer still receives the full series unchanged — only the UI
  summarizes. Implement by special-casing the `prometheus_range` key in
  `get_template_context` (or by having `obj_to_template_var` cap/summarize long
  arrays) so the giant arrays never reach the variables table.

## Docs

- `docs/templates.md`: document `prometheus_range.<name>[i].points[j].{t,value}` and `.labels`.

## TDD order (each: failing test first)

1. `parse_duration_secs` — `"1h"`→3600, `"30m"`→1800, `"60s"`→60, `"2d"`→172800, error on junk.
2. DB round-trip — create/get/update/delete a `range_query` (use existing sqlx test setup).
3. `RangeQuery::get_render_obj` shape — against a mocked/stub Prometheus response, assert
   the Liquid object has `points` with `t`+`value`. (May need a small HTTP mock; check testing.md.)
4. Renderer — a template using `prometheus_range.<name>` renders the series into SVG.
5. Cascade delete + clone-on-duplicate behave like instant queries.

## Resolved decisions

1. **Scaling helpers — yes.** Expose per-series `min`/`max`/`first`/`last`/`count`
   alongside `points`.
2. **`now` source — pure helper.** Don't thread `Clock` through the render path;
   isolate window arithmetic into a unit-tested `compute_window(now, ...)` and call
   it with `RealClock.now_secs()` at the edge.
3. **Duration/step units — mini-parser.** Start with a small tested `s/m/h/d`
   parser, no new dependency. Revisit if authors need richer formats.
4. **Preview / variables payload — summarize, never dump.** Both the editor live
   preview and the "Available Template Variables" panel show a per-series summary
   (count, span, min/max/last); the full points array only reaches the real
   renderer. See the two UI sections above.

## Remaining unknown to verify during implementation

- Exact `prometheus-http-query` 0.8 range API (`query_range` arg types,
  `as_matrix`, `RangeVector::samples`, `Sample::timestamp/value`). Pin this against
  the built crate before writing test #3.
