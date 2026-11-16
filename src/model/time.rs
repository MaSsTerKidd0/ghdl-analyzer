//! Time units, time formatting and the math behind the time axis.

use super::Time;

/// What one raw time unit means in seconds: `factor * 10^exponent`.
///
/// GHDL writes femtoseconds (`factor = 1, exponent = -15`) for GHW and FST;
/// VCD files declare it in their `$timescale` header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timescale {
    pub factor: u32,
    pub exponent: i8,
}

impl Default for Timescale {
    fn default() -> Self {
        Self::FEMTOSECONDS
    }
}

/// Display units, from smallest to largest. The number is the power of ten.
const UNITS: [(i8, &str); 7] = [
    (-18, "as"),
    (-15, "fs"),
    (-12, "ps"),
    (-9, "ns"),
    (-6, "us"),
    (-3, "ms"),
    (0, "s"),
];

impl Timescale {
    pub const FEMTOSECONDS: Timescale = Timescale {
        factor: 1,
        exponent: -15,
    };

    /// Convert a raw time to seconds (approximately; only used for display).
    pub fn to_seconds(self, t: f64) -> f64 {
        t * f64::from(self.factor) * 10f64.powi(i32::from(self.exponent))
    }

    /// Format a raw time with a readable unit, e.g. `12.5 ns`.
    ///
    /// The unit is chosen so the number is at least 1 (except for 0 itself,
    /// which is shown in the file's base unit).
    pub fn format(self, t: Time) -> String {
        self.format_f64(t as f64)
    }

    /// Same as [`Self::format`], for differences or fractional times.
    pub fn format_f64(self, t: f64) -> String {
        let seconds = self.to_seconds(t);
        let (exp, name) = if seconds == 0.0 {
            self.base_unit()
        } else {
            // The largest unit that keeps the number >= 1.
            UNITS
                .iter()
                .rev()
                .find(|(exp, _)| seconds.abs() >= 10f64.powi(i32::from(*exp)) * 0.999_999)
                .copied()
                .unwrap_or(UNITS[0])
        };
        let value = seconds / 10f64.powi(i32::from(exp));
        format!("{} {}", trim_number(value), name)
    }

    /// The display unit closest to the file's own unit.
    fn base_unit(self) -> (i8, &'static str) {
        UNITS
            .iter()
            .rev()
            .find(|(exp, _)| *exp <= self.exponent)
            .copied()
            .unwrap_or(UNITS[0])
    }
}

/// Print with up to 3 decimals and drop trailing zeros: `12.500` -> `12.5`, `3.000` -> `3`.
fn trim_number(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".to_string()
    } else {
        s.to_string()
    }
}

/// Pick a "nice" distance between axis ticks: 1, 2 or 5 times a power of ten,
/// and at least `min_step` raw units (the smallest step that keeps labels from overlapping).
pub fn nice_step(min_step: f64) -> Time {
    if min_step <= 1.0 {
        return 1;
    }
    let magnitude = 10f64.powf(min_step.log10().floor());
    let step = [1.0, 2.0, 5.0, 10.0]
        .iter()
        .map(|m| m * magnitude)
        .find(|&s| s >= min_step)
        .unwrap_or(10.0 * magnitude);
    step.round() as Time
}

/// The tick positions for the visible range `[start, end]`, given that labels need
/// `min_step` raw units of room.
pub fn ticks(start: f64, end: f64, min_step: f64) -> Vec<Time> {
    let step = nice_step(min_step);
    let first = (start.max(0.0) / step as f64).ceil() as Time * step;
    let mut out = Vec::new();
    let mut t = first;
    while (t as f64) <= end && out.len() < 1000 {
        out.push(t);
        t += step;
    }
    out
}

/// The visible part of the time line: what range of time maps onto the waveform's width.
///
/// Times are `f64` here so zooming in very far (less than one raw unit per pixel)
/// still behaves smoothly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub start: f64,
    pub end: f64,
}

impl Viewport {
    /// Smallest visible span, in raw units: we never zoom closer than this.
    const MIN_SPAN: f64 = 4.0;

    pub fn new(start: f64, end: f64) -> Self {
        let mut v = Self { start, end };
        v.fix();
        v
    }

    pub fn span(&self) -> f64 {
        self.end - self.start
    }

    /// Time -> x offset in pixels from the left edge, for a waveform `width` pixels wide.
    pub fn to_x(&self, t: f64, width: f32) -> f32 {
        ((t - self.start) / self.span() * f64::from(width)) as f32
    }

    /// x offset in pixels -> time.
    pub fn to_time(&self, x: f32, width: f32) -> f64 {
        self.start + f64::from(x) / f64::from(width) * self.span()
    }

    /// Zoom by `factor` (< 1 zooms in) keeping time `anchor` at the same screen position.
    pub fn zoom(&mut self, factor: f64, anchor: f64) {
        self.start = anchor - (anchor - self.start) * factor;
        self.end = anchor + (self.end - anchor) * factor;
        self.fix();
    }

    /// Move the view by `dt` raw units.
    pub fn pan(&mut self, dt: f64) {
        self.start += dt;
        self.end += dt;
        self.fix();
    }

    /// Keep the view sane: never before time 0 and never narrower than `MIN_SPAN`.
    fn fix(&mut self) {
        if self.span() < Self::MIN_SPAN {
            let mid = (self.start + self.end) / 2.0;
            self.start = mid - Self::MIN_SPAN / 2.0;
            self.end = mid + Self::MIN_SPAN / 2.0;
        }
        if self.start < 0.0 {
            self.end -= self.start;
            self.start = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NS: Timescale = Timescale {
        factor: 1,
        exponent: -9,
    };

    #[test]
    fn formatting() {
        let fs = Timescale::FEMTOSECONDS;
        assert_eq!(fs.format(0), "0 fs");
        assert_eq!(fs.format(500), "500 fs");
        assert_eq!(fs.format(1_000), "1 ps");
        assert_eq!(fs.format(12_500_000), "12.5 ns");
        assert_eq!(fs.format(3_000_000_000_000), "3 ms");
        assert_eq!(NS.format(0), "0 ns");
        assert_eq!(NS.format(1500), "1.5 us");
        let ten_ps = Timescale {
            factor: 10,
            exponent: -12,
        };
        assert_eq!(ten_ps.format(5), "50 ps");
    }

    #[test]
    fn nice_steps() {
        assert_eq!(nice_step(0.3), 1);
        assert_eq!(nice_step(1.5), 2);
        assert_eq!(nice_step(3.0), 5);
        assert_eq!(nice_step(7.0), 10);
        assert_eq!(nice_step(130.0), 200);
        assert_eq!(nice_step(1000.0), 1000);
    }

    #[test]
    fn tick_positions() {
        assert_eq!(ticks(0.0, 50.0, 15.0), vec![0, 20, 40]);
        assert_eq!(ticks(25.0, 60.0, 9.0), vec![30, 40, 50, 60]);
    }

    #[test]
    fn viewport_mapping() {
        let v = Viewport::new(100.0, 200.0);
        assert_eq!(v.to_x(150.0, 1000.0), 500.0);
        assert_eq!(v.to_time(250.0, 1000.0), 125.0);
    }

    #[test]
    fn viewport_zoom_keeps_anchor() {
        let mut v = Viewport::new(0.0, 1000.0);
        v.zoom(0.5, 400.0);
        assert_eq!(v, Viewport::new(200.0, 700.0));
        // Anchor stays at the same relative position (40%).
        assert!((v.to_x(400.0, 100.0) - 40.0).abs() < 1e-4);
    }

    #[test]
    fn viewport_limits() {
        let mut v = Viewport::new(0.0, 100.0);
        v.pan(-50.0);
        assert_eq!(v, Viewport::new(0.0, 100.0));
        v.zoom(0.0001, 50.0);
        assert!(v.span() >= 4.0);
    }
}
