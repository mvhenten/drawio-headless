//! drawio-headless CLI.
//!
//! ```text
//! drawio-headless render <input.drawio> [<output.svg>]
//! drawio-headless render --stdin > out.svg
//!
//! drawio-headless author <input.json> [<output.drawio>]
//! drawio-headless author --stdin > out.drawio
//! ```
//!
//! `author` reads a small JSON schema (see `docs/authoring-schema.md`) and
//! emits a `.drawio` XML file using the `drawio-author` library.

mod author;

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
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
    /// Author a .drawio XML file from a small JSON schema.
    Author {
        /// Input JSON file. Ignored when --stdin is set.
        input: Option<PathBuf>,
        /// Output file. If omitted (or `-`), write to stdout.
        output: Option<PathBuf>,
        /// Read the JSON from stdin instead of a file.
        #[arg(long)]
        stdin: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.cmd {
        Cmd::Render {
            input,
            output,
            stdin,
        } => run_render(input.as_deref(), output.as_deref(), stdin),
        Cmd::Author {
            input,
            output,
            stdin,
        } => run_author(input.as_deref(), output.as_deref(), stdin),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("drawio-headless: {e}");
            ExitCode::from(1)
        }
    }
}

fn run_render(
    input: Option<&Path>,
    output: Option<&Path>,
    use_stdin: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let xml = read_input(input, use_stdin)?;
    let svg = drawio_render::render(&xml)?;
    write_output(output, svg.as_bytes())
}

fn run_author(
    input: Option<&Path>,
    output: Option<&Path>,
    use_stdin: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let json = read_input(input, use_stdin)?;
    let xml = author::build_xml(&json)?;
    write_output(output, xml.as_bytes())
}

fn read_input(input: Option<&Path>, use_stdin: bool) -> Result<String, Box<dyn std::error::Error>> {
    if use_stdin {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        Ok(s)
    } else {
        let path = input.ok_or("missing input file (or pass --stdin)")?;
        std::fs::read_to_string(path).map_err(|e| format!("reading {}: {e}", path.display()).into())
    }
}

fn write_output(output: Option<&Path>, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    match output {
        Some(p) if p.as_os_str() != "-" => {
            std::fs::write(p, bytes).map_err(|e| format!("writing {}: {e}", p.display()))?;
        }
        _ => {
            std::io::stdout().write_all(bytes)?;
        }
    }
    Ok(())
}
