//! The data layer: everything the viewer knows about a waveform, independent of the GUI.
//!
//! The UI only ever talks to the [`WaveSource`] trait. Today the only implementation is
//! [`file_source::FileSource`] (VCD/FST/GHW files via the `wellen` crate). In Phase 2 a
//! `LiveSource` will implement the same trait and fill itself from a running GHDL
//! simulation, and the UI will not need to change.

pub mod file_source;
pub mod time;
pub mod value;

pub use time::Timescale;
pub use value::{Radix, Value};

/// Raw simulation time, in units of the file's [`Timescale`].
///
/// A plain `u64` alias (not a newtype) keeps the arithmetic in the drawing code simple.
pub type Time = u64;

/// Our own index for a signal. It indexes [`WaveSource::signals`].
///
/// We use our own id instead of exposing `wellen::VarRef`, so that the UI
/// does not depend on wellen at all (a live source will not have wellen ids).
pub type SignalId = usize;

/// What kind of values a signal carries; decides how it is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalKind {
    /// A single bit (`std_logic`, `bit`, `boolean`): drawn as a digital trace.
    Bit,
    /// A multi-bit vector of the given width: drawn as a bus.
    Vector(u32),
    /// A VHDL enumeration (e.g. a state machine type): drawn as a bus showing the names.
    Enum,
    /// A floating point value.
    Real,
    /// Free-form text.
    Text,
}

impl SignalKind {
    /// Only plain bit vectors can be shown in a different radix.
    pub fn supports_radix(self) -> bool {
        matches!(self, SignalKind::Vector(_))
    }
}

/// Static information about a signal.
#[derive(Debug, Clone)]
pub struct SignalInfo {
    /// Short name, as shown in the tree (e.g. `count`).
    pub name: String,
    /// Full hierarchical path (e.g. `counter_tb.dut.count`).
    pub path: String,
    pub kind: SignalKind,
}

/// One node of the design hierarchy (a VHDL entity instance, process, record...).
#[derive(Debug, Clone, Default)]
pub struct ScopeNode {
    pub name: String,
    pub scopes: Vec<ScopeNode>,
    pub signals: Vec<SignalId>,
}

impl ScopeNode {
    /// True if neither this scope nor any scope below it has signals.
    pub fn is_empty(&self) -> bool {
        self.signals.is_empty() && self.scopes.iter().all(ScopeNode::is_empty)
    }
}

/// One piece of a trace as it should be drawn.
///
/// A segment starts at `start` and lasts until the start of the next segment
/// (or the end of the requested window for the last one).
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub start: Time,
    pub value: SegmentValue,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SegmentValue {
    /// The signal holds this value for the whole segment.
    Stable(Value),
    /// The signal changes many times inside a span narrower than the drawing resolution.
    /// Drawing every change would be both slow and unreadable, so we draw one block.
    Dense,
}

/// Anything that can provide waveform data to the viewer.
///
/// `Send` is required because files are loaded on a background thread and then
/// handed to the UI thread (see `app.rs`).
pub trait WaveSource: Send {
    /// A short description for the title bar / status line (e.g. the file name).
    fn description(&self) -> String;

    /// The root scopes of the design.
    fn hierarchy(&self) -> &[ScopeNode];

    /// All signals; a [`SignalId`] indexes this slice.
    fn signals(&self) -> &[SignalInfo];

    fn timescale(&self) -> Timescale;

    /// First and last time stamps that have data.
    fn time_range(&self) -> (Time, Time);

    /// Make sure the value changes of these signals are in memory.
    ///
    /// Takes `&mut self` because a file source decodes signals lazily, the first
    /// time they are needed. Calling it again for an already loaded signal is cheap.
    fn load(&mut self, ids: &[SignalId]);

    /// The value of a signal at time `t` (the last change at or before `t`).
    /// `None` if the signal has no value yet at `t` or has not been loaded.
    fn value_at(&self, id: SignalId, t: Time) -> Option<Value>;

    /// The segments to draw for the window `[t0, t1]`.
    ///
    /// `resolution` is the smallest time span worth drawing separately (typically the
    /// duration of one pixel). Runs of changes closer together than that are merged
    /// into a [`SegmentValue::Dense`] segment, which keeps drawing fast even when a
    /// signal has millions of changes in the visible window.
    fn segments(&self, id: SignalId, t0: Time, t1: Time, resolution: Time) -> Vec<Segment>;

    /// Time of the first change strictly after `t`.
    fn next_change(&self, id: SignalId, t: Time) -> Option<Time>;

    /// Time of the last change strictly before `t`.
    fn prev_change(&self, id: SignalId, t: Time) -> Option<Time>;
}
