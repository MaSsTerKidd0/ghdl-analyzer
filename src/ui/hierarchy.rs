//! The design hierarchy browser (left panel).

use egui::{CollapsingHeader, RichText, ScrollArea, TextEdit, Ui};

use crate::model::{ScopeNode, SignalId, SignalInfo, SignalKind, WaveSource};

/// State of the hierarchy panel that survives between frames.
#[derive(Default)]
pub struct HierarchyPanel {
    filter: String,
}

/// The most results the filtered list shows; more would make the UI sluggish.
const MAX_FILTER_RESULTS: usize = 500;

impl HierarchyPanel {
    /// Draw the panel. Returns the signals the user asked to add to the view.
    pub fn ui(&mut self, ui: &mut Ui, source: &dyn WaveSource) -> Vec<SignalId> {
        let mut add = Vec::new();

        ui.horizontal(|ui| {
            ui.add(
                TextEdit::singleline(&mut self.filter)
                    .hint_text("Filter signals…")
                    .desired_width(f32::INFINITY),
            );
        });
        ui.label(
            RichText::new("Double-click a signal to add it")
                .small()
                .weak(),
        );
        ui.separator();

        ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.filter.trim().is_empty() {
                    for scope in source.hierarchy() {
                        scope_ui(ui, scope, source.signals(), &mut add, 0);
                    }
                } else {
                    self.filtered_ui(ui, source.signals(), &mut add);
                }
            });
        add
    }

    /// A flat list of all signals whose path contains the filter text.
    fn filtered_ui(&self, ui: &mut Ui, signals: &[SignalInfo], add: &mut Vec<SignalId>) {
        let needle = self.filter.trim().to_lowercase();
        let matches: Vec<SignalId> = (0..signals.len())
            .filter(|&id| signals[id].path.to_lowercase().contains(&needle))
            .take(MAX_FILTER_RESULTS + 1)
            .collect();

        if matches.is_empty() {
            ui.label(RichText::new("No matching signals").weak());
            return;
        }
        if ui
            .button(format!(
                "Add all {} shown",
                matches.len().min(MAX_FILTER_RESULTS)
            ))
            .clicked()
        {
            add.extend(matches.iter().take(MAX_FILTER_RESULTS));
        }
        for &id in matches.iter().take(MAX_FILTER_RESULTS) {
            signal_row(ui, id, &signals[id], &signals[id].path, add);
        }
        if matches.len() > MAX_FILTER_RESULTS {
            ui.label(RichText::new("…more results, refine the filter").weak());
        }
    }
}

/// One scope and, recursively, everything under it.
fn scope_ui(
    ui: &mut Ui,
    scope: &ScopeNode,
    signals: &[SignalInfo],
    add: &mut Vec<SignalId>,
    depth: usize,
) {
    CollapsingHeader::new(&scope.name)
        // The id must be unique per tree position; the name alone may repeat.
        .id_salt((depth, scope as *const ScopeNode as usize))
        .default_open(depth == 0 && !scope.is_empty())
        .show(ui, |ui| {
            for child in &scope.scopes {
                scope_ui(ui, child, signals, add, depth + 1);
            }
            if !scope.signals.is_empty() {
                let response = ui.small_button("+ add all signals here");
                if response.clicked() {
                    add.extend(&scope.signals);
                }
            }
            for &id in &scope.signals {
                signal_row(ui, id, &signals[id], &signals[id].name, add);
            }
        });
}

fn signal_row(ui: &mut Ui, id: SignalId, info: &SignalInfo, label: &str, add: &mut Vec<SignalId>) {
    let icon = match info.kind {
        SignalKind::Bit => "⎍",
        SignalKind::Vector(_) => "⇶",
        SignalKind::Enum => "≡",
        SignalKind::Real => "∿",
        SignalKind::Text => "T",
    };
    let response = ui
        .selectable_label(false, format!("{icon} {label}"))
        .on_hover_text(&info.path);
    if response.double_clicked() {
        add.push(id);
    }
    response.context_menu(|ui| {
        if ui.button("Add to waveform").clicked() {
            add.push(id);
            ui.close();
        }
    });
}
