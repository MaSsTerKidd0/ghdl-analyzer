//! Everything that draws on screen.
//!
//! [`WaveView`] owns the state of the main area (which signals are shown, zoom level,
//! cursors) and splits it into two columns that share the same rows:
//!
//! ```text
//! +-----------------------+--------------------------------------+
//! | name          value   |  time axis                           |  <- AXIS_HEIGHT
//! +-----------------------+--------------------------------------+
//! | clk               1   |  _|‾|_|‾|_|‾|_                       |  <- ROW_HEIGHT each
//! | count[7:0]       0A   |  <09><0A><0B>                        |
//! +-----------------------+--------------------------------------+
//!   signal_list.rs           waveform.rs
//! ```
//!
//! Both columns are painted by hand (instead of with egui widgets) so their rows line
//! up exactly and we stay in full control of scrolling and the mouse wheel.

pub mod hierarchy;
mod signal_list;
mod waveform;

use egui::{Color32, Key, Rect, Sense, Ui, Vec2, pos2};

use crate::model::time::Viewport;
use crate::model::{Radix, SignalId, Time, WaveSource};

pub const AXIS_HEIGHT: f32 = 28.0;
pub const ROW_HEIGHT: f32 = 26.0;
const NAMES_WIDTH: f32 = 300.0;

/// One signal displayed in the waveform view.
#[derive(Debug, Clone)]
pub struct Trace {
    pub id: SignalId,
    pub radix: Radix,
}

/// State of the main waveform area.
pub struct WaveView {
    pub traces: Vec<Trace>,
    pub selected: Option<usize>,
    pub viewport: Viewport,
    /// Primary cursor: set by clicking. Values in the name column are read here.
    pub cursor: Option<Time>,
    /// Secondary marker, used to measure time differences.
    pub marker: Option<Time>,
    /// Vertical scroll of the rows, in pixels.
    scroll_y: f32,
    /// The trace being dragged to a new position in the list, if any.
    dragging: Option<usize>,
    /// Width of the waveform column at the last frame (needed by keyboard zoom).
    last_wave_width: f32,
}

impl WaveView {
    pub fn new(time_range: (Time, Time)) -> Self {
        let mut view = Self {
            traces: Vec::new(),
            selected: None,
            viewport: Viewport::new(0.0, 1.0),
            cursor: None,
            marker: None,
            scroll_y: 0.0,
            dragging: None,
            last_wave_width: 800.0,
        };
        view.zoom_to_fit(time_range);
        view
    }

    pub fn zoom_to_fit(&mut self, (start, end): (Time, Time)) {
        // A little room on the right so the last change is visible.
        let span = ((end - start) as f64).max(1.0);
        self.viewport = Viewport::new(start as f64, end as f64 + span * 0.02);
    }

    /// Add signals to the view (signals already shown are added again on purpose:
    /// it can be handy to see one signal in two radixes).
    pub fn add_signals(&mut self, source: &mut dyn WaveSource, ids: &[SignalId]) {
        source.load(ids);
        self.traces.extend(ids.iter().map(|&id| Trace {
            id,
            radix: Radix::default(),
        }));
        if self.selected.is_none() && !self.traces.is_empty() {
            self.selected = Some(self.traces.len() - 1);
        }
    }

    pub fn remove_selected(&mut self) {
        if let Some(i) = self.selected
            && i < self.traces.len()
        {
            self.traces.remove(i);
            self.selected = if self.traces.is_empty() {
                None
            } else {
                Some(i.min(self.traces.len() - 1))
            };
        }
    }

    pub fn zoom_in(&mut self) {
        let anchor = self.zoom_anchor();
        self.viewport.zoom(0.5, anchor);
    }

    pub fn zoom_out(&mut self) {
        let anchor = self.zoom_anchor();
        self.viewport.zoom(2.0, anchor);
    }

    /// Keyboard and menu zoom keeps the cursor in place if it is visible, else the center.
    fn zoom_anchor(&self) -> f64 {
        let vp = self.viewport;
        match self.cursor {
            Some(c) if (c as f64) >= vp.start && (c as f64) <= vp.end => c as f64,
            _ => (vp.start + vp.end) / 2.0,
        }
    }

    /// Move the cursor to the next (`forward`) or previous change of the selected signal,
    /// and scroll the view if the new position is off screen.
    pub fn jump_to_edge(&mut self, source: &dyn WaveSource, forward: bool) {
        let Some(trace) = self.selected.and_then(|i| self.traces.get(i)) else {
            return;
        };
        let from = self.cursor.unwrap_or(self.viewport.start.max(0.0) as Time);
        let target = if forward {
            source.next_change(trace.id, from)
        } else {
            source.prev_change(trace.id, from)
        };
        if let Some(t) = target {
            self.cursor = Some(t);
            let tf = t as f64;
            if tf < self.viewport.start || tf > self.viewport.end {
                let half = self.viewport.span() / 2.0;
                self.viewport.pan(tf - half - self.viewport.start);
            }
        }
    }

    /// Keyboard shortcuts. Ignored while a text field (e.g. the filter) has focus.
    pub fn handle_keys(&mut self, ui: &Ui, source: &dyn WaveSource) {
        if ui.ctx().memory(|m| m.focused().is_some()) {
            return;
        }
        let pressed = |key| ui.input(|i| i.key_pressed(key));
        let shift = ui.input(|i| i.modifiers.shift);

        if pressed(Key::F) {
            self.zoom_to_fit(source.time_range());
        }
        if pressed(Key::Plus) || pressed(Key::Equals) {
            self.zoom_in();
        }
        if pressed(Key::Minus) {
            self.zoom_out();
        }
        let pan_step = self.viewport.span() * 0.25;
        if pressed(Key::ArrowLeft) && !shift {
            self.viewport.pan(-pan_step);
        }
        if pressed(Key::ArrowRight) && !shift {
            self.viewport.pan(pan_step);
        }
        if pressed(Key::N) || (shift && pressed(Key::ArrowRight)) {
            self.jump_to_edge(source, true);
        }
        if pressed(Key::P) || (shift && pressed(Key::ArrowLeft)) {
            self.jump_to_edge(source, false);
        }
        if pressed(Key::ArrowUp) {
            self.selected = match self.selected {
                Some(i) if i > 0 => Some(i - 1),
                other => other.or((!self.traces.is_empty()).then_some(0)),
            };
        }
        if pressed(Key::ArrowDown) {
            self.selected = match self.selected {
                Some(i) if i + 1 < self.traces.len() => Some(i + 1),
                other => other.or((!self.traces.is_empty()).then_some(0)),
            };
        }
        if pressed(Key::Delete) || pressed(Key::Backspace) {
            self.remove_selected();
        }
        if pressed(Key::M) {
            self.marker = self.cursor;
        }
        if pressed(Key::Home) {
            let span = self.viewport.span();
            self.viewport = Viewport::new(0.0, span);
        }
        if pressed(Key::End) {
            let span = self.viewport.span();
            let end = source.time_range().1 as f64 + span * 0.02;
            self.viewport = Viewport::new((end - span).max(0.0), end.max(span));
        }
    }

    /// Draw the whole waveform area into the remaining space of `ui`.
    pub fn ui(&mut self, ui: &mut Ui, source: &dyn WaveSource) {
        let full = ui.available_rect_before_wrap();
        // Claim the whole area so egui knows it is used.
        ui.allocate_rect(full, Sense::hover());

        let names_width = NAMES_WIDTH.min(full.width() * 0.4);
        let names = Rect::from_min_max(full.min, pos2(full.left() + names_width, full.bottom()));
        let wave = Rect::from_min_max(pos2(names.right() + 1.0, full.top()), full.max);
        self.last_wave_width = wave.width();

        // Vertical scrolling with the mouse wheel over the names column.
        let rows_height = full.height() - AXIS_HEIGHT;
        let content_height = self.traces.len() as f32 * ROW_HEIGHT;
        if ui.rect_contains_pointer(names) {
            let dy = ui.input(|i| i.smooth_scroll_delta.y);
            self.scroll_y -= dy;
        }
        self.scroll_y = self
            .scroll_y
            .clamp(0.0, (content_height - rows_height + ROW_HEIGHT).max(0.0));

        let palette = Palette::new(ui.visuals().dark_mode);
        ui.painter().rect_filled(full, 0.0, palette.background);
        ui.painter()
            .vline(names.right() + 0.5, full.y_range(), (1.0, palette.grid));

        signal_list::ui(self, ui, names, source, &palette);
        waveform::ui(self, ui, wave, source, &palette);
    }

    /// The screen rectangle of row `i`, inside the column `column`.
    fn row_rect(&self, column: Rect, i: usize) -> Rect {
        let top = column.top() + AXIS_HEIGHT + i as f32 * ROW_HEIGHT - self.scroll_y;
        Rect::from_min_size(
            pos2(column.left(), top),
            Vec2::new(column.width(), ROW_HEIGHT),
        )
    }

    /// Indices of the rows that are (at least partly) visible.
    fn visible_rows(&self, column: Rect) -> std::ops::Range<usize> {
        let first = (self.scroll_y / ROW_HEIGHT).floor() as usize;
        let count = ((column.height() - AXIS_HEIGHT) / ROW_HEIGHT).ceil() as usize + 1;
        first.min(self.traces.len())..(first + count).min(self.traces.len())
    }
}

/// Colors, chosen for the current light/dark theme.
pub struct Palette {
    pub background: Color32,
    pub axis_bg: Color32,
    pub grid: Color32,
    pub text: Color32,
    pub weak_text: Color32,
    pub selected_row: Color32,
    pub trace: Color32,
    pub high_z: Color32,
    pub unknown: Color32,
    pub unknown_fill: Color32,
    pub dense_fill: Color32,
    pub cursor: Color32,
    pub marker: Color32,
}

impl Palette {
    fn new(dark: bool) -> Self {
        if dark {
            Self {
                background: Color32::from_rgb(0x16, 0x18, 0x1c),
                axis_bg: Color32::from_rgb(0x22, 0x25, 0x2b),
                grid: Color32::from_rgb(0x30, 0x34, 0x3b),
                text: Color32::from_rgb(0xe6, 0xe6, 0xe6),
                weak_text: Color32::from_rgb(0x9a, 0xa0, 0xa8),
                selected_row: Color32::from_rgb(0x1f, 0x33, 0x4d),
                trace: Color32::from_rgb(0x4c, 0xd9, 0x7b),
                high_z: Color32::from_rgb(0xf0, 0xc6, 0x4a),
                unknown: Color32::from_rgb(0xff, 0x5c, 0x5c),
                unknown_fill: Color32::from_rgba_unmultiplied(0xff, 0x5c, 0x5c, 0x40),
                dense_fill: Color32::from_rgba_unmultiplied(0x4c, 0xd9, 0x7b, 0x70),
                cursor: Color32::from_rgb(0xff, 0xd8, 0x4d),
                marker: Color32::from_rgb(0x4d, 0xc3, 0xff),
            }
        } else {
            Self {
                background: Color32::from_rgb(0xfa, 0xfa, 0xfb),
                axis_bg: Color32::from_rgb(0xec, 0xee, 0xf1),
                grid: Color32::from_rgb(0xdc, 0xdf, 0xe4),
                text: Color32::from_rgb(0x1d, 0x1f, 0x23),
                weak_text: Color32::from_rgb(0x5f, 0x66, 0x70),
                selected_row: Color32::from_rgb(0xd6, 0xe6, 0xfa),
                trace: Color32::from_rgb(0x12, 0x8a, 0x3c),
                high_z: Color32::from_rgb(0xb5, 0x83, 0x00),
                unknown: Color32::from_rgb(0xd0, 0x21, 0x21),
                unknown_fill: Color32::from_rgba_unmultiplied(0xd0, 0x21, 0x21, 0x30),
                dense_fill: Color32::from_rgba_unmultiplied(0x12, 0x8a, 0x3c, 0x60),
                cursor: Color32::from_rgb(0xc7, 0x7c, 0x00),
                marker: Color32::from_rgb(0x00, 0x6f, 0xb8),
            }
        }
    }
}
