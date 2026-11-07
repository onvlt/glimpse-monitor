# glimpse-monitor

Applet id: `lt.nyv.glimpse-monitor`.

Requires Cargo. Cargo fetches the `glimpse-sdk` crate on first build.

Run `just dev`: it builds the release binary, loads `devconfig.toml` and attaches the
applet with its options and env. Options edits update it; source edits rebuild and restart it.

The justfile wraps the whole loop: `just dev`, `just test`, `just lint`, `just package`,
`just install`.

Add `center = ["lt.nyv.glimpse-monitor"]` to a panel's applet zone.
