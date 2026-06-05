//! SVG -> PNG rasterisation, gated behind the `rasterize` feature flag.
//!
//! Mirrors the recipe in `closed-loop-test`: parse the SVG with `usvg`,
//! allocate a `tiny-skia` pixmap sized to the SVG's natural extents, paint a
//! white background (drawio diagrams are designed against white), render
//! via `resvg`, and encode to PNG. No on-disk intermediate.
//!
//! Fonts: `drawio-render` emits `<text font-family="sans-serif">`. usvg's
//! default `fontdb` is empty, so without loading fonts *and* binding the
//! `sans-serif` generic family to a concrete face, resvg silently drops every
//! label. We load the system fonts, point `sans-serif` at the first sans face
//! we find, and — when the host has no fonts at all (a bare CI/container) —
//! warn on stderr so the missing text is loud, not silent.

use std::io::Write;

use resvg::usvg;
use resvg::usvg::fontdb;

/// Rasterise an SVG string to a PNG byte buffer.
///
/// The pixmap is sized to `ceil(width)` x `ceil(height)` of the SVG's
/// natural extents. Returns an error if the SVG cannot be parsed or if the
/// pixmap allocation fails (very large diagrams).
pub fn svg_to_png(svg: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut opts = usvg::Options::default();
    setup_fonts(opts.fontdb_mut(), &mut std::io::stderr());
    let tree = usvg::Tree::from_str(svg, &opts).map_err(|e| format!("usvg parse: {e}"))?;
    let size = tree.size().to_int_size();
    let mut pixmap = tiny_skia::Pixmap::new(size.width(), size.height())
        .ok_or("tiny-skia pixmap allocation failed (svg too large?)")?;
    pixmap.fill(tiny_skia::Color::WHITE);
    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());
    let png = pixmap
        .encode_png()
        .map_err(|e| format!("png encode: {e}"))?;
    Ok(png)
}

/// Load the host's fonts into `db`, then bind the `sans-serif` family and warn
/// (via [`bind_and_warn`]). Split from [`bind_and_warn`] so the latter — the
/// part with the fontless branch — is testable against an empty `fontdb`
/// without `load_system_fonts` repopulating it on a font-equipped host.
fn setup_fonts(db: &mut fontdb::Database, warn: &mut dyn Write) {
    db.load_system_fonts();
    bind_and_warn(db, warn);
}

/// Bind the `sans-serif` generic family to a concrete face if one exists, and
/// warn through `warn` when `db` is empty (a bare CI/container with no fonts),
/// since resvg will then drop every `<text>` element.
fn bind_and_warn(db: &mut fontdb::Database, warn: &mut dyn Write) {
    if let Some(name) = sans_serif_face(db) {
        db.set_sans_serif_family(name);
    }
    if db.is_empty() {
        // Best-effort: a failed write to stderr is not worth aborting over.
        let _ = writeln!(
            warn,
            "warning: no fonts found on this system; PNG text labels will be \
             missing. Install a font package (e.g. fonts-dejavu-core) or render \
             to SVG instead.",
        );
    }
}

/// Pick the family name of the first sans-serif face in `db`, so the generic
/// `sans-serif` family the renderer emits resolves to something concrete.
/// Returns `None` when the database holds no font advertising a "sans" family.
fn sans_serif_face(db: &fontdb::Database) -> Option<String> {
    db.faces()
        .find(|face| {
            face.families
                .iter()
                .any(|(name, _)| name.to_lowercase().contains("sans"))
        })
        .map(|face| face.families[0].0.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_fontdb_emits_warning() {
        // The fontless-container case from the bug report: an empty database
        // must make `bind_and_warn` write a clear warning rather than letting
        // resvg drop labels silently.
        let mut db = fontdb::Database::new();
        assert!(db.is_empty());
        let mut captured: Vec<u8> = Vec::new();
        bind_and_warn(&mut db, &mut captured);
        let msg = String::from_utf8(captured).unwrap();
        assert!(msg.contains("no fonts found"), "warning text: {msg}");
        assert!(msg.contains("text labels will be"), "warning text: {msg}");
    }

    #[test]
    fn populated_fontdb_does_not_warn() {
        // With at least one font present, no warning should be emitted. Skip
        // the assertion on a genuinely fontless host (the empty-db test above
        // already covers that branch).
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        if db.is_empty() {
            return;
        }
        let mut captured: Vec<u8> = Vec::new();
        bind_and_warn(&mut db, &mut captured);
        assert!(
            captured.is_empty(),
            "no warning expected when fonts are present: {:?}",
            String::from_utf8_lossy(&captured),
        );
    }

    #[test]
    fn sans_serif_face_picks_a_sans_family() {
        // Build a database from whatever the host provides; if it has any
        // sans face, we must select it. On a fontless host this is a no-op
        // assertion (None), which is the case the warning covers.
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        // Some(name): must be a sans face. None: host advertises no sans
        // family — nothing to assert, the warning path covers that case.
        if let Some(name) = sans_serif_face(&db) {
            assert!(
                name.to_lowercase().contains("sans"),
                "selected family should be a sans face, got {name:?}",
            );
        }
    }
}
