//! `compose` subcommand: author + render in one shot.
//!
//! The common LLM flow is "JSON in, SVG out". `compose` is glue:
//! 1. Read JSON (file or stdin)
//! 2. Author to `.drawio` XML via [`crate::author::build_xml`]
//! 3. Render to SVG via `drawio_render::render`
//! 4. Optionally rasterise to PNG (gated on the `rasterize` feature)
//!
//! Unless `--keep-drawio <path>` is passed, the intermediate XML is held
//! entirely in memory — no stray files on disk.

use std::path::Path;

use crate::{OutFormat, author, default_output_path, read_input, write_output, write_rendered};

pub fn run(
    input: Option<&Path>,
    output: Option<&Path>,
    use_stdin: bool,
    format: OutFormat,
    keep_drawio: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let json = read_input(input, use_stdin)?;
    let xml = author::build_xml(&json)?;

    if let Some(drawio_path) = keep_drawio {
        std::fs::write(drawio_path, xml.as_bytes())
            .map_err(|e| format!("writing {}: {e}", drawio_path.display()))?;
    }

    let svg = drawio_render::render(&xml)?;

    // For SVG output and a positional path, write directly. For SVG with no
    // path on file input, fall back to `<input-stem>.svg` per the spec; with
    // stdin input and no path, write to stdout.
    match (format, output, input, use_stdin) {
        (OutFormat::Svg, Some(p), _, _) => write_output(Some(p), svg.as_bytes()),
        (OutFormat::Svg, None, Some(stem_src), _) => {
            let path = default_output_path(Some(stem_src), "svg")
                .ok_or("could not derive default output path from input")?;
            std::fs::write(&path, svg.as_bytes())
                .map_err(|e| format!("writing {}: {e}", path.display()).into())
        }
        (OutFormat::Svg, None, None, true) => write_output(None, svg.as_bytes()),
        (OutFormat::Svg, None, None, false) => {
            Err("compose: missing input file (or pass --stdin)".into())
        }
        (OutFormat::Png, _, _, _) => write_rendered(input, output, svg.as_bytes(), format),
    }
}
