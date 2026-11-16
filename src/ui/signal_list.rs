//! The left column of the waveform area: signal names and their value at the cursor.

use egui::{Align2, FontId, Rect, Sense, Ui, pos2};

use super::{AXIS_HEIGHT, Palette, WaveView};
use crate::model::{Radix, WaveSource};

pub(super) fn ui(
    view: &mut WaveView,
    ui: &mut Ui,
    column: Rect,
    source: &dyn WaveSource,
    palette: &Palette,
) {
    let painter = ui.painter_at(column);
    let font = FontId::monospace(13.0);

    // Header, aligned with the time axis on the right.
    let header = Rect::from_min_size(column.min, egui::vec2(column.width(), AXIS_HEIGHT));
    painter.rect_filled(header, 0.0, palette.axis_bg);
    painter.text(
        pos2(header.left() + 8.0, header.center().y),
        Align2::LEFT_CENTER,
        "Signal",
        FontId::proportional(13.0),
        palette.weak_text,
    );
    painter.text(
        pos2(header.right() - 8.0, header.center().y),
        Align2::RIGHT_CENTER,
        "Value @ cursor",
        FontId::proportional(13.0),
        palette.weak_text,
    );

    // Rows are clipped below the header so they never draw over it while scrolling.
    let rows_area = Rect::from_min_max(pos2(column.left(), column.top() + AXIS_HEIGHT), column.max);
    let painter = painter.with_clip_rect(rows_area);

    let mut action: Option<RowAction> = None;

    for i in view.visible_rows(column) {
        let row = view.row_rect(column, i);
        let trace = &view.traces[i];
        let info = &source.signals()[trace.id];

        // `ui.interact` turns a plain rectangle into something that can be clicked
        // or dragged. The id must be stable across frames, so we derive it from the row.
        let response = ui.interact(
            row.intersect(rows_area),
            ui.scope_id().with(("trace-row", i)),
            Sense::click_and_drag(),
        );

        if view.selected == Some(i) {
            painter.rect_filled(row, 0.0, palette.selected_row);
        } else if response.hovered() {
            painter.rect_filled(row, 0.0, palette.grid.gamma_multiply(0.5));
        }
        painter.hline(row.x_range(), row.bottom() - 0.5, (1.0, palette.grid));

        // The value at the cursor, right-aligned; the name gets the remaining room.
        let value_text = view
            .cursor
            .and_then(|t| source.value_at(trace.id, t))
            .map(|v| v.format(trace.radix))
            .unwrap_or_default();
        let value_rect = painter.text(
            pos2(row.right() - 8.0, row.center().y),
            Align2::RIGHT_CENTER,
            truncate(&value_text, 18),
            font.clone(),
            palette.trace,
        );
        let name_room = (value_rect.left() - row.left() - 16.0).max(20.0);
        let max_chars = (name_room / 7.8) as usize;
        painter.text(
            pos2(row.left() + 8.0, row.center().y),
            Align2::LEFT_CENTER,
            truncate_left(&info.name, max_chars),
            font.clone(),
            palette.text,
        );

        if response.clicked() || response.drag_started() {
            view.selected = Some(i);
        }
        if response.drag_started() {
            view.dragging = Some(i);
        }
        let hover = response.on_hover_text(&info.path);
        hover.context_menu(|ui| {
            view.selected = Some(i);
            if info.kind.supports_radix() {
                ui.label("Radix");
                for radix in Radix::ALL {
                    if ui.radio(trace.radix == radix, radix.label()).clicked() {
                        action = Some(RowAction::SetRadix(i, radix));
                        ui.close();
                    }
                }
                ui.separator();
            }
            if ui.button("Move up").clicked() {
                action = Some(RowAction::Move(i, i.saturating_sub(1)));
                ui.close();
            }
            if ui.button("Move down").clicked() {
                // Insertion index: "after the next row".
                action = Some(RowAction::Move(i, i + 2));
                ui.close();
            }
            if ui.button("Remove").clicked() {
                action = Some(RowAction::Remove(i));
                ui.close();
            }
        });
    }

    // Drag and drop to reorder: while dragging, show where the row will land;
    // on release, move it there.
    if let Some(from) = view.dragging {
        if let Some(pos) = ui.ctx().pointer_interact_pos() {
            let target = target_row(view, column, pos.y);
            let y = view.row_rect(column, target).top();
            painter.hline(column.x_range(), y, (2.0, palette.cursor));
            if ui.input(|i| i.pointer.any_released()) {
                action = Some(RowAction::Move(from, target.min(view.traces.len())));
                view.dragging = None;
            }
        } else {
            view.dragging = None;
        }
    }

    match action {
        Some(RowAction::SetRadix(i, radix)) => view.traces[i].radix = radix,
        Some(RowAction::Remove(i)) => {
            view.selected = Some(i);
            view.remove_selected();
        }
        Some(RowAction::Move(from, to)) => move_trace(view, from, to),
        None => {}
    }
}

/// Changes requested while drawing. We collect them and apply them after the loop,
/// because the loop borrows `view.traces` and Rust will not let us modify the vector
/// while we are still reading from it.
enum RowAction {
    SetRadix(usize, Radix),
    Move(usize, usize),
    Remove(usize),
}

/// The insertion position (0..=len) for a drop at height `y`.
fn target_row(view: &WaveView, column: Rect, y: f32) -> usize {
    let rel = y - column.top() - AXIS_HEIGHT + view.scroll_y;
    ((rel / super::ROW_HEIGHT).round().max(0.0) as usize).min(view.traces.len())
}

/// Move trace `from` so it ends up at position `to` (an insertion index).
fn move_trace(view: &mut WaveView, from: usize, to: usize) {
    if from >= view.traces.len() {
        return;
    }
    let trace = view.traces.remove(from);
    // Removing an earlier element shifts the later positions one to the left.
    let to = if to > from { to - 1 } else { to }.min(view.traces.len());
    view.traces.insert(to, trace);
    view.selected = Some(to);
}

/// Shorten text to `max` characters, ending with an ellipsis.
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let kept: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{kept}…")
    }
}

/// Shorten from the left: for names the end (`...count[7:0]`) is the useful part.
fn truncate_left(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max {
        s.to_string()
    } else {
        let kept: String = s.chars().skip(n - max.saturating_sub(1)).collect();
        format!("…{kept}")
    }
}
