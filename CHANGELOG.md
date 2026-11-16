# Changelog

## v0.1.0

First release: a viewer for waveform files produced by [GHDL](https://github.com/ghdl/ghdl).

- Opens GHW, FST and VCD files (tested with files from GHDL 6.0.0).
- Design hierarchy tree with search filter.
- Digital traces with X/U/Z rendering; buses with binary, hex, unsigned, signed and ASCII radix.
- VHDL enumeration names from GHW files.
- Zoom, pan, cursor and marker with time difference, edge snapping, next/previous edge.
- Stays fast on large files; files load in the background.
- `--show TEXT` to add matching signals at start-up.

Downloads: Linux (x86_64), Windows (x86_64), macOS (Apple Silicon and Intel). On Linux the
GUI needs `libxkbcommon-x11-0` and OpenGL/Vulkan libraries.
