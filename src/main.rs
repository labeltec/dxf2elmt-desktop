#![warn(
    clippy::all,
    clippy::pedantic,
    //clippy::cargo,
    //rust_2024_compatibility,
)]
//#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use anyhow::Result;
use clap::Parser;
use dxf2elmt::{convert_dxf_file, ConversionOptions};
use std::{io, path::PathBuf};
use tracing::{span, trace, Level};
use tracing_subscriber::prelude::*;

#[cfg(feature = "venator")]
use venator::Venator;

#[derive(Parser, Debug)]
#[command(name = "dxf2elmt")]
#[command(author, version, about = "A CLI program to convert .dxf files into .elmt files", long_about = None)]
struct Args {
    /// The .dxf file to convert
    //#[clap(short, long, value_parser)]
    file_names: Vec<PathBuf>,

    /// Activates verbose output, eliminates .elmt file writing
    #[clap(short, long, value_parser, default_value_t = false)]
    verbose: bool,

    /// Converts text entities into dynamic text instead of the default text box
    #[clap(short, long, value_parser, default_value_t = false)]
    dtext: bool,

    /// Determine the number of lines you want each spline to have (more lines = greater resolution)
    #[clap(short, long, value_parser, default_value_t = 20)]
    spline_step: u32,

    /// Toggles information output... defaults to off
    #[clap(short, long, value_parser, default_value_t = false)]
    info: bool,
}


#[allow(clippy::too_many_lines)]
fn main() -> Result<()> {
    #[cfg(feature = "venator")]
    let tr_reg = tracing_subscriber::registry()
        .with(Venator::default())
        .with(tracing_subscriber::fmt::Layer::default());

    #[cfg(not(feature = "venator"))]
    let tr_reg = {
        let file_layer =
            if let std::result::Result::Ok(file) = std::fs::File::create("dxf2elmt.log") {
                //if we can create a log file use it
                Some(tracing_subscriber::fmt::layer().with_writer(std::sync::Arc::new(file)))
            } else {
                None
            };

        let stde_layer = tracing_subscriber::fmt::layer()
            .pretty()
            .with_writer(io::stderr);
        tracing_subscriber::registry()
            .with(file_layer.map(|fl| {
                fl.with_file(true)
                    .with_line_number(true)
                    .with_thread_ids(true)
                    .with_ansi(false)
                    .with_filter(tracing_subscriber::EnvFilter::from_env("DXF2E_LOG"))
            }))
            .with(
                stde_layer
                    .with_file(true)
                    .with_line_number(true)
                    .with_thread_ids(true)
                    .with_filter(tracing_subscriber::EnvFilter::from_env("DXF2E_LOG")),
            )
    };
    tr_reg.init();

    trace!("Starting dxf2elmt");

    // Collect arguments
    let args: Args = Args::parse_from(wild::args());

    // Check if any files were provided
    if args.file_names.is_empty() {
        eprintln!("Error: No input files specified.");
        eprintln!("\nUsage: dxf2elmt <file.dxf> [options]");
        eprintln!("\nFor more information, use: dxf2elmt --help");
        std::process::exit(1);
    }

    // Convert files
    let dxf_loop_span = span!(Level::TRACE, "Looping over dxf files");
    let dxf_loop_guard = dxf_loop_span.enter();
    
    let options = ConversionOptions {
        spline_step: args.spline_step,
        verbose: args.verbose,
        info: args.info,
        px_per_mm: 2.0, // Default: 2px / 1mm
    };

    for file_name in args.file_names {
        let result = convert_dxf_file(&file_name, &options)?;

        if options.info {
            if let Some(stats) = result.stats {
                println!("Conversion complete!\n");
                println!("STATS");
                println!("~~~~~~~~~~~~~~~");
                println!("Circles: {}", stats.circles);
                println!("Lines: {}", stats.lines);
                println!("Arcs: {}", stats.arcs);
                println!("Splines: {}", stats.splines);
                println!("Texts: {}", stats.texts);
                println!("Ellipses: {}", stats.ellipses);
                println!("Polylines: {}", stats.polylines);
                println!("LwPolylines: {}", stats.lwpolylines);
                println!("Solids: {}", stats.solids);
                println!("Blocks: {}", stats.blocks);
                println!("Currently Unsupported: {}", stats.unsupported);
                println!("\nTime Elapsed: {} ms", stats.elapsed_ms);
            }
        }

        if options.verbose {
            if let Some(xml) = result.xml_content {
                print!("{xml}");
            }
        }
    }
    drop(dxf_loop_guard);

    Ok(())
}
