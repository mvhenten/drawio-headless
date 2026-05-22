//! drawio-headless CLI.
//!
//! ```text
//! drawio-headless render <input.drawio> [<output.svg>]
//! drawio-headless render --stdin > out.svg
//! ```
//!
//! Authoring is library-only for v0 (use the `drawio-author` crate).

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "drawio-headless", version, about)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// Render a .drawio XML file to SVG.
    Render {
        /// Input file. Ignored when --stdin is set.
        input: Option<PathBuf>,
        /// Output file. If omitted, write to stdout.
        output: Option<PathBuf>,
        /// Read the .drawio XML from stdin instead of a file.
        #[arg(long)]
        stdin: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Render {
            input,
            output,
            stdin,
        } => match run_render(input, output, stdin) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("drawio-headless: {e}");
                ExitCode::from(1)
            }
        },
    }
}

fn run_render(
    input: Option<PathBuf>,
    output: Option<PathBuf>,
    use_stdin: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let xml = if use_stdin {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        s
    } else {
        let path = input.ok_or("missing input file (or pass --stdin)")?;
        std::fs::read_to_string(&path).map_err(|e| format!("reading {}: {e}", path.display()))?
    };
    let svg = drawio_render::render(&xml)?;
    match output {
        Some(p) => {
            std::fs::write(&p, svg).map_err(|e| format!("writing {}: {e}", p.display()))?;
        }
        None => {
            std::io::stdout().write_all(svg.as_bytes())?;
        }
    }
    Ok(())
}
