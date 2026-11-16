use std::path::PathBuf;

use clap::Parser;
use ghdl_analyzer::app::AnalyzerApp;

/// Waveform viewer for GHDL simulations (GHW, FST and VCD files).
#[derive(Parser)]
#[command(version, about)]
struct Args {
    /// Waveform file to open (.ghw, .fst or .vcd).
    file: Option<PathBuf>,

    /// Add every signal whose path contains TEXT to the view at start-up
    /// (can be repeated, e.g. `--show clk --show counter_tb.count`).
    #[arg(long, value_name = "TEXT")]
    show: Vec<String>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // GPU backend: wgpu (Vulkan/Metal/DX12) by default, OpenGL ("glow") as a fallback
    // for machines and VMs where wgpu cannot create a surface.
    // Force one with GHDL_ANALYZER_RENDERER=glow or =wgpu.
    let forced = std::env::var("GHDL_ANALYZER_RENDERER").ok();
    let renderers = match forced.as_deref() {
        Some("glow") => vec![eframe::Renderer::Glow],
        Some("wgpu") => vec![eframe::Renderer::Wgpu],
        _ => vec![eframe::Renderer::Wgpu, eframe::Renderer::Glow],
    };

    let mut last_error = None;
    for renderer in renderers {
        match run(renderer, args.file.clone(), args.show.clone()) {
            Ok(()) => return Ok(()),
            Err(e) => {
                eprintln!("{renderer:?} renderer failed: {e}");
                last_error = Some(e);
            }
        }
    }
    // eframe's error type is not `Send + Sync`, so convert it to text for anyhow.
    Err(anyhow::anyhow!(
        "could not start the GUI: {}",
        last_error.map(|e| e.to_string()).unwrap_or_default()
    ))
}

fn run(renderer: eframe::Renderer, file: Option<PathBuf>, show: Vec<String>) -> eframe::Result {
    let options = eframe::NativeOptions {
        renderer,
        viewport: egui::ViewportBuilder::default()
            .with_title("GHDL Analyzer")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([640.0, 400.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };

    eframe::run_native(
        "GHDL Analyzer",
        options,
        Box::new(|cc| Ok(Box::new(AnalyzerApp::new(&cc.egui_ctx, file, show)))),
    )
}
