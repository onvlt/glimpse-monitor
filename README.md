# glimpse-monitor

Applet id: `lt.nyv.glimpse-monitor`.

System monitor widgets for the Glimpse panel: per-widget chips (CPU, RAM, disk,
temperature) and a popover with meter rows and a CPU history chart. Readings
come from the `sysinfo` crate.

Requires Cargo. Cargo fetches the `glimpse-sdk` and `sysinfo` crates on first
build.

Run `just dev`: it builds the release binary, loads `devconfig.toml` and attaches the
applet with its options and env. Options edits update it; source edits rebuild and restart it.

The justfile wraps the whole loop: `just dev`, `just test`, `just lint`, `just package`,
`just install`.

## Options (panel.toml)

| Option | Values | Default |
| --- | --- | --- |
| `interval` | seconds, `1..=60` | `2` |
| `widgets` | list of `cpu`, `ram`, `disk`, `temp` | all four |
| `mounts` | up to 8 absolute mount paths | `["/"]` |
| `ram-display` | `percent` or `absolute` | `percent` |
| `disk-display` | `percent` or `absolute` | `percent` |
| `temp-warn` | °C limit, `20..=120` | `75` |
| `temp-error` | °C limit, `20..=120` | `85` |

```toml
[panels.…]
left = ["lt.nyv.glimpse-monitor"]

[applets."lt.nyv.glimpse-monitor"]
options = { interval = 2, widgets = ["cpu", "ram", "disk", "temp"], mounts = ["/"], "ram-display" = "percent", "disk-display" = "percent", "temp-warn" = 75, "temp-error" = 85 }
```

Add `left = ["lt.nyv.glimpse-monitor"]` to a panel's applet zone.
