# Liquid Templates

Templates are SVG files rendered with the Liquid templating language. The rendering pipeline is: Liquid → SVG → usvg → resvg → 1-bit BMP.

## Available Variables

```
device.width, device.height, device.friendly_id, device.mac_address
device.battery_voltage, device.battery_percent_charged, device.rssi, device.fw_version
time (HH:MM AM/PM), date (YYYY-MM-DD), timezone (e.g. PST)
prometheus.<name>[i].value, prometheus.<name>[i].labels.<key>
http.<source_name>.<json.path>
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
