//! SVG -> PNG rasterisation, gated behind the `rasterize` feature flag.
//!
//! Mirrors the recipe in `closed-loop-test`: parse the SVG with `usvg`,
//! allocate a `tiny-skia` pixmap sized to the SVG's natural extents, paint a
//! white background (drawio diagrams are designed against white), render
//! via `resvg`, and encode to PNG. No on-disk intermediate.

use resvg::usvg;

/// Rasterise an SVG string to a PNG byte buffer.
///
/// The pixmap is sized to `ceil(width)` x `ceil(height)` of the SVG's
/// natural extents. Returns an error if the SVG cannot be parsed or if the
/// pixmap allocation fails (very large diagrams).
pub fn svg_to_png(svg: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let opts = usvg::Options::default();
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
