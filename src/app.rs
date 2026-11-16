//! The application: window layout, menus, file loading.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use egui::{Panel, RichText, Ui};

use crate::model::WaveSource;
use crate::model::file_source::{FileSource, LoadError};
use crate::ui::WaveView;
use crate::ui::hierarchy::HierarchyPanel;

/// A file being loaded on a background thread.
struct Loading {
    path: PathBuf,
    /// The background thread sends exactly one message here when it is done.
    receiver: Receiver<Result<FileSource, LoadError>>,
}

/// Everything that belongs to one opened waveform.
struct Session {
    /// `Box<dyn WaveSource>`: the app does not care which kind of source this is.
    source: Box<dyn WaveSource>,
    hierarchy: HierarchyPanel,
    view: WaveView,
}

#[derive(Default)]
pub struct AnalyzerApp {
    session: Option<Session>,
    loading: Option<Loading>,
    error: Option<String>,
    show_shortcuts: bool,
    /// Signals to add as soon as the first file has loaded (from `--show`).
    pending_signals: Vec<String>,
}

impl AnalyzerApp {
    /// `show` lists path fragments: every signal whose path contains one of them is
    /// added to the view once `initial_file` has loaded.
    pub fn new(ctx: &egui::Context, initial_file: Option<PathBuf>, show: Vec<String>) -> Self {
        let mut app = Self {
            pending_signals: show,
            ..Self::default()
        };
        if let Some(path) = initial_file {
            app.open(ctx, path);
        }
        app
    }

    /// Start loading a file without blocking the UI.
    pub fn open(&mut self, ctx: &egui::Context, path: PathBuf) {
        let (sender, receiver) = channel();
        let ctx = ctx.clone();
        let thread_path = path.clone();
        // `move` gives the closure ownership of `sender`, `ctx` and `thread_path`,
        // which is required because the thread may outlive this function.
        std::thread::spawn(move || {
            let result = FileSource::open(&thread_path);
            // If the app was closed meanwhile the receiver is gone; that is fine.
            let _ = sender.send(result);
            // Wake the UI up so it notices the result right away.
            ctx.request_repaint();
        });
        self.loading = Some(Loading { path, receiver });
        self.error = None;
    }

    /// Check whether the background load has finished.
    fn poll_loading(&mut self) {
        let Some(loading) = &self.loading else { return };
        match loading.receiver.try_recv() {
            Ok(Ok(source)) => {
                let mut source: Box<dyn WaveSource> = Box::new(source);
                let mut view = WaveView::new(source.time_range());
                // `std::mem::take` moves the list out and leaves an empty Vec behind,
                // so the patterns are only applied to the first file.
                let patterns = std::mem::take(&mut self.pending_signals);
                if !patterns.is_empty() {
                    let ids: Vec<_> = (0..source.signals().len())
                        .filter(|&id| {
                            let path = &source.signals()[id].path;
                            patterns.iter().any(|p| path.contains(p.as_str()))
                        })
                        .collect();
                    view.add_signals(source.as_mut(), &ids);
                }
                self.session = Some(Session {
                    source,
                    hierarchy: HierarchyPanel::default(),
                    view,
                });
                self.loading = None;
            }
            Ok(Err(e)) => {
                self.error = Some(e.to_string());
                self.loading = None;
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.error = Some(format!(
                    "loading {} failed unexpectedly",
                    loading.path.display()
                ));
                self.loading = None;
            }
        }
    }

    fn open_dialog(&mut self, ctx: &egui::Context) {
        let file = rfd::FileDialog::new()
            .add_filter("Waveforms", &["ghw", "fst", "vcd"])
            .add_filter("All files", &["*"])
            .pick_file();
        if let Some(path) = file {
            self.open(ctx, path);
        }
    }

    fn menu_bar(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Open…").clicked() {
                    ui.close();
                    self.open_dialog(&ctx);
                }
                if ui.button("Quit").clicked() {
                    ui.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("View", |ui| {
                let enabled = self.session.is_some();
                if ui
                    .add_enabled(enabled, egui::Button::new("Zoom to fit  (F)"))
                    .clicked()
                    && let Some(s) = &mut self.session
                {
                    s.view.zoom_to_fit(s.source.time_range());
                    ui.close();
                }
                if ui
                    .add_enabled(enabled, egui::Button::new("Zoom in  (+)"))
                    .clicked()
                    && let Some(s) = &mut self.session
                {
                    s.view.zoom_in();
                    ui.close();
                }
                if ui
                    .add_enabled(enabled, egui::Button::new("Zoom out  (-)"))
                    .clicked()
                    && let Some(s) = &mut self.session
                {
                    s.view.zoom_out();
                    ui.close();
                }
                ui.separator();
                egui::widgets::global_theme_preference_buttons(ui);
            });
            ui.menu_button("Help", |ui| {
                if ui.button("Keyboard & mouse").clicked() {
                    self.show_shortcuts = true;
                    ui.close();
                }
            });
        });
    }

    fn status_bar(&self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if let Some(loading) = &self.loading {
                ui.spinner();
                ui.label(format!("Loading {}…", loading.path.display()));
            } else if let Some(err) = &self.error {
                ui.colored_label(ui.visuals().error_fg_color, err);
            } else if let Some(s) = &self.session {
                let ts = s.source.timescale();
                ui.label(s.source.description());
                ui.separator();
                let (_, end) = s.source.time_range();
                ui.label(format!("End: {}", ts.format(end)));
                if let Some(c) = s.view.cursor {
                    ui.separator();
                    ui.label(format!("Cursor: {}", ts.format(c)));
                }
                if let Some(m) = s.view.marker {
                    ui.separator();
                    ui.label(format!("Marker: {}", ts.format(m)));
                }
                if let (Some(c), Some(m)) = (s.view.cursor, s.view.marker) {
                    ui.separator();
                    let delta = c.abs_diff(m);
                    ui.label(RichText::new(format!("Δ {}", ts.format(delta))).strong());
                }
            } else {
                ui.label("No file open");
            }
        });
    }

    fn welcome(ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.3);
            ui.heading("GHDL Analyzer");
            ui.add_space(8.0);
            ui.label("Open a waveform file (.ghw, .fst, .vcd) with File → Open, or drop it here.");
            ui.add_space(8.0);
            ui.label(
                RichText::new("Create one with:  ghdl -r my_tb --wave=my_tb.ghw")
                    .monospace()
                    .weak(),
            );
        });
    }

    fn shortcuts_window(&mut self, ctx: &egui::Context) {
        egui::Window::new("Keyboard & mouse")
            .open(&mut self.show_shortcuts)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                egui::Grid::new("shortcuts").striped(true).show(ui, |ui| {
                    for (key, what) in SHORTCUTS {
                        ui.label(RichText::new(*key).monospace());
                        ui.label(*what);
                        ui.end_row();
                    }
                });
            });
    }
}

const SHORTCUTS: &[(&str, &str)] = &[
    ("Mouse wheel", "Zoom around the pointer"),
    ("Shift+wheel / drag", "Pan"),
    ("Click", "Place cursor"),
    ("Shift+click / middle click", "Place marker"),
    ("F", "Zoom to fit"),
    ("+ / -", "Zoom in / out"),
    ("← / →", "Pan"),
    ("N / Shift+→", "Next edge of selected signal"),
    ("P / Shift+←", "Previous edge of selected signal"),
    ("↑ / ↓", "Select signal"),
    ("Delete", "Remove selected signal"),
    ("M", "Put marker at cursor"),
    ("Home / End", "Go to start / end"),
    ("Right-click a signal", "Radix, move, remove"),
    ("Drag a signal name", "Reorder"),
];

impl eframe::App for AnalyzerApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_loading();

        // Drag-and-drop a file onto the window.
        let dropped: Option<PathBuf> =
            ctx.input(|i| i.raw.dropped_files.first().map(|f| f.path().to_path_buf()));
        if let Some(path) = dropped {
            self.open(&ctx, path);
        }

        Panel::top("menu").show(ui, |ui| self.menu_bar(ui));
        Panel::bottom("status").show(ui, |ui| self.status_bar(ui));

        if let Some(session) = &mut self.session {
            let added = Panel::left("hierarchy")
                .resizable(true)
                .default_size(280.0)
                .show(ui, |ui| session.hierarchy.ui(ui, session.source.as_ref()))
                .inner;
            if !added.is_empty() {
                session.view.add_signals(session.source.as_mut(), &added);
            }
            egui::CentralPanel::no_frame().show(ui, |ui| {
                session.view.handle_keys(ui, session.source.as_ref());
                session.view.ui(ui, session.source.as_ref());
            });
        } else {
            egui::CentralPanel::default_margins().show(ui, Self::welcome);
        }

        self.shortcuts_window(&ctx);
        if self.loading.is_some() {
            // Keep the spinner animating.
            ctx.request_repaint();
        }
    }
}
