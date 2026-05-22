//! Headless renderer for `.drawio` XML.
//!
//! Pipeline (no browser, no DOM):
//!
//! 1. [`model::parse`] — read `mxfile/diagram/mxGraphModel/root/mxCell` into
//!    Rust structs.
//! 2. For each vertex: parse its style with [`style::StyleMap`].
//! 3. For each AWS resource-icon vertex: look up its stencil glyph in the
//!    bundled [`stencil::StencilLibrary`] and emit SVG.
//! 4. For each edge: draw a straight line between source/target midpoints
//!    with a simple arrowhead.
//!
//! Compressed payloads (`compressed="true"`) are rejected with a clear error.

pub mod model;
pub mod stencil;
pub mod style;

use std::fmt::Write as _;
use std::sync::OnceLock;

use crate::model::{Edge, Model, Vertex};
use crate::stencil::{StencilLibrary, render_stencil_to_svg};
use crate::style::StyleMap;

/// Bundled AWS stencil source (about 6 MB). Lives at compile time so the
/// library is self-contained.
const AWS4_STENCIL: &str = include_str!("../../../stencils/aws4.xml");

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("XML parse error: {0}")]
    Xml(String),
    #[error("compressed diagram payloads are not yet supported; re-save with compressed=\"false\"")]
    CompressedUnsupported,
    #[error("unsupported stencil command: {0}")]
    UnsupportedStencilCmd(String),
}

/// One-shot global library so we don't reparse the 6 MB stencil file each
/// call. Internally lazy.
fn aws4() -> &'static StencilLibrary {
    static LIB: OnceLock<StencilLibrary> = OnceLock::new();
    LIB.get_or_init(|| StencilLibrary::from_xml(AWS4_STENCIL).expect("bundled aws4.xml must parse"))
}

/// Render a `.drawio` XML string to SVG.
pub fn render(xml: &str) -> Result<String, RenderError> {
    let model = model::parse(xml)?;
    Ok(render_model(&model))
}

fn render_model(model: &Model) -> String {
    let (vb_x, vb_y, vb_w, vb_h) = compute_viewbox(model);
    let mut svg = String::with_capacity(8 * 1024);

    let _ = write!(
        svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" \
         viewBox=\"{vb_x} {vb_y} {vb_w} {vb_h}\" \
         width=\"{vb_w}\" height=\"{vb_h}\">"
    );
    // Background fill (white) so rasterised output has a defined backdrop.
    let _ = write!(
        svg,
        "<rect x=\"{vb_x}\" y=\"{vb_y}\" width=\"{vb_w}\" height=\"{vb_h}\" fill=\"#ffffff\"/>"
    );
    // Arrowhead marker definition for edges.
    svg.push_str(
        "<defs><marker id=\"arrow\" viewBox=\"0 0 10 10\" refX=\"9\" refY=\"5\" \
         markerWidth=\"6\" markerHeight=\"6\" orient=\"auto-start-reverse\">\
         <path d=\"M 0 0 L 10 5 L 0 10\" fill=\"none\" stroke=\"#232F3E\" stroke-width=\"1.5\"/>\
         </marker></defs>",
    );

    // Edges first (so vertices paint over edge endpoints).
    for e in &model.edges {
        render_edge(&mut svg, model, e);
    }
    for v in &model.vertices {
        render_vertex(&mut svg, v);
    }

    svg.push_str("</svg>");
    svg
}

fn compute_viewbox(model: &Model) -> (f64, f64, f64, f64) {
    if model.vertices.is_empty() {
        return (0.0, 0.0, 800.0, 600.0);
    }
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for v in &model.vertices {
        min_x = min_x.min(v.x);
        min_y = min_y.min(v.y);
        max_x = max_x.max(v.x + v.w);
        max_y = max_y.max(v.y + v.h + 24.0); // leave room for label below
    }
    let margin = 24.0;
    (
        min_x - margin,
        min_y - margin,
        (max_x - min_x) + 2.0 * margin,
        (max_y - min_y) + 2.0 * margin,
    )
}

fn render_vertex(out: &mut String, v: &Vertex) {
    let style = StyleMap::parse(&v.style);
    let shape = style.get("shape").unwrap_or_default();
    let fill = style.get_or("fillColor", "#cccccc");
    let font_color = style.get_or("fontColor", "#232F3E");

    if shape == "mxgraph.aws4.resourceIcon" {
        // Coloured tile.
        let _ = write!(
            out,
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{fill}\" \
             stroke=\"none\" rx=\"3\" ry=\"3\"/>",
            v.x, v.y, v.w, v.h
        );
        // Glyph from stencil.
        if let Some(res_icon) = style.get("resIcon")
            && let Some(stencil) = aws4().lookup(res_icon)
        {
            out.push_str(&render_stencil_to_svg(
                stencil, v.x, v.y, v.w, v.h, 0.18, "#ffffff",
            ));
        }
    } else {
        // Plain rect fallback.
        let _ = write!(
            out,
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{fill}\" \
             stroke=\"#999\" stroke-width=\"1\"/>",
            v.x, v.y, v.w, v.h
        );
    }

    // Label: plain text below the cell, horizontally centred.
    if !v.label.is_empty() {
        let cx = v.x + v.w / 2.0;
        let ly = v.y + v.h + 14.0;
        let _ = write!(
            out,
            "<text x=\"{cx}\" y=\"{ly}\" font-family=\"sans-serif\" font-size=\"12\" \
             fill=\"{font_color}\" text-anchor=\"middle\">{}</text>",
            escape_text(&v.label)
        );
    }
}

fn render_edge(out: &mut String, model: &Model, e: &Edge) {
    let Some(src) = model.vertices.iter().find(|v| v.id == e.source) else {
        return;
    };
    let Some(tgt) = model.vertices.iter().find(|v| v.id == e.target) else {
        return;
    };
    let sx = src.x + src.w / 2.0;
    let sy = src.y + src.h / 2.0;
    let tx = tgt.x + tgt.w / 2.0;
    let ty = tgt.y + tgt.h / 2.0;
    let style = StyleMap::parse(&e.style);
    let end_arrow = style.get_or("endArrow", "open");
    let marker_end = if end_arrow == "none" {
        ""
    } else {
        " marker-end=\"url(#arrow)\""
    };
    let _ = write!(
        out,
        "<line x1=\"{sx}\" y1=\"{sy}\" x2=\"{tx}\" y2=\"{ty}\" \
         stroke=\"#232F3E\" stroke-width=\"1.5\"{marker_end}/>"
    );
}

fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_minimal_diagram() {
        let xml = r#"
<mxfile compressed="false"><diagram><mxGraphModel><root>
<mxCell id="0"/><mxCell id="1" parent="0"/>
<mxCell id="a" value="A" vertex="1" parent="1" style="shape=mxgraph.aws4.resourceIcon;fillColor=#ED7100;resIcon=mxgraph.aws4.lambda;">
  <mxGeometry x="10" y="20" width="78" height="78" as="geometry"/>
</mxCell>
</root></mxGraphModel></diagram></mxfile>"#;
        let svg = render(xml).unwrap();
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains("<rect"));
        assert!(svg.contains("<path")); // stencil glyph rendered
    }

    #[test]
    fn rejects_compressed_payload() {
        let xml = r#"<mxfile compressed="true"><diagram>x</diagram></mxfile>"#;
        let err = render(xml).unwrap_err();
        assert!(matches!(err, RenderError::CompressedUnsupported));
    }
}
