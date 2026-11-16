#!/usr/bin/env sh
# Regenerate the sample waveforms from counter_tb.vhd with GHDL.
# Usage: examples/vhdl/generate.sh   (run from the repository root)
set -e
if ! command -v ghdl >/dev/null 2>&1; then
  echo "GHDL is not installed; see https://github.com/ghdl/ghdl" >&2
  exit 1
fi
work=$(mktemp -d)
here=$(cd "$(dirname "$0")" && pwd)
out="$here/.."
cd "$work"
ghdl -a --std=08 "$here/counter_tb.vhd"
ghdl -e --std=08 counter_tb
# One run per format: some GHDL versions fail when several dump options are combined.
for opt in "--wave=$out/counter_tb.ghw" "--fst=$out/counter_tb.fst" "--vcd=$out/counter_tb.vcd"; do
  ghdl -r --std=08 counter_tb --stop-time=3us "$opt"
done
echo "Wrote $out/counter_tb.ghw, counter_tb.fst and counter_tb.vcd"
