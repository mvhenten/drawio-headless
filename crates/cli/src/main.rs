//! drawio-headless CLI.
//!
//! ```text
//! drawio-headless render <input.drawio> [<output>] [--format svg|png]
//! drawio-headless render --stdin > out.svg
//!
//! drawio-headless author <input.json> [<output.drawio>]
//! drawio-headless author --stdin > out.drawio
//!
//! drawio-headless compose <input.json> [<output>]
//!     [--format svg|png] [--keep-drawio <path>] [--stdin]
//!
//! drawio-headless list-shapes [--library aws|azure|gcp|k8s|all]
//!                             [--format text|json]
//! ```
//!
//! `author` reads a small JSON schema (see `docs/authoring-schema.md`) and
//! emits a `.drawio` XML file using the `drawio-author` library. `compose`
//! is `author` piped into `render` — the common LLM flow. `list-shapes`
//! enumerates the curated factory catalogue so LLMs can discover what's
//! available without scraping headers.

mod author;
mod compose;
mod listing;
#[cfg(feature = "rasterize")]
mod rasterize;

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(name = "drawio-headless", version, about)]
struct Cli {
    /// Print full error chain and stack traces.
    #[arg(long, global = true)]
    verbose: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// Render a .drawio XML file to SVG (or PNG with `--format png`).
    Render {
        /// Input file. Ignored when --stdin is set.
        input: Option<PathBuf>,
        /// Output file. If omitted, write to stdout (svg) or to
        /// `<input-stem>.png` (png).
        output: Option<PathBuf>,
        /// Read the .drawio XML from stdin instead of a file.
        #[arg(long)]
        stdin: bool,
        /// Output format. PNG requires the `rasterize` feature.
        #[arg(long, value_enum, default_value_t = OutFormat::Svg)]
        format: OutFormat,
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
    /// Author + render in one shot: JSON in, SVG (or PNG) out.
    Compose {
        /// Input JSON file. Ignored when --stdin is set.
        input: Option<PathBuf>,
        /// Output file. Defaults to `./<input-stem>.<format>` (or stdout
        /// when reading from stdin and no output is given).
        output: Option<PathBuf>,
        /// Read the JSON from stdin instead of a file.
        #[arg(long)]
        stdin: bool,
        /// Output format. PNG requires the `rasterize` feature.
        #[arg(long, value_enum, default_value_t = OutFormat::Svg)]
        format: OutFormat,
        /// Also write the intermediate `.drawio` XML to this path.
        #[arg(long, value_name = "PATH")]
        keep_drawio: Option<PathBuf>,
    },
    /// Enumerate the curated factory catalogue.
    ListShapes {
        /// Restrict to one library (default: all).
        #[arg(long, value_enum, default_value_t = LibraryFilter::All)]
        library: LibraryFilter,
        /// Output format.
        #[arg(long, value_enum, default_value_t = ListFormat::Text)]
        format: ListFormat,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutFormat {
    Svg,
    Png,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum LibraryFilter {
    Aws,
    Azure,
    Gcp,
    K8s,
    All,
}

impl LibraryFilter {
    fn as_str(self) -> &'static str {
        match self {
            Self::Aws => "aws",
            Self::Azure => "azure",
            Self::Gcp => "gcp",
            Self::K8s => "k8s",
            Self::All => "all",
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ListFormat {
    Text,
    Json,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let verbose = cli.verbose;
    let result = match cli.cmd {
        Cmd::Render {
            input,
            output,
            stdin,
            format,
        } => run_render(input.as_deref(), output.as_deref(), stdin, format),
        Cmd::Author {
            input,
            output,
            stdin,
        } => run_author(input.as_deref(), output.as_deref(), stdin),
        Cmd::Compose {
            input,
            output,
            stdin,
            format,
            keep_drawio,
        } => compose::run(
            input.as_deref(),
            output.as_deref(),
            stdin,
            format,
            keep_drawio.as_deref(),
        ),
        Cmd::ListShapes { library, format } => listing::run(library.as_str(), format),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            // One-line stable prefix in the common path; full chain only
            // under --verbose. Multi-line errors break LLM error scraping.
            if verbose {
                eprintln!("error: {e}");
                let mut src = e.source();
                while let Some(s) = src {
                    eprintln!("  caused by: {s}");
                    src = s.source();
                }
            } else {
                eprintln!("error: {}", first_line(&e.to_string()));
            }
            ExitCode::from(1)
        }
    }
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or(s).to_string()
}

fn run_render(
    input: Option<&Path>,
    output: Option<&Path>,
    use_stdin: bool,
    format: OutFormat,
) -> Result<(), Box<dyn std::error::Error>> {
    let xml = read_input(input, use_stdin)?;
    let svg = drawio_render::render(&xml)?;
    write_rendered(input, output, svg.as_bytes(), format)
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

/// Read either a file or stdin into a string. The `--stdin` flag wins over
/// any positional input — matches the long-standing pattern.
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

/// Write rendered bytes — handling the SVG vs PNG split and the default
/// `<input-stem>.<format>` path. Used by both `render` and `compose`.
pub(crate) fn write_rendered(
    input: Option<&Path>,
    output: Option<&Path>,
    svg: &[u8],
    format: OutFormat,
) -> Result<(), Box<dyn std::error::Error>> {
    match format {
        OutFormat::Svg => write_output(output, svg),
        OutFormat::Png => {
            #[cfg(feature = "rasterize")]
            {
                let png = rasterize::svg_to_png(std::str::from_utf8(svg)?)?;
                let path = output
                    .map(std::path::PathBuf::from)
                    .or_else(|| default_output_path(input, "png"))
                    .ok_or("--format png requires an output path (or a file input)")?;
                std::fs::write(&path, png)
                    .map_err(|e| format!("writing {}: {e}", path.display()))?;
                Ok(())
            }
            #[cfg(not(feature = "rasterize"))]
            {
                let _ = (svg, input, output);
                Err("PNG output requires building with --features rasterize".into())
            }
        }
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

/// Build a `<input-stem>.<ext>` path in the current working directory. Used
/// when the user omits an explicit output path.
pub(crate) fn default_output_path(input: Option<&Path>, ext: &str) -> Option<PathBuf> {
    let stem = input?.file_stem()?.to_owned();
    let mut p = PathBuf::from(stem);
    p.set_extension(ext);
    Some(p)
}
