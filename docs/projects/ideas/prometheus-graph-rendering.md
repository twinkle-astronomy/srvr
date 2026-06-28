# Prometheus Graph Rendering Helpers

Users can turn a Prometheus time series into a chart on their e-ink display
without hand-writing SVG. Instead of computing coordinates and scaling in a
Liquid template, an author points a built-in helper at a range query and gets a
trend line, sparkline, or bar chart rendered for them.

This builds on time-range query support (which exposes the raw series). It
removes the friction of manually mapping data points to pixel coordinates,
handling min/max scaling, and drawing axes — the parts that are tedious and
error-prone to express in a template.

Possible levels, smallest to largest:
- A **sparkline** helper that emits a single SVG path/polyline for one series
  (e.g. `{{ prometheus_range.cpu | sparkline: width: 200, height: 40 }}`).
- A **full chart** helper that emits a complete labelled chart with axes and
  gridlines.
