//! GHDL Analyzer: a waveform viewer for GHDL simulations.
//!
//! The crate is split into a library (this file) and a small binary (`main.rs`), so
//! that integration tests in `tests/` can use the data layer directly.

pub mod app;
pub mod model;
pub mod ui;
