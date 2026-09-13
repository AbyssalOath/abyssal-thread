// Prevents Windows from allocating a console window for this process at
// all (that's what made the packaged .exe/.msi pop up a blank terminal
// behind the GUI on launch). CLI usage (`abyssal-thread.exe build ...`)
// still works from a terminal - see attach_parent_console() below, which
// reconnects to the caller's console before any output is printed.
#![cfg_attr(windows, windows_subsystem = "windows")]

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::fs;
use std::path::PathBuf;

mod gui;
mod print;
mod print_shaped;

/// Re-attaches this GUI-subsystem process's stdout/stderr to the console
/// of whatever launched it (a terminal), so CLI subcommands still print
/// visibly there. Never called when launched with no args (double-click,
/// desktop shortcut) - those runs stay fully headless.
#[cfg(windows)]
fn attach_parent_console() {
    use std::fs::OpenOptions;
    use std::os::windows::io::IntoRawHandle;
    use windows_sys::Win32::System::Console::{
        AttachConsole, SetStdHandle, ATTACH_PARENT_PROCESS, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE,
    };

    // SAFETY: FFI call with no preconditions; failure (no parent console
    // to attach to) is handled by simply returning.
    let attached = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) != 0 };
    if !attached {
        return;
    }

    // GetStdHandle still returns this process's original (invalid) stdio
    // handles after AttachConsole - std::io::stdout()/stderr() need to be
    // repointed at the console buffer we just attached to.
    for std_handle in [STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
        if let Ok(file) = OpenOptions::new().read(true).write(true).open("CONOUT$") {
            // SAFETY: `file`'s raw handle is a valid, open HANDLE that we
            // hand ownership of to the console via SetStdHandle.
            unsafe {
                SetStdHandle(std_handle, file.into_raw_handle() as _);
            }
        }
    }
}

#[derive(Parser)]
#[command(
    name = "Abyssal Thread",
    about = "A Rust crochet CAD system: DSL -> stitch graph -> 3D model -> chart"
)]
struct Cli {
    // Optional so double-clicking the built binary (no args at all, e.g. on
    // Windows) falls through to the Gui branch below instead of clap
    // printing usage and exiting instantly - see main()'s `unwrap_or`.
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Parse a .cgp pattern file and build/inspect the stitch graph.
    Build {
        /// Path to a .cgp pattern source file.
        input: PathBuf,
        /// Write a schematic SVG chart to this path.
        #[arg(long)]
        svg: Option<PathBuf>,
        /// Write a Wavefront OBJ stitch armature to this path (for Blender).
        #[arg(long)]
        obj: Option<PathBuf>,
        /// Print a stitch-tension summary after layout.
        #[arg(long)]
        tension: bool,
        /// Swatch gauge: stitches per 4 inches. Defaults to the reference
        /// gauge already baked into StitchKind's baseline stitch
        /// dimensions (16), which reproduces the old hardcoded-only
        /// behavior exactly.
        #[arg(long)]
        gauge_sts_per_4in: Option<f32>,
        /// Swatch gauge: rows per 4 inches. Defaults to the reference
        /// gauge (16), same as `gauge_sts_per_4in`.
        #[arg(long)]
        gauge_rows_per_4in: Option<f32>,
        /// Mass-spring relaxation passes to run after the closed-form ring
        /// layout (see `abyssal_thread_layout::relax`). 0 disables it and
        /// uses the closed-form layout as-is; a modest default is applied
        /// otherwise since relaxation is a no-op on already-symmetric
        /// rounds and only does real work on asymmetric ones.
        #[arg(long, default_value_t = 20)]
        relax_iterations: usize,
    },

    /// Launch the interactive grid editor + 3D viewport GUI.
    Gui {
        /// Optional .cgp pattern file to open on startup.
        input: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    // Any argument (a subcommand, `--help`, etc.) means this was launched
    // from a terminal expecting CLI behavior, not double-clicked.
    #[cfg(windows)]
    if std::env::args().len() > 1 {
        attach_parent_console();
    }

    let cli = Cli::parse();
    match cli.command.unwrap_or(Commands::Gui { input: None }) {
        Commands::Build {
            input,
            svg,
            obj,
            tension,
            gauge_sts_per_4in,
            gauge_rows_per_4in,
            relax_iterations,
        } => {
            let gauge = abyssal_thread_layout::Gauge {
                sts_per_4in: gauge_sts_per_4in
                    .unwrap_or(abyssal_thread_layout::REFERENCE_STS_PER_4IN),
                rows_per_4in: gauge_rows_per_4in
                    .unwrap_or(abyssal_thread_layout::REFERENCE_ROWS_PER_4IN),
            };
            build(input, svg, obj, tension, gauge, relax_iterations)
        }
        Commands::Gui { input } => gui::run(input),
    }
}

fn build(
    input: PathBuf,
    svg: Option<PathBuf>,
    obj: Option<PathBuf>,
    tension: bool,
    gauge: abyssal_thread_layout::Gauge,
    relax_iterations: usize,
) -> Result<()> {
    let src = fs::read_to_string(&input).with_context(|| format!("reading {}", input.display()))?;

    let pattern = abyssal_thread_lang::parser::parse(&src)
        .map_err(|e| anyhow::anyhow!("parse error: {e}"))?;
    println!(
        "Parsed pattern '{}' with {} round(s), {} custom stitch definition(s) (unexpanded)",
        pattern.name.as_deref().unwrap_or("<unnamed>"),
        pattern.rounds.len(),
        pattern.definitions.len(),
    );

    let mut graph = abyssal_thread_lang::eval::eval(&pattern)
        .map_err(|e| anyhow::anyhow!("eval error: {e}"))?;
    println!(
        "Built stitch graph: {} stitches across {} round(s)",
        graph.stitch_count(),
        graph.round_count()
    );
    // Currently just raw `DEF` bodies' out-of-range `%-N`/`%+N` refs - see
    // `StitchGraph::warnings`'s doc comment. Non-fatal, but worth a nudge
    // rather than vanishing with no trace.
    for warning in &graph.warnings {
        eprintln!("warning: {warning}");
    }

    abyssal_thread_layout::layout_ring(&mut graph, gauge);
    abyssal_thread_layout::relax(&mut graph, gauge, relax_iterations);

    if tension {
        abyssal_thread_layout::analyze_tension(&mut graph, gauge);
        let report = abyssal_thread_layout::summarize_tension(&graph);
        println!(
            "Tension report: {} normal, {} loose, {} stretched (>{}% deviation)",
            report.normal,
            report.loose,
            report.stretched,
            (abyssal_thread_layout::TENSION_THRESHOLD * 100.0) as i32
        );
    }

    if let Some(path) = svg {
        let chart = abyssal_thread_export::export_svg_chart(&graph);
        fs::write(&path, chart).with_context(|| format!("writing {}", path.display()))?;
        println!("Wrote SVG chart to {}", path.display());
    }

    if let Some(path) = obj {
        let obj_text = abyssal_thread_export::export_obj(&graph);
        fs::write(&path, obj_text).with_context(|| format!("writing {}", path.display()))?;
        println!("Wrote OBJ stitch armature to {}", path.display());
    }

    Ok(())
}
