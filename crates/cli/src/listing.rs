//! `list-shapes` subcommand: enumerate the curated factory catalogue.
//!
//! `text` (default) groups entries by library/category for humans. `json`
//! emits a flat array of `{library, key, category}` objects so an LLM can
//! consume the catalogue without a second pass at parsing.

use std::io::Write;

use drawio_author::catalogue;

use crate::ListFormat;

pub fn run(library: &str, format: ListFormat) -> Result<(), Box<dyn std::error::Error>> {
    let entries = catalogue::for_library(library);
    let mut stdout = std::io::stdout().lock();
    match format {
        ListFormat::Json => print_json(&mut stdout, &entries)?,
        ListFormat::Text => print_text(&mut stdout, &entries)?,
    }
    Ok(())
}

fn print_json<W: Write>(out: &mut W, entries: &[catalogue::Entry]) -> std::io::Result<()> {
    // Hand-rolled to avoid pulling serde_json into the print path. The
    // catalogue is small, the shape is fixed, and the output is asserted by
    // the integration tests so any drift is caught.
    writeln!(out, "[")?;
    for (i, e) in entries.iter().enumerate() {
        let comma = if i + 1 == entries.len() { "" } else { "," };
        writeln!(
            out,
            "  {{\"library\":\"{lib}\",\"key\":\"{key}\",\"category\":{cat}}}{comma}",
            lib = json_escape(e.library),
            key = json_escape(e.key),
            cat = json_string(e.category),
        )?;
    }
    writeln!(out, "]")?;
    Ok(())
}

fn print_text<W: Write>(out: &mut W, entries: &[catalogue::Entry]) -> std::io::Result<()> {
    // Group by (library, category) in the input order — the catalogue is
    // already sorted that way.
    let mut current_lib: Option<&str> = None;
    let mut current_cat: Option<&str> = None;
    for e in entries {
        if current_lib != Some(e.library) {
            if current_lib.is_some() {
                writeln!(out)?;
            }
            writeln!(out, "{}:", e.library)?;
            current_lib = Some(e.library);
            current_cat = None;
        }
        if current_cat != Some(e.category) {
            writeln!(out, "  {}:", e.category)?;
            current_cat = Some(e.category);
        }
        writeln!(out, "    {}.{}", e.library, e.key)?;
    }
    Ok(())
}

/// Escape a `&str` for embedding inside a JSON string literal, returning a
/// raw fragment (no surrounding quotes). Only the five-or-so escapes JSON
/// requires; we own the inputs (static catalogue strings) and they don't
/// contain control characters.
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                use std::fmt::Write as _;
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

fn json_string(s: &str) -> String {
    format!("\"{}\"", json_escape(s))
}
