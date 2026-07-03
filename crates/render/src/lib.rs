//! Headless renderer for `.drawio` XML.
//!
//! Pipeline (no browser, no DOM):
//!
//! 1. [`model::parse`] — read `mxfile/diagram/mxGraphModel/root/mxCell` into
//!    Rust structs. Transparently inflates compressed `<diagram>` payloads
//!    (the default in interactively-saved drawio files) via [`inflate`].
//! 2. For each vertex: parse its style with [`style::StyleMap`].
//! 3. For each vertex bound to a known stencil library (AWS / Azure / GCP /
//!    Kubernetes): resolve the glyph against the matching bundled
//!    [`stencil::StencilLibrary`] and emit SVG.
//! 4. For each edge: route between the picked connection points, following
//!    the rules in `docs/edge-routing.md` ([`routing`]). An explicit
//!    `exitX/exitY`/`entryX/entryY` override wins over the default pick —
//!    unless it names an exact corner (both members `0` or `1`), which is
//!    nudged to a quarter-point on the same side instead (issue #49): no
//!    departure or arrival point, pinned or defaulted, ever lands on a
//!    corner. Absent an override, the endpoint is distributed across a
//!    shared side alongside any other edges touching it, with a minimum
//!    jetty stub and lane separation from other routes. Edges declaring
//!    `edgeStyle=orthogonalEdgeStyle` render as a right-angle polyline;
//!    other edges fall back to a straight line between the same endpoints.
//!    Endpoints carry a simple arrowhead.
//!
//! A vertex's `rotation` style key rotates the rendered shape (tile/glyph,
//! not the label) around the cell's own centre via an SVG `rotate(deg cx
//! cy)` transform — a rotation relative to the cell, not the page.
//!
//! Stencil library coverage
//! ------------------------
//! Four libraries are bundled at compile time as static strings:
//!
//! - `mxgraph.aws4` from `stencils/aws4.xml` (AWS).
//! - `mxgraph.azure` from `stencils/azure.xml`.
//! - `mxgraph.gcp` from `stencils/gcp.xml` (concatenated category files,
//!   wrapped in a synthetic root).
//! - `mxgraph.kubernetes` from `stencils/kubernetes.xml`.
//!
//! Each library is parsed lazily via its own [`OnceLock`].
//!
//! See [`stencil`] for the mxStencil DSL command coverage (issue #7):
//! `<save>`/`<restore>`, `<strokecolor>`/`<fillcolor>`/`<fontcolor>`,
//! `<strokewidth>`, `<dashed>`, and `<text>` are all implemented; `<image>`
//! is rejected with [`RenderError::UnsupportedStencilCmd`] rather than
//! silently dropped, since this crate has no raster-embedding support.
//! Commands outside that candidate list (`<alpha>`, `<dashpattern>`,
//! `<linecap>`, `<linejoin>`, `<miterlimit>`, `<fontstyle>`, `<fontfamily>`,
//! `<fontsize>`, `<include-shape>`) remain silently skipped, so a shape's
//! outer silhouette may still render with reduced detail.

pub mod inflate;
pub mod model;
mod routing;
pub mod stencil;
pub mod style;

use std::fmt::Write as _;
use std::sync::OnceLock;

use crate::model::{Edge, Model, Vertex};
use crate::routing::Route;
use crate::stencil::{CellBounds, Stencil, StencilLibrary, render_stencil_to_svg};
use crate::style::StyleMap;

/// Bundled AWS stencil source (about 6 MB). Lives at compile time so the
/// library is self-contained.
const AWS4_STENCIL: &str = include_str!("../../../stencils/aws4.xml");
/// Bundled Azure stencil source.
const AZURE_STENCIL: &str = include_str!("../../../stencils/azure.xml");
/// Bundled GCP stencil source. Concatenation of upstream category files
/// under a synthetic `<gcp-libraries>` root — see `stencils/SOURCE-gcp`.
const GCP_STENCIL: &str = include_str!("../../../stencils/gcp.xml");
/// Bundled Kubernetes stencil source.
const KUBERNETES_STENCIL: &str = include_str!("../../../stencils/kubernetes.xml");

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("XML parse error: {0}")]
    Xml(String),
    #[error("base64 decode error: {0}")]
    Base64Decode(#[from] base64::DecodeError),
    #[error("DEFLATE inflate error: {0}")]
    Inflate(#[from] std::io::Error),
    #[error("URL decode error: {0}")]
    UrlDecode(String),
    #[error("unsupported stencil command: {0}")]
    UnsupportedStencilCmd(String),
}

/// One-shot global libraries so we don't reparse the multi-MB stencil
/// files on every call. Internally lazy.
fn aws4() -> &'static StencilLibrary {
    static LIB: OnceLock<StencilLibrary> = OnceLock::new();
    LIB.get_or_init(|| {
        StencilLibrary::from_xml(AWS4_STENCIL, "mxgraph.aws4").expect("bundled aws4.xml must parse")
    })
}

fn azure() -> &'static StencilLibrary {
    static LIB: OnceLock<StencilLibrary> = OnceLock::new();
    LIB.get_or_init(|| {
        StencilLibrary::from_xml(AZURE_STENCIL, "mxgraph.azure")
            .expect("bundled azure.xml must parse")
    })
}

fn gcp() -> &'static StencilLibrary {
    static LIB: OnceLock<StencilLibrary> = OnceLock::new();
    LIB.get_or_init(|| {
        StencilLibrary::from_xml(GCP_STENCIL, "mxgraph.gcp").expect("bundled gcp.xml must parse")
    })
}

fn kubernetes() -> &'static StencilLibrary {
    static LIB: OnceLock<StencilLibrary> = OnceLock::new();
    LIB.get_or_init(|| {
        StencilLibrary::from_xml(KUBERNETES_STENCIL, "mxgraph.kubernetes")
            .expect("bundled kubernetes.xml must parse")
    })
}

/// Identifier for one of the bundled stencil libraries; returned by
/// [`resolve_stencil`] so the caller can pick a glyph colour or label
/// styling appropriate to that library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LibraryKind {
    Aws4,
    Azure,
    Gcp,
    Kubernetes,
}

/// Inspect a vertex's parsed style and, if it refers to a known stencil
/// library, return the matching glyph plus an identifier for the library.
///
/// Dispatches across the four bundled libraries:
/// - `shape=mxgraph.aws4.resourceIcon;resIcon=mxgraph.aws4.<key>` → AWS.
/// - `shape=mxgraph.kubernetes.icon2;prIcon=<key>` → Kubernetes.
/// - `shape=mxgraph.azure.<key>` → Azure (direct stencil reference).
/// - `shape=mxgraph.gcp.<category>.<key>` → GCP.
fn resolve_stencil(style: &StyleMap) -> Option<(&'static Stencil, LibraryKind)> {
    let shape = style.get("shape")?;
    if shape == "mxgraph.aws4.resourceIcon" {
        let res_icon = style.get("resIcon")?;
        return aws4()
            .lookup(res_icon)
            .or_else(|| aws4_res_icon_alias(res_icon).and_then(|a| aws4().lookup(a)))
            .map(|s| (s, LibraryKind::Aws4));
    }
    if shape == "mxgraph.kubernetes.icon2" {
        let pr_icon = style.get("prIcon")?;
        return kubernetes()
            .lookup(pr_icon)
            .map(|s| (s, LibraryKind::Kubernetes));
    }
    if shape.starts_with("mxgraph.azure.") {
        return azure().lookup(shape).map(|s| (s, LibraryKind::Azure));
    }
    if shape.starts_with("mxgraph.gcp.") {
        return gcp().lookup(shape).map(|s| (s, LibraryKind::Gcp));
    }
    None
}

/// Map an AWS `resIcon` value that has no matching stencil onto an equivalent
/// glyph that does. drawio's own catalogue labels the Analytics service group
/// "`OpenSearch` Service" but still keys its tile on the legacy
/// `elasticsearch_service` stencil — so the natural-looking
/// `mxgraph.aws4.opensearch_service` resolves nowhere. Treat it as the
/// elasticsearch-service glyph (same icon, pre-rename name).
///
/// Returns `None` when the icon needs no aliasing, so the caller only pays the
/// second lookup on a miss.
fn aws4_res_icon_alias(res_icon: &str) -> Option<&'static str> {
    let normalised = res_icon.rsplit('.').next().unwrap_or(res_icon);
    match normalised {
        "opensearch_service" => Some("elasticsearch_service"),
        _ => None,
    }
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
    let routes = routing::route_edges(model);
    for (e, route) in model.edges.iter().zip(&routes) {
        let Some(route) = route else { continue };
        render_edge(&mut svg, e, route);
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
    let aspect_fixed = style.get("aspect") == Some("fixed");
    let cell = CellBounds {
        x: v.x,
        y: v.y,
        w: v.w,
        h: v.h,
    };

    // Cell-relative rotation (issue #7): the `rotation` style key rotates
    // the shape around its own centre, not the page. Build the shape into
    // its own buffer so it can be wrapped in a single SVG `rotate(deg cx
    // cy)` transform; the label (rendered separately, below the cell) is
    // intentionally left unrotated — it already has no positional relation
    // to the shape's own coordinate space in this renderer.
    let mut shape_svg = String::new();

    if shape == "mxgraph.aws4.resourceIcon" {
        // AWS resource-icon: coloured tile + white glyph from stencil.
        let _ = write!(
            shape_svg,
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{fill}\" \
             stroke=\"none\" rx=\"3\" ry=\"3\"/>",
            v.x, v.y, v.w, v.h
        );
        if let Some((stencil, _)) = resolve_stencil(&style) {
            shape_svg.push_str(&render_stencil_to_svg(
                stencil,
                cell,
                0.18,
                "#ffffff",
                aspect_fixed,
            ));
        }
    } else if let Some((stencil, kind)) = resolve_stencil(&style) {
        match kind {
            LibraryKind::Kubernetes => {
                // K8s shapes are drawn inside a filled hexagon-ish tile via
                // `mxgraph.kubernetes.icon2`. Approximate as a filled rect
                // (using the declared fillColor) with a white glyph on top.
                let tile_fill = style.get_or("fillColor", "#2875E2");
                let _ = write!(
                    shape_svg,
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" \
                     fill=\"{tile_fill}\" stroke=\"none\" rx=\"4\" ry=\"4\"/>",
                    v.x, v.y, v.w, v.h
                );
                shape_svg.push_str(&render_stencil_to_svg(
                    stencil,
                    cell,
                    0.18,
                    "#ffffff",
                    aspect_fixed,
                ));
            }
            LibraryKind::Azure | LibraryKind::Gcp => {
                // Azure and GCP stencils declare their geometry directly
                // (no surrounding tile). The fillColor in the style is the
                // glyph colour; default to the canonical Azure cyan / GCP
                // blue if the diagram omits it.
                let default = if kind == LibraryKind::Azure {
                    "#00BEF2"
                } else {
                    "#4285F4"
                };
                let glyph = style.get_or("fillColor", default);
                shape_svg.push_str(&render_stencil_to_svg(
                    stencil,
                    cell,
                    0.04,
                    glyph,
                    aspect_fixed,
                ));
            }
            LibraryKind::Aws4 => unreachable!("aws4 handled above"),
        }
    } else {
        // Plain rect fallback for unknown shapes.
        let _ = write!(
            shape_svg,
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{fill}\" \
             stroke=\"#999\" stroke-width=\"1\"/>",
            v.x, v.y, v.w, v.h
        );
    }

    let rotation: f64 = style
        .get("rotation")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);
    if rotation == 0.0 {
        out.push_str(&shape_svg);
    } else {
        let cx = v.x + v.w / 2.0;
        let cy = v.y + v.h / 2.0;
        let _ = write!(out, "<g transform=\"rotate({rotation} {cx} {cy})\">");
        out.push_str(&shape_svg);
        out.push_str("</g>");
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

/// Draw one edge's already-routed path ([`routing::route_edges`]). A
/// non-orthogonal edge (no `edgeStyle=orthogonalEdgeStyle`) ignores the
/// route's interior bends and draws a straight line between the same two
/// endpoints, matching drawio's own fallback for that style.
fn render_edge(out: &mut String, e: &Edge, route: &Route) {
    let style = StyleMap::parse(&e.style);
    let end_arrow = style.get_or("endArrow", "open");
    let marker_end = if end_arrow == "none" {
        ""
    } else {
        " marker-end=\"url(#arrow)\""
    };

    let orthogonal = style.get("edgeStyle") == Some("orthogonalEdgeStyle");
    if orthogonal && route.points.len() > 2 {
        let mut d = String::new();
        for (i, (x, y)) in route.points.iter().enumerate() {
            let cmd = if i == 0 { "M" } else { "L" };
            let _ = write!(d, "{cmd} {x} {y} ");
        }
        let _ = write!(
            out,
            "<path d=\"{}\" fill=\"none\" stroke=\"#232F3E\" stroke-width=\"1.5\"{marker_end}/>",
            d.trim_end()
        );
        return;
    }

    let (sx, sy) = route.points[0];
    let (tx, ty) = *route.points.last().expect("route always has >= 2 points");
    let _ = write!(
        out,
        "<line x1=\"{sx}\" y1=\"{sy}\" x2=\"{tx}\" y2=\"{ty}\" \
         stroke=\"#232F3E\" stroke-width=\"1.5\"{marker_end}/>"
    );
}

pub(crate) fn escape_text(s: &str) -> String {
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
    fn raw_opensearch_res_icon_resolves_via_alias() {
        // `mxgraph.aws4.opensearch_service` is the natural-looking resIcon a
        // hand-authored raw style reaches for, but no stencil carries that
        // name — drawio keys the OpenSearch tile on `elasticsearch_service`.
        // The alias must resolve it so the glyph renders instead of a bare
        // fill rect.
        let xml = r#"
<mxfile compressed="false"><diagram><mxGraphModel><root>
<mxCell id="0"/><mxCell id="1" parent="0"/>
<mxCell id="os" value="OpenSearch" vertex="1" parent="1" style="shape=mxgraph.aws4.resourceIcon;fillColor=#3334B9;resIcon=mxgraph.aws4.opensearch_service;">
  <mxGeometry x="40" y="40" width="78" height="78" as="geometry"/>
</mxCell>
</root></mxGraphModel></diagram></mxfile>"#;
        let svg = render(xml).unwrap();
        assert!(
            svg.contains("<path"),
            "expected stencil glyph path, got only: {svg}",
        );
    }

    #[test]
    fn cell_rotation_wraps_the_shape_in_an_svg_rotate_transform() {
        // Issue #7's <rotation> candidate: drawio's `rotation` style key
        // rotates a cell around its own centre, not the page. Confirm the
        // shape gets wrapped in a `rotate(deg cx cy)` transform group using
        // the cell's own centre, and that the underlying rect keeps its
        // unrotated (axis-aligned) coordinates — the transform does the
        // rotating, not pre-rotated geometry.
        let xml = r#"
<mxfile compressed="false"><diagram><mxGraphModel><root>
<mxCell id="0"/><mxCell id="1" parent="0"/>
<mxCell id="r" value="Tilted" vertex="1" parent="1" style="rounded=0;fillColor=#dae8fc;rotation=45;">
  <mxGeometry x="100" y="100" width="80" height="40" as="geometry"/>
</mxCell>
</root></mxGraphModel></diagram></mxfile>"#;
        let svg = render(xml).unwrap();
        assert!(
            svg.contains("<g transform=\"rotate(45 140 120)\">"),
            "expected a rotate transform around the cell centre; got: {svg}"
        );
        assert!(
            svg.contains("<rect x=\"100\" y=\"100\" width=\"80\" height=\"40\" fill=\"#dae8fc\""),
            "expected the rect's own coordinates to stay unrotated; got: {svg}"
        );
    }

    #[test]
    fn zero_rotation_omits_the_transform_wrapper() {
        let xml = r#"
<mxfile compressed="false"><diagram><mxGraphModel><root>
<mxCell id="0"/><mxCell id="1" parent="0"/>
<mxCell id="r" value="Flat" vertex="1" parent="1" style="rounded=0;fillColor=#dae8fc;">
  <mxGeometry x="10" y="10" width="80" height="40" as="geometry"/>
</mxCell>
</root></mxGraphModel></diagram></mxfile>"#;
        let svg = render(xml).unwrap();
        assert!(!svg.contains("rotate("));
    }

    #[test]
    fn aws4_res_icon_alias_maps_opensearch_to_elasticsearch() {
        assert_eq!(
            aws4_res_icon_alias("mxgraph.aws4.opensearch_service"),
            Some("elasticsearch_service"),
        );
        assert_eq!(
            aws4_res_icon_alias("opensearch_service"),
            Some("elasticsearch_service")
        );
        assert_eq!(aws4_res_icon_alias("mxgraph.aws4.lambda"), None);
    }

    #[test]
    fn renders_edge_with_explicit_entry_exit_overrides() {
        // Edge style declares exit/entry. The picker on the cells would
        // pick corners (only corner points declared) — the overrides must
        // win and force right-mid -> left-mid attachment.
        let xml = r#"
<mxfile compressed="false"><diagram><mxGraphModel><root>
<mxCell id="0"/><mxCell id="1" parent="0"/>
<mxCell id="a" value="A" vertex="1" parent="1" style="shape=mxgraph.aws4.resourceIcon;points=[[0,0,0],[1,0,0],[0,1,0],[1,1,0]];resIcon=mxgraph.aws4.lambda;fillColor=#ED7100;">
  <mxGeometry x="0" y="0" width="78" height="78" as="geometry"/>
</mxCell>
<mxCell id="b" value="B" vertex="1" parent="1" style="shape=mxgraph.aws4.resourceIcon;points=[[0,0,0],[1,0,0],[0,1,0],[1,1,0]];resIcon=mxgraph.aws4.lambda;fillColor=#ED7100;">
  <mxGeometry x="200" y="0" width="78" height="78" as="geometry"/>
</mxCell>
<mxCell id="e1" edge="1" parent="1" source="a" target="b" style="edgeStyle=orthogonalEdgeStyle;html=0;endArrow=open;startArrow=none;rounded=0;exitX=1;exitY=0.5;entryX=0;entryY=0.5;">
  <mxGeometry relative="1" as="geometry"/>
</mxCell>
</root></mxGraphModel></diagram></mxfile>"#;
        let svg = render(xml).unwrap();
        // Colinear (both endpoints at y=39): orthogonal router degrades to
        // a single straight segment, emitted as <line>.
        assert!(
            svg.contains("<line x1=\"78\" y1=\"39\" x2=\"200\" y2=\"39\""),
            "expected colinear straight line at y=39; got: {svg}",
        );
    }

    #[test]
    fn renders_orthogonal_edge_as_path_with_corner() {
        // Two AWS resource icons offset both horizontally and vertically
        // (issue #40's reported pattern) — the default picker lands on
        // A's right-mid and B's top-mid (perpendicular to the exit side),
        // never a corner, so the route bends and arrives head-on.
        let xml = r#"
<mxfile compressed="false"><diagram><mxGraphModel><root>
<mxCell id="0"/><mxCell id="1" parent="0"/>
<mxCell id="a" value="A" vertex="1" parent="1" style="shape=mxgraph.aws4.resourceIcon;resIcon=mxgraph.aws4.lambda;fillColor=#ED7100;">
  <mxGeometry x="0" y="0" width="78" height="78" as="geometry"/>
</mxCell>
<mxCell id="b" value="B" vertex="1" parent="1" style="shape=mxgraph.aws4.resourceIcon;resIcon=mxgraph.aws4.lambda;fillColor=#ED7100;">
  <mxGeometry x="300" y="200" width="78" height="78" as="geometry"/>
</mxCell>
<mxCell id="e1" edge="1" parent="1" source="a" target="b" style="edgeStyle=orthogonalEdgeStyle;html=0;endArrow=open;rounded=0;">
  <mxGeometry relative="1" as="geometry"/>
</mxCell>
</root></mxGraphModel></diagram></mxfile>"#;
        let svg = render(xml).unwrap();
        // |dx| (300) dominates |dy| (200) from A's centre (39, 39) to B's
        // centre (339, 239), so the exit is A's right-mid (78, 39). The
        // entry must be perpendicular to that (a top/bottom side of B),
        // oriented toward A: B's top-mid (339, 200). The corner sits at
        // (entry.x, exit.y) = (339, 39).
        assert!(
            svg.contains("<path d=\"M 78 39 L 339 39 L 339 200\""),
            "expected orthogonal path landing head-on; got: {svg}",
        );
    }

    #[test]
    fn straight_line_edge_when_edgestyle_missing() {
        // No `edgeStyle=orthogonalEdgeStyle` in the edge style — keep
        // the legacy straight-line behaviour. Endpoint defaulting is
        // unaffected by the missing `edgeStyle`; only the corner-bend
        // rendering is.
        let xml = r#"
<mxfile compressed="false"><diagram><mxGraphModel><root>
<mxCell id="0"/><mxCell id="1" parent="0"/>
<mxCell id="a" value="A" vertex="1" parent="1" style="shape=mxgraph.aws4.resourceIcon;resIcon=mxgraph.aws4.lambda;fillColor=#ED7100;">
  <mxGeometry x="0" y="0" width="78" height="78" as="geometry"/>
</mxCell>
<mxCell id="b" value="B" vertex="1" parent="1" style="shape=mxgraph.aws4.resourceIcon;resIcon=mxgraph.aws4.lambda;fillColor=#ED7100;">
  <mxGeometry x="300" y="200" width="78" height="78" as="geometry"/>
</mxCell>
<mxCell id="e1" edge="1" parent="1" source="a" target="b" style="endArrow=open;rounded=0;">
  <mxGeometry relative="1" as="geometry"/>
</mxCell>
</root></mxGraphModel></diagram></mxfile>"#;
        let svg = render(xml).unwrap();
        // No path-with-corner; the legacy <line> is emitted instead,
        // straight from A's right-mid to B's top-mid.
        assert!(
            svg.contains("<line x1=\"78\" y1=\"39\" x2=\"339\" y2=\"200\""),
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
