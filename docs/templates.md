# Liquid Templates

Templates are SVG files rendered with the Liquid templating language. The rendering pipeline is: Liquid → SVG → usvg → resvg → 1-bit BMP.

## Available Variables

```
device.width, device.height, device.friendly_id, device.mac_address
device.battery_voltage, device.battery_percent_charged, device.rssi, device.fw_version
time (HH:MM AM/PM), date (YYYY-MM-DD), timezone (e.g. PST)
prometheus.<name>[i].value, prometheus.<name>[i].labels.<key>
prometheus_range.<name>[i].labels.<key>
prometheus_range.<name>[i].points[j].t (unix seconds), prometheus_range.<name>[i].points[j].value
prometheus_range.<name>[i].min, .max, .first, .last, .count
http.<source_name>.<json.path>
```

`prometheus` holds **instant** queries (one value per series). `prometheus_range`
holds **range** queries (a time series per match) — configured per template with a
PromQL expression plus a duration (e.g. `1h`) and step (e.g. `60s`). Each series
exposes its raw `points` plus scaling helpers (`min`/`max`/`first`/`last`/`count`)
so you can map values into a viewport. Example sparkline-style polyline:

```liquid
{% assign s = prometheus_range.cpu[0] %}
<polyline points="{% for p in s.points %}{{ forloop.index0 }},{{ p.value }} {% endfor %}" />
<!-- scale Y with s.min / s.max to fit your SVG viewport -->
```

## Custom Filters

```liquid
{{ value | qrcode }}
{{ value | qrcode: module_size: 3 }}
{{ ssid | qrcode_wifi: password: "pw" }}
{{ ssid | qrcode_wifi: password: "pw", security: "WEP", module_size: 3 }}
{{ "OpenNet" | qrcode_wifi: password: "", security: "nopass" }}
```

Filter implementations are in `src/device/liquid_filters.rs`.
