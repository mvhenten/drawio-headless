//! Headless renderer for `.drawio` XML.
//!
//! Pipeline (no browser, no DOM):
//!
//! 1. [`model::parse`] — read `mxfile/diagram/mxGraphModel/root/mxCell` into
//!    Rust structs.
//! 2. For each vertex: parse its style with [`style::StyleMap`].
//! 3. For each AWS resource-icon vertex: look up its stencil glyph in the
//!    bundled [`stencil::StencilLibrary`] and emit SVG.
//! 4. For each edge: route between the picked connection points. Edges
//!    declaring `edgeStyle=orthogonalEdgeStyle` render as a two-segment
//!    right-angle polyline (one corner); other edges fall back to a
//!    straight line. Endpoints carry a simple arrowhead.
//!
//! Compressed payloads (`compressed="true"`) are rejected with a clear error.

pub mod model;
pub mod stencil;
pub mod style;

use std::fmt::Write as _;
use std::sync::OnceLock;

use crate::model::{Edge, Model, Vertex};
use crate::stencil::{StencilLibrary, render_stencil_to_svg};
use crate::style::{StyleMap, parse_points};

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

    // Z-order: groups first (boundary containers paint behind everything),
    // then edges, then non-group vertices on top so icon tiles visually
    // cover any connector passing under them — arrowheads land at the
    // cell's connection point and get tucked a hair under the tile edge,
    // matching the upstream drawio look.
    for v in &model.vertices {
        if is_group(&v.style) {
            render_group(&mut svg, v);
        }
    }
    for e in &model.edges {
        render_edge(&mut svg, model, e);
    }
    for v in &model.vertices {
        if !is_group(&v.style) {
            render_vertex(&mut svg, v);
        }
    }

    svg.push_str("</svg>");
    svg
}

fn is_group(style: &str) -> bool {
    StyleMap::parse(style).get("shape") == Some("mxgraph.aws4.group")
}

fn render_group(out: &mut String, v: &Vertex) {
    let style = StyleMap::parse(&v.style);
    let stroke = style.get_or("strokeColor", "#7D8998");
    let font_color = style.get_or("fontColor", stroke);
    let dashed = style.get("dashed").unwrap_or("1") != "0";
    let dash_attr = if dashed {
        " stroke-dasharray=\"6,4\""
    } else {
        ""
    };
    let _ = write!(
        out,
        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"none\" \
         stroke=\"{stroke}\" stroke-width=\"1.5\"{dash_attr} rx=\"6\" ry=\"6\"/>",
        v.x, v.y, v.w, v.h
    );
    if !v.label.is_empty() {
        let lx = v.x + 14.0;
        let ly = v.y + 20.0;
        let _ = write!(
            out,
            "<text x=\"{lx}\" y=\"{ly}\" font-family=\"sans-serif\" font-size=\"13\" \
             font-weight=\"600\" fill=\"{font_color}\" text-anchor=\"start\">{}</text>",
            escape_text(&v.label)
        );
    }
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
    let src_centre = (src.x + src.w / 2.0, src.y + src.h / 2.0);
    let tgt_centre = (tgt.x + tgt.w / 2.0, tgt.y + tgt.h / 2.0);
    let (sx, sy) = pick_endpoint(src, tgt_centre).unwrap_or(src_centre);
    let (tx, ty) = pick_endpoint(tgt, src_centre).unwrap_or(tgt_centre);
    let style = StyleMap::parse(&e.style);
    let end_arrow = style.get_or("endArrow", "open");
    let marker_end = if end_arrow == "none" {
        ""
    } else {
        " marker-end=\"url(#arrow)\""
    };

    let orthogonal = style.get("edgeStyle") == Some("orthogonalEdgeStyle");
    if orthogonal && let Some((mx, my)) = orthogonal_corner(src, (sx, sy), (tx, ty)) {
        let _ = write!(
            out,
            "<path d=\"M {sx} {sy} L {mx} {my} L {tx} {ty}\" fill=\"none\" \
             stroke=\"#232F3E\" stroke-width=\"1.5\"{marker_end}/>"
        );
        return;
    }

    let _ = write!(
        out,
        "<line x1=\"{sx}\" y1=\"{sy}\" x2=\"{tx}\" y2=\"{ty}\" \
         stroke=\"#232F3E\" stroke-width=\"1.5\"{marker_end}/>"
    );
}

/// Compute the single corner of a two-segment right-angle route from
/// `start` to `end`, given the cell `start` sits on.
///
/// The orientation of the leading segment is chosen by which side of the
/// source cell the start endpoint lies on:
/// - start on a vertical side (left/right): leave horizontally — corner is
///   `(end.x, start.y)`.
/// - start on a horizontal side (top/bottom): leave vertically — corner is
///   `(start.x, end.y)`.
///
/// Returns `None` when the endpoints are colinear (same x or y) so the
/// caller can degrade to a single straight segment. Also returns `None` if
/// the start point is not on any edge of the source cell (e.g. it landed
/// at the cell centre because no connection points were declared) — there
/// is no sensible orientation to choose in that case.
fn orthogonal_corner(src: &Vertex, start: (f64, f64), end: (f64, f64)) -> Option<(f64, f64)> {
    // Colinear: a single segment is already the right-angle route.
    if (start.0 - end.0).abs() < 1e-9 || (start.1 - end.1).abs() < 1e-9 {
        return None;
    }
    let on_vertical_side =
        (start.0 - src.x).abs() < 1e-6 || (start.0 - (src.x + src.w)).abs() < 1e-6;
    let on_horizontal_side =
        (start.1 - src.y).abs() < 1e-6 || (start.1 - (src.y + src.h)).abs() < 1e-6;
    if on_vertical_side {
        // Leave horizontally first.
        Some((end.0, start.1))
    } else if on_horizontal_side {
        // Leave vertically first.
        Some((start.0, end.1))
    } else {
        None
    }
}

/// Choose the absolute coordinate of `cell`'s declared connection point
/// nearest to `toward`. Returns `None` when the cell has no `points=` in
/// its style, leaving the caller to fall back to the midpoint.
///
/// Tie-break: if two constraints are equidistant from `toward`, the one
/// declared first in the style string wins (stable iteration order from
/// [`style::parse_points`]).
fn pick_endpoint(cell: &Vertex, toward: (f64, f64)) -> Option<(f64, f64)> {
    let style = StyleMap::parse(&cell.style);
    let points = parse_points(style.get("points")?);
    if points.is_empty() {
        return None;
    }
    let mut best: Option<((f64, f64), f64)> = None;
    for (nx, ny) in points {
        let ax = cell.x + f64::from(nx) * cell.w;
        let ay = cell.y + f64::from(ny) * cell.h;
        let dx = ax - toward.0;
        let dy = ay - toward.1;
        let d2 = dx * dx + dy * dy;
        match best {
            None => best = Some(((ax, ay), d2)),
            Some((_, bd)) if d2 < bd => best = Some(((ax, ay), d2)),
            _ => {}
        }
    }
    best.map(|(pt, _)| pt)
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

    #[test]
    fn edge_endpoints_snap_to_perimeter_constraints() {
        // Canonical 16-point AWS resource-icon constraint set: corners +
        // 0.25/0.5/0.75 along every edge.
        let aws_style = "shape=mxgraph.aws4.resourceIcon;\
             points=[[0,0,0],[0.25,0,0],[0.5,0,0],[0.75,0,0],[1,0,0],\
             [0,1,0],[0.25,1,0],[0.5,1,0],[0.75,1,0],[1,1,0],\
             [0,0.25,0],[0,0.5,0],[0,0.75,0],[1,0.25,0],[1,0.5,0],[1,0.75,0]];\
             resIcon=mxgraph.aws4.lambda;fillColor=#ED7100;";
        let a = Vertex {
            id: "a".into(),
            label: String::new(),
            style: aws_style.into(),
            x: 0.0,
            y: 0.0,
            w: 78.0,
            h: 78.0,
        };
        let b = Vertex {
            id: "b".into(),
            label: String::new(),
            style: aws_style.into(),
            x: 300.0,
            y: 0.0,
            w: 78.0,
            h: 78.0,
        };
        let a_end = pick_endpoint(&a, (b.x + b.w / 2.0, b.y + b.h / 2.0)).unwrap();
        let b_end = pick_endpoint(&b, (a.x + a.w / 2.0, a.y + a.h / 2.0)).unwrap();
        // Source endpoint should be on A's right edge (x = 78).
        assert!(
            (a_end.0 - 78.0).abs() < 1e-9,
            "source endpoint x should be on A's right edge (78), got {a_end:?}",
        );
        // Target endpoint should be on B's left edge (x = 300).
        assert!(
            (b_end.0 - 300.0).abs() < 1e-9,
            "target endpoint x should be on B's left edge (300), got {b_end:?}",
        );
        // With the 16-point set, the right-mid (1, 0.5) is closest to B's
        // centre — so the source endpoint lands at A's vertical midline
        // (y = 39), not a corner.
        assert!(
            (a_end.1 - 39.0).abs() < 1e-9,
            "source endpoint y should be A's right-mid (39), got {a_end:?}",
        );
        assert!(
            (b_end.1 - 39.0).abs() < 1e-9,
            "target endpoint y should be B's left-mid (39), got {b_end:?}",
        );
        // And both should NOT be at the cell midpoints (x = 39 / x = 339).
        assert!(
            (a_end.0 - 39.0).abs() > 1e-6,
            "should not pick A's midpoint"
        );
        assert!(
            (b_end.0 - 339.0).abs() > 1e-6,
            "should not pick B's midpoint"
        );
    }

    #[test]
    fn pick_endpoint_falls_back_when_no_points_declared() {
        let plain = Vertex {
            id: "p".into(),
            label: String::new(),
            style: "shape=rectangle;fillColor=#cccccc;".into(),
            x: 10.0,
            y: 10.0,
            w: 60.0,
            h: 40.0,
        };
        assert!(pick_endpoint(&plain, (100.0, 100.0)).is_none());
    }

    #[test]
    fn orthogonal_corner_horizontal_first_from_right_edge() {
        // Source endpoint sits on the right edge of source (x = 78). The
        // route must leave horizontally, so the corner is at (end.x, start.y).
        let src = Vertex {
            id: "a".into(),
            label: String::new(),
            style: String::new(),
            x: 0.0,
            y: 0.0,
            w: 78.0,
            h: 78.0,
        };
        let corner = orthogonal_corner(&src, (78.0, 39.0), (300.0, 100.0)).unwrap();
        assert!(
            (corner.0 - 300.0).abs() < 1e-9,
            "corner.x = end.x: {corner:?}"
        );
        assert!(
            (corner.1 - 39.0).abs() < 1e-9,
            "corner.y = start.y: {corner:?}"
        );
    }

    #[test]
    fn orthogonal_corner_vertical_first_from_bottom_edge() {
        // Source endpoint sits on the bottom edge of source (y = 78). The
        // route must leave vertically, so the corner is at (start.x, end.y).
        let src = Vertex {
            id: "a".into(),
            label: String::new(),
            style: String::new(),
            x: 0.0,
            y: 0.0,
            w: 78.0,
            h: 78.0,
        };
        let corner = orthogonal_corner(&src, (39.0, 78.0), (200.0, 300.0)).unwrap();
        assert!(
            (corner.0 - 39.0).abs() < 1e-9,
            "corner.x = start.x: {corner:?}"
        );
        assert!(
            (corner.1 - 300.0).abs() < 1e-9,
            "corner.y = end.y: {corner:?}"
        );
    }

    #[test]
    fn orthogonal_corner_colinear_endpoints_yield_none() {
        let src = Vertex {
            id: "a".into(),
            label: String::new(),
            style: String::new(),
            x: 0.0,
            y: 0.0,
            w: 78.0,
            h: 78.0,
        };
        // Same y: a single horizontal segment is already the route.
        assert!(orthogonal_corner(&src, (78.0, 39.0), (300.0, 39.0)).is_none());
        // Same x: a single vertical segment is already the route.
        assert!(orthogonal_corner(&src, (39.0, 78.0), (39.0, 300.0)).is_none());
    }

    #[test]
    fn orthogonal_corner_endpoint_not_on_edge_yields_none() {
        // No declared connection points -> endpoint defaults to the cell
        // centre, which sits on no edge. We have no orientation to pick.
        let src = Vertex {
            id: "a".into(),
            label: String::new(),
            style: String::new(),
            x: 0.0,
            y: 0.0,
            w: 78.0,
            h: 78.0,
        };
        assert!(orthogonal_corner(&src, (39.0, 39.0), (300.0, 100.0)).is_none());
    }

    #[test]
    fn renders_orthogonal_edge_as_path_with_corner() {
        // Two AWS resource icons offset both horizontally and vertically —
        // the picker lands on A's right-mid and B's left-mid, so the
        // route must bend.
        let xml = r#"
<mxfile compressed="false"><diagram><mxGraphModel><root>
<mxCell id="0"/><mxCell id="1" parent="0"/>
<mxCell id="a" value="A" vertex="1" parent="1" style="shape=mxgraph.aws4.resourceIcon;points=[[0,0,0],[0.25,0,0],[0.5,0,0],[0.75,0,0],[1,0,0],[0,1,0],[0.25,1,0],[0.5,1,0],[0.75,1,0],[1,1,0],[0,0.25,0],[0,0.5,0],[0,0.75,0],[1,0.25,0],[1,0.5,0],[1,0.75,0]];resIcon=mxgraph.aws4.lambda;fillColor=#ED7100;">
  <mxGeometry x="0" y="0" width="78" height="78" as="geometry"/>
</mxCell>
<mxCell id="b" value="B" vertex="1" parent="1" style="shape=mxgraph.aws4.resourceIcon;points=[[0,0,0],[0.25,0,0],[0.5,0,0],[0.75,0,0],[1,0,0],[0,1,0],[0.25,1,0],[0.5,1,0],[0.75,1,0],[1,1,0],[0,0.25,0],[0,0.5,0],[0,0.75,0],[1,0.25,0],[1,0.5,0],[1,0.75,0]];resIcon=mxgraph.aws4.lambda;fillColor=#ED7100;">
  <mxGeometry x="300" y="200" width="78" height="78" as="geometry"/>
</mxCell>
<mxCell id="e1" edge="1" parent="1" source="a" target="b" style="edgeStyle=orthogonalEdgeStyle;html=0;endArrow=open;rounded=0;">
  <mxGeometry relative="1" as="geometry"/>
</mxCell>
</root></mxGraphModel></diagram></mxfile>"#;
        let svg = render(xml).unwrap();
        // The picker snaps A's endpoint to its bottom-right corner (78, 78)
        // — the constraint nearest to B's centre (339, 239) — and B's
        // endpoint to its top-left corner (300, 200). Source endpoint sits
        // on the right edge of A (x = 78), so routing leaves horizontally:
        // corner at (end.x, start.y) = (300, 78).
        assert!(
            svg.contains("<path d=\"M 78 78 L 300 78 L 300 200\""),
            "expected orthogonal path; got: {svg}",
        );
    }

    #[test]
    fn straight_line_edge_when_edgestyle_missing() {
        // No `edgeStyle=orthogonalEdgeStyle` in the edge style — keep
        // the legacy straight-line behaviour.
        let xml = r#"
<mxfile compressed="false"><diagram><mxGraphModel><root>
<mxCell id="0"/><mxCell id="1" parent="0"/>
<mxCell id="a" value="A" vertex="1" parent="1" style="shape=mxgraph.aws4.resourceIcon;points=[[1,0.5,0]];resIcon=mxgraph.aws4.lambda;fillColor=#ED7100;">
  <mxGeometry x="0" y="0" width="78" height="78" as="geometry"/>
</mxCell>
<mxCell id="b" value="B" vertex="1" parent="1" style="shape=mxgraph.aws4.resourceIcon;points=[[0,0.5,0]];resIcon=mxgraph.aws4.lambda;fillColor=#ED7100;">
  <mxGeometry x="300" y="200" width="78" height="78" as="geometry"/>
</mxCell>
<mxCell id="e1" edge="1" parent="1" source="a" target="b" style="endArrow=open;rounded=0;">
  <mxGeometry relative="1" as="geometry"/>
</mxCell>
</root></mxGraphModel></diagram></mxfile>"#;
        let svg = render(xml).unwrap();
        // No path-with-corner; the legacy <line> is emitted instead.
        assert!(
            svg.contains("<line x1=\"78\" y1=\"39\" x2=\"300\" y2=\"239\""),
            "expected straight <line>; got: {svg}",
        );
    }

    #[test]
    fn renders_group_as_dashed_rect() {
        let xml = r#"
<mxfile compressed="false"><diagram><mxGraphModel><root>
<mxCell id="0"/><mxCell id="1" parent="0"/>
<mxCell id="g" value="Account A" vertex="1" parent="1" style="shape=mxgraph.aws4.group;grIcon=mxgraph.aws4.group_account;strokeColor=#CD2264;fontColor=#CD2264;dashed=1;fillColor=none;">
  <mxGeometry x="40" y="40" width="320" height="200" as="geometry"/>
</mxCell>
<mxCell id="a" value="Lambda" vertex="1" parent="1" style="shape=mxgraph.aws4.resourceIcon;resIcon=mxgraph.aws4.lambda;fillColor=#ED7100;">
  <mxGeometry x="120" y="120" width="78" height="78" as="geometry"/>
</mxCell>
</root></mxGraphModel></diagram></mxfile>"#;
        let svg = render(xml).unwrap();
        assert!(
            svg.contains("stroke-dasharray"),
            "group should be dashed: {svg}"
        );
        assert!(
            svg.contains("Account A"),
            "group label should appear: {svg}"
        );
        assert!(
            svg.contains("stroke=\"#CD2264\""),
            "group stroke colour: {svg}"
        );
    }
}
