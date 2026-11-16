# Roadmap

## Phase 1: recorded waveforms (done)

Viewer for GHW, FST and VCD files: hierarchy, traces, buses, radix, zoom/pan, cursor and
marker, edge navigation, dense-change rendering.

### Small follow-ups

- Save and load the view (selected signals, radix, zoom) to a file.
- Signal groups and dividers.
- Search the trace for a value (e.g. "next time `state = WRAP`").
- Expand a bus into its individual bits.
- Color per signal.
- Show analog (`real`) signals as a line plot.
- Reload the file when it changes on disk (re-run the simulation, the view updates).

## Phase 2: live view of a running simulation

Watch a GHDL simulation while it runs.

- A `LiveSource` implementing the existing `WaveSource` trait, filled from a socket.
- On the GHDL side, either:
  - a **VPI plugin** loaded with `ghdl -r tb --vpi=./ghdl_analyzer_vpi.so`: it registers
    value-change callbacks and streams them to the viewer. No changes to GHDL needed. Or
  - a **GHDL runtime module** (like GHDL's VCD/FST writers in `src/grt/`) adding a
    `--live=host:port` option.
- A small binary protocol: hierarchy first, then (time, signal, value) changes.
- The viewer records everything it receives, so you can still scroll back in time.

## Phase 3: logic-analyzer controls

- Triggers: edge, value or pattern conditions, combined with AND/OR.
- Capture a window of time before and after the trigger.
- Pause, single-step (cycle or delta) and continue the simulation from the GUI.
- Protocol decoders as plugins: UART, SPI, I²C.
