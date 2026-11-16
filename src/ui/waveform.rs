//! The right column of the waveform area: time axis, traces and cursors.

use egui::{Align2, FontId, Painter, Pos2, Rect, Sense, Shape, Stroke, Ui, pos2};

use super::{AXIS_HEIGHT, Palette, Trace, WaveView};
use crate::model::time::ticks;
use crate::model::{SegmentValue, SignalKind, Time, Value, WaveSource};

/// Minimum room between two time labels on the axis, in pixels.
const LABEL_SPACING: f32 = 110.0;

pub(super) fn ui(
    view: &mut WaveView,
    ui: &mut Ui,
    column: Rect,
    source: &dyn WaveSource,
    palette: &Palette,
) {
    handle_mouse(view, ui, column, source);

    let painter = ui.painter_at(column);
    let width = column.width();
    let vp = view.viewport;
    let timescale = source.timescale();

    // ---- Time axis and grid ----
    let axis = Rect::from_min_size(column.min, egui::vec2(width, AXIS_HEIGHT));
    painter.rect_filled(axis, 0.0, palette.axis_bg);
    let min_step = f64::from(LABEL_SPACING) * vp.span() / f64::from(width);
    for t in ticks(vp.start, vp.end, min_step) {
        let x = column.left() + vp.to_x(t as f64, width);
        painter.vline(
            x,
            axis.bottom() - 6.0..=column.bottom(),
            (1.0, palette.grid),
        );
        painter.text(
            pos2(x + 3.0, axis.center().y - 2.0),
            Align2::LEFT_CENTER,
            timescale.format(t),
            FontId::proportional(12.0),
            palette.weak_text,
        );
    }

    // ---- Traces ----
    let rows_area = Rect::from_min_max(pos2(column.left(), axis.bottom()), column.max);
    let row_painter = painter.with_clip_rect(rows_area);
    let (_, sim_end) = source.time_range();
    // One pixel worth of time: changes closer than this are drawn as a dense block.
    let resolution = (vp.span() / f64::from(width)).floor().max(1.0) as Time;
    let t0 = vp.start.floor().max(0.0) as Time;
    let t1 = vp.end.ceil() as Time;

    for i in view.visible_rows(column) {
        let row = view.row_rect(column, i);
        if view.selected == Some(i) {
            row_painter.rect_filled(row, 0.0, palette.selected_row.gamma_multiply(0.4));
        }
        row_painter.hline(row.x_range(), row.bottom() - 0.5, (1.0, palette.grid));
        draw_trace(
            &row_painter,
            row,
            &view.traces[i],
            source,
            t0,
            t1,
            resolution,
            sim_end,
            view,
            palette,
        );
    }

    // ---- Cursor and marker ----
    for (time, color, name) in [
        (view.marker, palette.marker, "M"),
        (view.cursor, palette.cursor, ""),
    ] {
        let Some(t) = time else { continue };
        let x = column.left() + vp.to_x(t as f64, width);
        if x < column.left() || x > column.right() {
            continue;
        }
        painter.vline(x, column.y_range(), (1.5, color));
        let label = if name.is_empty() {
            timescale.format(t)
        } else {
            format!("{name} {}", timescale.format(t))
        };
        let galley = painter.layout_no_wrap(label, FontId::proportional(12.0), palette.background);
        let size = galley.size() + egui::vec2(8.0, 2.0);
        // Keep the label on screen, flipping it to the left of the line near the right edge.
        let left = if x + size.x > column.right() {
            x - size.x
        } else {
            x
        };
        let tag = Rect::from_min_size(pos2(left, axis.bottom() - size.y), size);
        painter.rect_filled(tag, 2.0, color);
        painter.galley(tag.min + egui::vec2(4.0, 1.0), galley, palette.background);
    }
}

/// How close (in pixels) a click must be to a signal change to snap onto it.
const SNAP_PIXELS: f32 = 6.0;

/// The time under pixel `x`, moved onto the nearest change of any displayed signal
/// if one is within [`SNAP_PIXELS`]. Measuring "from this edge to that edge" is the
/// most common thing to do with cursors, and hitting an edge exactly by hand is hard.
fn snap_to_edge(view: &WaveView, source: &dyn WaveSource, x: f32, width: f32) -> Time {
    let vp = view.viewport;
    let raw = vp.to_time(x, width).max(0.0);
    let tolerance = f64::from(SNAP_PIXELS) * vp.span() / f64::from(width);
    let t = raw.round() as Time;

    let mut best: Option<(f64, Time)> = None;
    for trace in &view.traces {
        // The candidates are the closest change on each side of the click.
        let at_or_after = source.next_change(trace.id, t.saturating_sub(1));
        let before = source.prev_change(trace.id, t);
        for edge in [at_or_after, before].into_iter().flatten() {
            let distance = (edge as f64 - raw).abs();
            if distance <= tolerance && best.is_none_or(|(d, _)| distance < d) {
                best = Some((distance, edge));
            }
        }
    }
    best.map_or(t, |(_, edge)| edge)
}

/// Clicks, drags and the mouse wheel over the waveform.
fn handle_mouse(view: &mut WaveView, ui: &mut Ui, column: Rect, source: &dyn WaveSource) {
    let response = ui.interact(
        column,
        ui.scope_id().with("waveform"),
        Sense::click_and_drag(),
    );
    let width = column.width();

    // Click: place the cursor. Shift+click or middle-click: place the marker.
    if let Some(pos) = response.interact_pointer_pos() {
        let t = snap_to_edge(view, source, pos.x - column.left(), width);
        let shift = ui.input(|i| i.modifiers.shift);
        if response.clicked() && !shift {
            view.cursor = Some(t);
        } else if (response.clicked() && shift) || response.middle_clicked() {
            view.marker = Some(t);
        }
    }

    // Drag: pan. The drag delta is in pixels; convert to time.
    if response.dragged() {
        let dx = response.drag_delta().x;
        let dt = -f64::from(dx) / f64::from(width) * view.viewport.span();
        view.viewport.pan(dt);
    }

    // Wheel: zoom around the mouse pointer. Horizontal wheel / shift+wheel: pan.
    if let Some(pos) = response.hover_pos() {
        let (scroll, zoom, shift) =
            ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.modifiers.shift));
        let anchor = view.viewport.to_time(pos.x - column.left(), width);
        let pan_px = if shift { scroll.x + scroll.y } else { scroll.x };
        if pan_px != 0.0 {
            view.viewport
                .pan(-f64::from(pan_px) / f64::from(width) * view.viewport.span());
        }
        if !shift && scroll.y != 0.0 {
            // 1.0015^-pixels: smooth, and zooming in then out returns to the same place.
            view.viewport
                .zoom(1.0015f64.powf(-f64::from(scroll.y)), anchor);
        }
        if zoom != 1.0 {
            // Ctrl+wheel or pinch gesture.
            view.viewport.zoom(1.0 / f64::from(zoom), anchor);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_trace(
    painter: &Painter,
    row: Rect,
    trace: &Trace,
    source: &dyn WaveSource,
    t0: Time,
    t1: Time,
    resolution: Time,
    sim_end: Time,
    view: &WaveView,
    palette: &Palette,
) {
    let segments = source.segments(trace.id, t0, t1, resolution);
    let kind = source.signals()[trace.id].kind;
    let width = row.width();
    let to_x = |t: Time| row.left() + view.viewport.to_x(t as f64, width);
    let end_x = to_x(sim_end).min(row.right() + 2.0);

    let top = row.top() + 5.0;
    let bottom = row.bottom() - 6.0;
    let mid = (top + bottom) / 2.0;

    for (k, seg) in segments.iter().enumerate() {
        // Clamp to just outside the visible area: painting far off-screen coordinates
        // wastes time and can lose precision.
        let x0 = to_x(seg.start).max(row.left() - 2.0);
        let x1 = segments
            .get(k + 1)
            .map_or(end_x, |next| to_x(next.start))
            .min(row.right() + 2.0);
        if x1 < x0 {
            continue;
        }
        let starts_on_screen = seg.start >= t0 && k > 0;

        match (&seg.value, kind) {
            (SegmentValue::Dense, _) => {
                let rect = Rect::from_x_y_ranges(x0..=x1.max(x0 + 1.0), top..=bottom);
                painter.rect_filled(rect, 0.0, palette.dense_fill);
            }
            (SegmentValue::Stable(value), SignalKind::Bit) => {
                draw_bit(
                    painter,
                    value,
                    x0,
                    x1,
                    top,
                    mid,
                    bottom,
                    starts_on_screen,
                    palette,
                );
            }
            (SegmentValue::Stable(value), _) => {
                draw_bus(painter, value, trace, x0, x1, top, mid, bottom, palette);
            }
        }
    }
}

/// A 1-bit signal: high line, low line, middle line for 'z', red block for unknown.
#[allow(clippy::too_many_arguments)]
fn draw_bit(
    painter: &Painter,
    value: &Value,
    x0: f32,
    x1: f32,
    top: f32,
    mid: f32,
    bottom: f32,
    edge: bool,
    palette: &Palette,
) {
    let stroke = Stroke::new(1.5, palette.trace);
    match value.bit().unwrap_or('x') {
        '1' | 'h' => {
            painter.hline(x0..=x1, top, stroke);
        }
        '0' | 'l' => {
            painter.hline(x0..=x1, bottom, stroke);
        }
        'z' => {
            painter.hline(x0..=x1, mid, (1.5, palette.high_z));
        }
        _ => {
            // x, u, w, '-': unknown / uninitialized.
            let rect = Rect::from_x_y_ranges(x0..=x1, top..=bottom);
            painter.rect_filled(rect, 0.0, palette.unknown_fill);
            painter.hline(x0..=x1, top, (1.0, palette.unknown));
            painter.hline(x0..=x1, bottom, (1.0, palette.unknown));
        }
    }
    if edge {
        painter.vline(x0, top..=bottom, stroke);
    }
}

/// A multi-bit signal: a "hexagon" per value with the value written inside.
#[allow(clippy::too_many_arguments)]
fn draw_bus(
    painter: &Painter,
    value: &Value,
    trace: &Trace,
    x0: f32,
    x1: f32,
    top: f32,
    mid: f32,
    bottom: f32,
    palette: &Palette,
) {
    let (color, fill) = match value {
        Value::Bits(b) if b.chars().all(|c| c == 'z') => (palette.high_z, None),
        Value::Bits(b) if b.chars().any(|c| !matches!(c, '0' | '1' | 'h' | 'l' | 'z')) => {
            (palette.unknown, Some(palette.unknown_fill))
        }
        _ => (palette.trace, None),
    };

    // The slanted corners, at most 4 px, and never wider than half the segment.
    let slant = ((x1 - x0) / 2.0).min(4.0);
    let points: Vec<Pos2> = vec![
        pos2(x0, mid),
        pos2(x0 + slant, top),
        pos2(x1 - slant, top),
        pos2(x1, mid),
        pos2(x1 - slant, bottom),
        pos2(x0 + slant, bottom),
    ];
    let stroke = Stroke::new(1.2, color);
    if let Some(fill) = fill {
        painter.add(Shape::convex_polygon(points, fill, stroke));
    } else {
        painter.add(Shape::closed_line(points, stroke));
    }

    // The value text, centered in the segment, if there is room for it.
    let room = x1 - x0 - 2.0 * slant - 4.0;
    if room < 8.0 {
        return;
    }
    let text = value.format(trace.radix);
    let font = FontId::monospace(12.0);
    let mut galley = painter.layout_no_wrap(text.clone(), font.clone(), palette.text);
    if galley.size().x > room {
        // Try a shortened version ("1A2B…") before giving up.
        let fits = ((room / galley.size().x) * text.chars().count() as f32) as usize;
        if fits < 2 {
            return;
        }
        let short: String = text.chars().take(fits - 1).chain(['…']).collect();
        galley = painter.layout_no_wrap(short, font, palette.text);
    }
    let center = pos2((x0 + x1) / 2.0, mid);
    let pos = center - galley.size() / 2.0;
    painter.galley(pos, galley, palette.text);
}
