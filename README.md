# glimpse-monitor

Applet id: `lt.nyv.glimpse-monitor`.

System monitor widgets for the Glimpse panel: chips for CPU, RAM, swap, disk,
temperature, network speed, load average and uptime, plus a popover with meter
rows, a CPU history chart, per-core bars and system facts. Readings come from
the `sysinfo` crate.

Requires Cargo. Cargo fetches the `glimpse-sdk` and `sysinfo` crates on first
build.

Run `just dev`: it builds the release binary, loads `devconfig.toml` and attaches the
applet with its options and env. Options edits update it; source edits rebuild and restart it.

The justfile wraps the whole loop: `just dev`, `just test`, `just lint`, `just package`,
`just install`.

See [docs/architecture.md](docs/architecture.md) for how the code is structured.

## Options (panel.toml)

| Option | Values | Default |
| --- | --- | --- |
| `interval` | seconds, `1..=60` | `2` |
| `widgets` | list of `cpu`, `ram`, `swap`, `disk`, `temp`, `net`, `load`, `uptime` | `["cpu", "ram", "disk", "temp"]` |
| `mounts` | up to 8 absolute mount paths | `["/"]` |
| `ram-display` | `percent` or `absolute` | `percent` |
| `swap-display` | `percent` or `absolute` | `percent` |
| `disk-display` | `percent` or `absolute` | `percent` |
| `temp-warn` | °C limit, `20..=120` | `75` |
| `temp-error` | °C limit, `20..=120` | `85` |
| `temp-unit` | `celsius` or `fahrenheit` | `celsius` |

```toml
[panels.…]
left = ["lt.nyv.glimpse-monitor"]

[applets."lt.nyv.glimpse-monitor"]
options = { interval = 2, widgets = ["cpu", "ram", "disk", "temp"], mounts = ["/"], "ram-display" = "percent", "swap-display" = "percent", "disk-display" = "percent", "temp-unit" = "celsius", "temp-warn" = 75, "temp-error" = 85 }
```

Add `left = ["lt.nyv.glimpse-monitor"]` to a panel's applet zone.
