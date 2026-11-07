# Architecture guid

Developer guide: how the code is put together and where to change things.
User-facing documentation (options, install, panel config) lives in
[`README.md`](../README.md).

## The one-sentence version

A message-only process that spawns a thread to sample `sysinfo` on a timer,
publishes immutable snapshots of the readings into shared state, and re-renders
the whole widget tree from that state on every wake — options are validated once
at startup, everything else is a pure function of (`Snapshot`, `Config`).

## The two loops

There are exactly two threads, with asymmetric roles:

**The sampling thread** (one, in `monitor.rs`) owns _every_ sysinfo object
(`System`, `Disks`, `Components`, `Networks`, `Gpus` — one instance each, kept
for their whole life because sysinfo computes diffs across refreshes). Every
`interval` seconds it refreshes, converts the raw reads into a `Snapshot` (owned
plain data — no sysinfo types escape this file), stores it in `Arc<Shared>`, then
calls `waker.wake()` to ask for a rebuild. It never touches the UI and never
blocks on the panel.

**The SDK run loop** (in `glimpse_sdk::run`, owned by the SDK) talks the panel's
socket protocol. On every `Wake` (or panel event) it calls `App::build()`, which
reads the latest `Snapshot` and returns freshly built widgets. There are **no
messages flowing hardware→UI**: `enum Msg {}` is empty; wake-ups carry the data
implicitly via `Shared`.

So the key invariant: `build()` is deterministic and side-effect-free, and only
locks the snapshot mutex (`Shared::snapshot()` clone) — cheap, called once per
rebuild.

## Modules

- **`config.rs`** — the schema layer. `parse(&serde_json::Value) ->
Result<Config, Vec<String>>`. It collects _all_ errors in one pass (they are
  shown as one error chip tooltip). Per-key parse helpers each fall back to the
  default so the returned `Config` is always structurally valid, even when
  issues exist. Note the deliberate asymmetric naming: the on-wire key is
  `"widgets"` (compatibility with the panel/config and the earlier JS applet),
  while internal code says `Chip` — a chip is specifically _one configurable bar
  indicator bound to a metric_; `Widget` means any SDK tree node, so the two
  words never overlap.
- **`format.rs`** — pure text/unit adapters: binary vs decimal byte sizing with
  zero-trimming, `°C/°F` conversion, speed rates, uptime humanization, load
  numbers. The temperature scale itself is a config value (`temp-unit`) chosen
  in `config.rs`, never derived from the locale.
- **`snapshot.rs`** — the _data contract_ between sampler and renderer
  (`Snapshot`, `TempReading`, `NetSample`, …) plus the _policy_ that turns
  numbers into colors (`usage_tone`, `temp_tone` with the crit-scaled
  thresholds) and one aggregation rule (`net_samples`: skip loopback, rank
  busiest, cap 4). These are pure and heavily unit-tested.
- **`monitor.rs`** — the only place invoking sysinfo. Per tick: grouped sysinfo
  refreshes into `Sources` (kept under clippy's argument limit), reads →
  `Snapshot` → publish → wake. One-time setup does the CPU warm-up
  (`refresh_cpu_list` fills the per-core slots before `System::new()` leaves
  them empty, `refresh_cpu_usage` once so the first tick has real percentages),
  fills kernel/OS/hostname statics once, and reads network speeds from the
  _measured_ elapsed time (`Instant`), not the nominal interval, so sleep drift
  can't distort them.
- **`ui.rs`** — the pure renderer and the threshold→`Severity` mapping. It's the
  only place that imports SDK element builders. Chip texts/tooltips
  (`chip_text`, `chip_tooltip`, `chip_severity`) are exported pure functions —
  that's why chip behavior is unit-testable without any SDK session. Section
  functions follow the "missing data degrades to a placeholder or is omitted"
  rule: no swap → no swap row; no GPUs → no Graphics section; empty
  net/disks → placeholder.
- **`app.rs`** — the `glimpse_sdk::App` glue: parse options in `start` (spawn
  the thread only when options are valid — invalid options render an error chip
  instead), `update` unreachable, `build` = `ui::tree()`. The integration tests
  here drive the real SDK `Session` handshake and count the mounted tree —
  that's the closest thing to "does the panel accept this".

## How to extend it (the shape of a new widget)

Adding a metric — a bar chip plus a section — is the same walk every time,
bottom-up:

1. **Type** in `snapshot.rs` plus a pure helper if any aggregation/policy
   applies (test it).
2. **Read** it in `monitor.rs` (sysinfo statics are free; per-tick data lives on
   existing refreshers — prefer `refresh_memory_specifics`-style calls with
   `nothing()` plus only the fields you need), and put it on `Snapshot`.
3. **Format** helper in `format.rs` if the numbers become a new text shape
   (test it).
4. **Config**: add the `Chip` variant if it is a bar indicator (name in
   `Chip::from_name`, README table), or a new option key with its parse helper.
5. **Render** in `ui.rs`: chip arms in the three chip functions (all arms are
   exhaustive matches — the compiler helps), plus a section or row, honoring the
   degrade/omit rules.

Removal flows the same way inside-out. The tests are arranged so steps 1 and 3
can be done with zero SDK knowledge; only step 5 touches elements and step 2
touches sysinfo.

## Things that look odd on purpose

- **`Shared` is not an early-warning mutex field** — it is the single handoff
  point; everything flows through one `Snapshot` publish. If you find yourself
  wanting incremental channels, reconsider: freshness is bounded by the
  interval anyway.
- **`Snapshot` is `PartialEq` and unit-tested, but sampler glue itself is thin
  on tests** — sysinfo I/O cannot run hermetically in CI, so the sampler stays
  dumb glue and all decisions live in pure functions.
- **Text caps** — the panel shows 32 characters of indicator text; chip texts
  all fit, and popover text is trimmed at 256 by the panel, intentionally
  untouched here.
- **`Msg` is empty** — the first interactive feature (say, a refresh button)
  will add a variant, and the flow will need a decision: `Shared` for data,
  `Msg` for user intent.
- **Error chip instead of exiting on bad options** — matches the JS applet's
  contract: bad `panel.toml` options must never respawn-crash the applet under
  the panel's supervision.

## Where the risky API knowledge is concentrated

- `sysinfo` quirks live in `monitor.rs` only (single instances, warm-up,
  `nothing().with_*()` refresh kinds, delta-based speeds, composed
  `Component::label`).
- Element constraints (severity/tone enums, typography, which parent holds
  which child, text caps) live in `ui.rs` only.
- The wire/`Session` contract lives in `app.rs` tests only.

That separation is the main thing to preserve when modifying: three files that
must each stay the _only_ place its outside knowledge is interpreted.

## Workflow

```sh
just lint     # clippy, warnings denied
just test     # cargo test
just dev      # attach to the running panel, hot-reload on edits
just install  # build and install the applet for the user
```

## Dependency shape

The `glimpse-sdk` crate depends only on `serde` + `serde_json`; this applet adds
`sysinfo` with features limited to `system`, `disk`, `component`, `network` and
`gpu` (no process, user or windows-only pulls), so the binary stays lean and
compiles quickly.
