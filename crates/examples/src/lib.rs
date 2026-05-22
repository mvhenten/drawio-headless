//! Shared helper for the worked AWS architecture examples.
//!
//! The actual scenarios live in `examples/<name>.rs` and run with
//! `cargo run -p drawio-headless-examples --example <name>`.

use std::path::{Path, PathBuf};
use std::{fs, io};

use drawio_author::Diagram;

/// Resolve `docs/examples/` relative to the workspace root.
pub fn output_dir() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest
        .parent()
        .and_then(Path::parent)
        .expect("workspace root above crates/examples");
    workspace.join("docs").join("examples")
}

/// Serialise `diagram` to `.drawio` + `.svg` under `docs/examples/<name>.*`.
pub fn write_artifacts(name: &str, diagram: &Diagram) -> io::Result<()> {
    let dir = output_dir();
    fs::create_dir_all(&dir)?;
    let xml = diagram.to_xml();
    fs::write(dir.join(format!("{name}.drawio")), &xml)?;
    let svg = drawio_render::render(&xml).map_err(io::Error::other)?;
    fs::write(dir.join(format!("{name}.svg")), svg)?;
    Ok(())
}
