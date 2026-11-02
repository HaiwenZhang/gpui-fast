# Contributing to GPUI Fast

Issues are not open for now. Pull requests are welcome for fixes and
improvements to Retained Mode, and for needs that are clearly reasonable and
pressing and keep gpui-fast in step with GPUI upstream. Anything else belongs
in [GPUI upstream](https://github.com/zed-industries/zed); see the
[README](README.md#contributing).

## Staying in step with upstream

gpui-fast tracks Zed's `crates/gpui` and the Zed crates it depends on. The
upstream commit it is based on is recorded in [`UPSTREAM`](UPSTREAM); the
source layout and crate paths are upstream's, so any file compares path for
path with a Zed checkout.

Local changes are stored outside the upstream directories: performance code in
`fast/`, feature adapters and CAD in `extensions/`, and connection points in
`integration/patches/`. The build workspace is generated under
`target/integration/workspace/`. Never edit that generated source or the pristine
upstream source for local features.

`python3 script/check-upstream` enforces both pristine upstream source and small
hooks in the assembled workspace. See [upstream-sync.md](docs/upstream-sync.md)
for ownership rules, editor setup, patch maintenance and upstream updates.

## Building and testing

```sh
python3 script/cargo run -p gpui --example hello_world
python3 script/cargo test -p gpui --features test-support
script/check-upstream
```

## Measuring

Always measure a release build (`--release`): a debug build's costs are
dominated by what the optimizer removes, and say nothing about gpui-fast.

### The showcase

`python3 script/cargo run -p gpui_perf --release` opens the showcase, a component gallery
laid out the way GPUI Kit's story gallery is: a sidebar of pages, a scrolled
page of component sections, and a data table refreshed by a timer. It is the
first place to look at a performance problem found in an application, without
setting one up.

- The toolbar picks what scrolls itself — `Off`, `Sidebar`, `Page`, `Table`
  (keys `1`–`4`) — and switches `Refresh data` (`R`), which updates table
  rows every 33 ms, and `Retained views` (`V`). The arrow keys move through
  the sidebar.
- The status bar shows, every half second: frames per second, the CPU of the
  whole process and of the main thread, on macOS the share of that CPU time
  spent on performance cores, the process's memory (resident on Linux, its
  footprint on macOS), what the main thread spent per frame
  and on build, prepaint, layout (Taffy's share of prepaint) and paint, and how
  many views were built and reused per frame.

`--auto` runs every scenario — idle, scrolling the sidebar, the page and the
table, refreshing the table — with retained views on and then off, prints
what each cost per frame, and quits:

```sh
python3 script/cargo run -p gpui_perf --release -- --auto
python3 script/cargo run -p gpui_perf --release -- --auto --only ScrollPage --retention on --frames 1000
```

On macOS, `--auto` holds the CPU's clock up while it measures, with a helper
process spinning on a performance core, as the performance governor does on
Linux. Without it macOS runs a lightly loaded process on efficiency cores and
at low clocks, so the less work a frame does, the longer each instruction of
it takes: CPU time per frame differs twofold from one run of the same build
to the next, and retained views can report as much CPU time as drawing from
scratch while retiring 40% fewer instructions. `--no-hold-clock` measures
without the helper, as an application runs. The report also gives the main
thread's instructions per frame (`instr p50`), which do not depend on the
clock, and the share of the process's CPU time spent on performance cores
(`p-cores`). The window has to be on screen: with the display asleep or the
screen locked, no frames are drawn and `--auto` waits.

To profile one scenario, run it for longer under a profiler, for instance
`samply record ./target/release/gpui_perf --auto --only ScrollPage --frames 5000`.

### Comparing with upstream GPUI

The `upstream` feature builds the same showcase against upstream GPUI, the
`gpui-pre` snapshot GPUI Kit pins (`gpui-pre` and `gpui-pre-platform`
`=0.3.7`), instead of this repository's, so both can run the same screens
and the same `--auto` scenarios:

```sh
python3 script/cargo run -p gpui_perf --release -- --auto                       # gpui-fast
python3 script/cargo run -p gpui_perf --release --features upstream -- --auto   # upstream
```

Upstream has no retained views and does not time the phases of a frame, so
its showcase hides the `Retained views` switch, and its status bar and report
show frame rate and CPU only (`-` in the other columns). Add
`--no-default-features` to the upstream build to skip building this
repository's GPUI. `--headless` runs on gpui-fast only.

### Headless scenarios

`--headless` measures simulated screens — forms, lists, tables, settings, a
docked trading workspace — without a window, with real text shaping,
retained and not, and `--verify` checks that both paint the same quads,
text, icons and images on every frame:

```sh
python3 script/cargo run -p gpui_perf --release -- --headless
python3 script/cargo run -p gpui_perf --release -- --headless --scenario table --frames 200
python3 script/cargo run -p gpui_perf --release -- --headless --verify
python3 script/cargo run -p gpui_perf --release -- --headless --list
```

### Other benchmarks

```sh
# A dashboard of panel views in a real window, 2 of 60 panels changing every frame
python3 script/cargo run -p gpui_perf --example views_frames --release -- 60 64 2
GPUI_VIEW_RETENTION=0 python3 script/cargo run -p gpui_perf --example views_frames --release -- 60 64 2
# 60 panel views of 64 labels, headless
python3 script/cargo test -p gpui --lib --release retained_bench -- --ignored --nocapture
```

[`docs/retained-mode.md`](docs/retained-mode.md) describes how retained mode
works, and how it is verified and measured.

A retained frame has to be the frame that drawing from scratch would have
produced. An oracle test (`fast/gpui/src/tests/oracle.rs`) drives two
windows through the same random history, one drawing incrementally and one
from scratch, and requires every frame to match; `gpui_perf --headless
--verify` does the same for whole simulated screens.

## Repository layout

```
crates/                     pristine GPUI, renderers and support crates from Zed
tooling/perf/               pristine test-perf harness from Zed
fast/<crate>/src/           owned performance implementations and tests
extensions/                 composition/GPU adapters and the CAD package
apps/gpui_perf/             owned showcase and performance tooling
integration/                connection patches, source mappings and Cargo inputs
target/integration/workspace/  generated build workspace
```
