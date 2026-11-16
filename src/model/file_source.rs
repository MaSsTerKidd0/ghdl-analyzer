//! [`WaveSource`] for waveform files (VCD, FST, GHW), built on the `wellen` crate.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use thiserror::Error;
use wellen::simple::Waveform;
use wellen::{Hierarchy, ScopeRef, SignalEncoding, SignalRef, SignalValueRef, VarRef};

use super::{
    ScopeNode, Segment, SegmentValue, SignalId, SignalInfo, SignalKind, Time, Timescale, Value,
    WaveSource,
};

#[derive(Debug, Error)]
pub enum LoadError {
    #[error("could not open {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("could not read {path}: {source}")]
    Wellen {
        path: PathBuf,
        #[source]
        source: wellen::WellenError,
    },
}

/// A waveform file opened with wellen.
pub struct FileSource {
    path: PathBuf,
    waveform: Waveform,
    roots: Vec<ScopeNode>,
    signals: Vec<SignalInfo>,
    /// For each of our signals, the wellen signal holding its data.
    ///
    /// Several variables can share one wellen `SignalRef` (e.g. a port and the
    /// signal it is connected to), which is why we keep this mapping.
    signal_refs: Vec<SignalRef>,
    /// For enumeration signals: the bit pattern -> literal name table.
    enum_names: HashMap<SignalId, HashMap<String, String>>,
    timescale: Timescale,
}

impl FileSource {
    /// Open a file; the format is detected from its content.
    ///
    /// This reads the whole file header and body, which can take a while for big
    /// files, so the app calls it on a background thread.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, LoadError> {
        let path = path.as_ref().to_path_buf();
        // Open the file ourselves: `wellen::simple::read` panics on a missing file,
        // while we want a normal error the UI can show.
        let file = std::fs::File::open(&path).map_err(|source| LoadError::Io {
            path: path.clone(),
            source,
        })?;
        let waveform =
            wellen::simple::read_from_reader(std::io::BufReader::new(file)).map_err(|source| {
                LoadError::Wellen {
                    path: path.clone(),
                    source,
                }
            })?;

        let mut builder = TreeBuilder::default();
        let h = waveform.hierarchy();
        // Empty scopes (GHDL lists every VHDL package used, e.g. `std_logic_1164`)
        // only clutter the tree, so drop them.
        let roots = h
            .scopes()
            .map(|s| builder.scope(h, s))
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        // Signals declared outside of any scope (rare, but legal in VCD).
        let mut top_vars: Vec<SignalId> = h.vars().map(|v| builder.var(h, v, "")).collect();
        let mut roots = roots;
        if !top_vars.is_empty() {
            top_vars.sort_by_key(|&id| builder.signals[id].name.clone());
            roots.insert(
                0,
                ScopeNode {
                    name: "(top)".into(),
                    scopes: vec![],
                    signals: top_vars,
                },
            );
        }

        let timescale = h
            .timescale()
            .and_then(|ts| {
                Some(Timescale {
                    factor: ts.factor,
                    exponent: ts.unit.to_exponent()?,
                })
            })
            .unwrap_or_default();

        Ok(Self {
            path,
            waveform,
            roots,
            signals: builder.signals,
            signal_refs: builder.signal_refs,
            enum_names: builder.enum_names,
            timescale,
        })
    }

    fn signal(&self, id: SignalId) -> Option<&wellen::Signal> {
        self.waveform.get_signal(*self.signal_refs.get(id)?)
    }

    /// Convert wellen's borrowed value into our owned [`Value`].
    fn convert(&self, id: SignalId, v: SignalValueRef<'_>) -> Value {
        match v {
            SignalValueRef::BitVec(bits) => {
                let s = bits.bit_string();
                // GHW stores enumerations as the index of the literal; show the name instead.
                match self.enum_names.get(&id).and_then(|names| names.get(&s)) {
                    Some(name) => Value::Text(name.clone()),
                    None => Value::Bits(s),
                }
            }
            SignalValueRef::String(s) => Value::Text(s.to_string()),
            SignalValueRef::Real(r) => Value::Real(r),
            SignalValueRef::Event => Value::Event,
        }
    }
}

/// Helper that walks the wellen hierarchy and builds our own tree.
#[derive(Default)]
struct TreeBuilder {
    signals: Vec<SignalInfo>,
    signal_refs: Vec<SignalRef>,
    enum_names: HashMap<SignalId, HashMap<String, String>>,
}

impl TreeBuilder {
    fn scope(&mut self, h: &Hierarchy, scope_ref: ScopeRef) -> ScopeNode {
        let scope = &h[scope_ref];
        let path = scope.full_name(h);
        let mut signals: Vec<SignalId> = scope.vars(h).map(|v| self.var(h, v, &path)).collect();
        signals.sort_by_key(|&id| self.signals[id].name.to_lowercase());
        ScopeNode {
            name: scope.name(h).to_string(),
            // Recursion: each child scope builds its own subtree.
            scopes: scope
                .scopes(h)
                .map(|s| self.scope(h, s))
                .filter(|s| !s.is_empty())
                .collect(),
            signals,
        }
    }

    fn var(&mut self, h: &Hierarchy, var_ref: VarRef, scope_path: &str) -> SignalId {
        let var = &h[var_ref];
        let id = self.signals.len();

        // VCD often stores the range separately from the name: show `data[7:0]`.
        let mut name = var.name(h).to_string();
        if let Some(index) = var.index()
            && !name.ends_with(']')
        {
            if index.msb() == index.lsb() {
                name.push_str(&format!("[{}]", index.msb()));
            } else {
                name.push_str(&format!("[{}:{}]", index.msb(), index.lsb()));
            }
        }

        let kind = if let Some((_type_name, literals)) = var.enum_type(h) {
            let table = literals
                .into_iter()
                .map(|(bits, lit)| (bits.to_string(), lit.to_string()))
                .collect();
            self.enum_names.insert(id, table);
            SignalKind::Enum
        } else {
            match var.signal_encoding(h) {
                SignalEncoding::BitVector(1) => SignalKind::Bit,
                SignalEncoding::BitVector(w) => SignalKind::Vector(w),
                SignalEncoding::Real => SignalKind::Real,
                SignalEncoding::String => SignalKind::Text,
            }
        };

        let path = if scope_path.is_empty() {
            name.clone()
        } else {
            format!("{scope_path}.{name}")
        };
        self.signals.push(SignalInfo { name, path, kind });
        self.signal_refs.push(var.signal_ref());
        id
    }
}

impl WaveSource for FileSource {
    fn description(&self) -> String {
        let name = self
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.display().to_string());
        let format = format!("{:?}", self.waveform.hierarchy().file_format());
        format!("{name} ({format})")
    }

    fn hierarchy(&self) -> &[ScopeNode] {
        &self.roots
    }

    fn signals(&self) -> &[SignalInfo] {
        &self.signals
    }

    fn timescale(&self) -> Timescale {
        self.timescale
    }

    fn time_range(&self) -> (Time, Time) {
        let table = self.waveform.time_table();
        (
            table.first().copied().unwrap_or(0),
            table.last().copied().unwrap_or(0),
        )
    }

    fn load(&mut self, ids: &[SignalId]) {
        let refs: Vec<SignalRef> = ids
            .iter()
            .filter_map(|&id| self.signal_refs.get(id).copied())
            .collect();
        self.waveform.load_signals_multi_threaded(&refs);
    }

    fn value_at(&self, id: SignalId, t: Time) -> Option<Value> {
        let signal = self.signal(id)?;
        let changes = Changes::new(signal, self.waveform.time_table());
        let p = changes.count_at_or_before(t);
        if p == 0 {
            return None;
        }
        Some(self.convert(id, changes.value(p - 1)))
    }

    fn segments(&self, id: SignalId, t0: Time, t1: Time, resolution: Time) -> Vec<Segment> {
        let Some(signal) = self.signal(id) else {
            return Vec::new();
        };
        let changes = Changes::new(signal, self.waveform.time_table());
        let resolution = resolution.max(1);
        let n = changes.len();
        let mut out = Vec::new();

        // Start with the change that is active at t0 (it may lie before the window).
        let mut p = changes.count_at_or_before(t0).saturating_sub(1);
        while p < n && changes.time(p) <= t1 {
            let t = changes.time(p);
            // With delta cycles several changes share the same time; the last one wins.
            let next = changes.count_at_or_before(t);
            let last_at_t = next - 1;

            if next < n && changes.time(next) < t.saturating_add(resolution) && t >= t0 {
                // Many changes within one resolution step: skip them all with a binary
                // search instead of visiting each one. This is what keeps zoomed-out
                // views of huge files fast: the cost is per pixel, not per change.
                let skip_to = changes.count_before(t.saturating_add(resolution));
                out.push(Segment {
                    start: t,
                    value: SegmentValue::Dense,
                });
                // Continue from the last change inside the dense run; its value is
                // what the signal holds afterwards.
                p = skip_to - 1;
                if p <= last_at_t {
                    p = next;
                }
            } else {
                out.push(Segment {
                    start: t,
                    value: SegmentValue::Stable(self.convert(id, changes.value(last_at_t))),
                });
                p = next;
            }
        }
        out
    }

    fn next_change(&self, id: SignalId, t: Time) -> Option<Time> {
        let signal = self.signal(id)?;
        let changes = Changes::new(signal, self.waveform.time_table());
        let p = changes.count_at_or_before(t);
        (p < changes.len()).then(|| changes.time(p))
    }

    fn prev_change(&self, id: SignalId, t: Time) -> Option<Time> {
        let signal = self.signal(id)?;
        let changes = Changes::new(signal, self.waveform.time_table());
        let p = changes.count_before(t);
        (p > 0).then(|| changes.time(p - 1))
    }
}

/// A small view over one loaded signal that answers "when" questions with binary search.
///
/// wellen stores, per signal, a sorted list of indices into the global time table,
/// and the values at the same positions. This struct borrows both (note the lifetime
/// `'a`: it cannot outlive the signal it was created from).
struct Changes<'a> {
    signal: &'a wellen::Signal,
    time_indices: &'a [u32],
    time_table: &'a [Time],
}

impl<'a> Changes<'a> {
    fn new(signal: &'a wellen::Signal, time_table: &'a [Time]) -> Self {
        Self {
            signal,
            time_indices: signal.time_indices(),
            time_table,
        }
    }

    fn len(&self) -> usize {
        self.time_indices.len()
    }

    fn time(&self, p: usize) -> Time {
        self.time_table[self.time_indices[p] as usize]
    }

    fn value(&self, p: usize) -> SignalValueRef<'a> {
        self.signal.data().get_value_at(p)
    }

    /// Number of changes with time <= t.
    fn count_at_or_before(&self, t: Time) -> usize {
        self.time_indices
            .partition_point(|&i| self.time_table[i as usize] <= t)
    }

    /// Number of changes with time < t.
    fn count_before(&self, t: Time) -> usize {
        self.time_indices
            .partition_point(|&i| self.time_table[i as usize] < t)
    }
}
