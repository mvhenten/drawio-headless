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
//! 4. For each edge: route between the picked connection points. An
//!    explicit `exitX/exitY`/`entryX/entryY` override always wins; absent
//!    an override, the endpoint snaps to the nearest side-centre of the
//!    cell (never a corner), and the unpinned side of the pair is chosen
//!    so the route's final segment lands perpendicular to the side it
//!    enters (see [`edge_endpoints`]). Edges declaring
//!    `edgeStyle=orthogonalEdgeStyle` render as a two-segment right-angle
//!    polyline (one corner); other edges fall back to a straight line.
//!    Endpoints carry a simple arrowhead.
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
//! Render fidelity is not 1:1 with the upstream drawio app. Azure and GCP
//! shapes lean heavily on stencil DSL commands that this renderer does not
//! yet fully implement (`<save>`/`<restore>`, `<alpha>`, `<strokecolor>`,
//! `<fillcolor>` — tracked in issue #7; `<arc>` is now supported). Those
//! remaining commands are silently skipped, so a shape's outer silhouette
//! may render with reduced detail.

pub mod inflate;
pub mod model;
pub mod stencil;
pub mod style;

use std::fmt::Write as _;
use std::sync::OnceLock;

use crate::model::{Edge, Model, Vertex};
use crate::stencil::{CellBounds, Stencil, StencilLibrary, render_stencil_to_svg};
use crate::style::{EdgeEndpoints, StyleMap};

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
    let aspect_fixed = style.get("aspect") == Some("fixed");
    let cell = CellBounds {
        x: v.x,
        y: v.y,
        w: v.w,
        h: v.h,
    };

    if shape == "mxgraph.aws4.resourceIcon" {
        // AWS resource-icon: coloured tile + white glyph from stencil.
        let _ = write!(
            out,
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{fill}\" \
             stroke=\"none\" rx=\"3\" ry=\"3\"/>",
            v.x, v.y, v.w, v.h
        );
        if let Some((stencil, _)) = resolve_stencil(&style) {
            out.push_str(&render_stencil_to_svg(
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
                    out,
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" \
                     fill=\"{tile_fill}\" stroke=\"none\" rx=\"4\" ry=\"4\"/>",
                    v.x, v.y, v.w, v.h
                );
                out.push_str(&render_stencil_to_svg(
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
                out.push_str(&render_stencil_to_svg(
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
    let style = StyleMap::parse(&e.style);
    let overrides = EdgeEndpoints::from_style(&style);
    let (sx, sy, tx, ty) = edge_endpoints(src, tgt, overrides.exit, overrides.entry);
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

/// One of the four cardinal sides of a rectangular cell an edge can attach
/// to. Defaults always resolve to a side's *centre* — never a corner
/// (issue #40).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Top,
    Bottom,
    Left,
    Right,
}

impl Side {
    /// `true` for the two sides that run horizontally (top/bottom). A
    /// segment travels perpendicular to the side it lands on head-on, so a
    /// route entering a horizontal side must arrive travelling vertically,
    /// and vice versa — see [`edge_endpoints`].
    fn is_horizontal_side(self) -> bool {
        matches!(self, Side::Top | Side::Bottom)
    }

    /// The other side of the same axis-aligned pair (`Top`/`Bottom` or
    /// `Left`/`Right`). Used for the colinear case, where the route is a
    /// single straight segment and both ends must share the same axis.
    fn opposite(self) -> Side {
        match self {
            Side::Top => Side::Bottom,
            Side::Bottom => Side::Top,
            Side::Left => Side::Right,
            Side::Right => Side::Left,
        }
    }
}

/// Absolute coordinate of `cell`'s side-centre for `side`.
fn side_centre(cell: &Vertex, side: Side) -> (f64, f64) {
    match side {
        Side::Top => (cell.x + cell.w / 2.0, cell.y),
        Side::Bottom => (cell.x + cell.w / 2.0, cell.y + cell.h),
        Side::Left => (cell.x, cell.y + cell.h / 2.0),
        Side::Right => (cell.x + cell.w, cell.y + cell.h / 2.0),
    }
}

/// Classify an explicit normalised override `(nx, ny)` (an `exitX/exitY` or
/// `entryX/entryY` pair, each `0..1`) by which side of the cell it sits on.
/// Used only to infer the *orientation* of a pinned anchor, so the other
/// (unpinned) end of the edge can still be defaulted sensibly — the pinned
/// point itself is always used verbatim.
///
/// An exact corner override (both members at `0`/`1`) is a tie; it reads as
/// the horizontal side (`Top`/`Bottom`), a deliberate, arbitrary but
/// deterministic choice — the caller pinned a corner on purpose, so there
/// is no "correct" orientation to recover.
fn side_of_override(nx: f32, ny: f32) -> Side {
    if ny <= 0.0 {
        Side::Top
    } else if ny >= 1.0 {
        Side::Bottom
    } else if nx <= 0.0 {
        Side::Left
    } else {
        Side::Right
    }
}

/// The side of a cell, restricted to the given orientation, that faces the
/// direction `(dx, dy)` points *away* from the cell — i.e. the side nearest
/// whatever lies in that direction. `horizontal` selects between
/// `Top`/`Bottom` (`true`) and `Left`/`Right` (`false`).
fn facing_side(horizontal: bool, dx: f64, dy: f64) -> Side {
    if horizontal {
        if dy >= 0.0 { Side::Bottom } else { Side::Top }
    } else if dx >= 0.0 {
        Side::Right
    } else {
        Side::Left
    }
}

/// Resolve both ends of an edge between `src` and `tgt`.
///
/// An explicit override (`exitX/exitY` or `entryX/entryY`) is always
/// honoured verbatim — this function never second-guesses a pinned anchor.
///
/// Absent an override, the endpoint snaps to one of the four side-centre
/// anchors, never a corner. When the unpinned end still needs a default
/// side, it is chosen relative to whichever side the *other* end already
/// has (pinned or just defaulted):
/// - boxes that are colinear (share an x or y, so a straight line reaches
///   head-on with no bend) mirror the other end's orientation — the same
///   axis, opposite side;
/// - otherwise an L-bend is unavoidable, and the router's single corner
///   always flips the direction of travel onto the perpendicular axis (see
///   [`orthogonal_corner`]), so the unpinned side must sit on *that* axis
///   to be entered head-on instead of sliding along it.
///
/// When neither end is pinned, the exit side is picked first — whichever
/// axis has the larger centre-to-centre offset — and the entry side then
/// follows from it by the same rule.
fn edge_endpoints(
    src: &Vertex,
    tgt: &Vertex,
    exit_override: Option<(f32, f32)>,
    entry_override: Option<(f32, f32)>,
) -> (f64, f64, f64, f64) {
    const EPS: f64 = 1e-6;
    let src_centre = (src.x + src.w / 2.0, src.y + src.h / 2.0);
    let tgt_centre = (tgt.x + tgt.w / 2.0, tgt.y + tgt.h / 2.0);
    let dx = tgt_centre.0 - src_centre.0;
    let dy = tgt_centre.1 - src_centre.1;
    let colinear = dx.abs() < EPS || dy.abs() < EPS;

    let (ex, ey, exit_side) = if let Some((nx, ny)) = exit_override {
        (
            src.x + f64::from(nx) * src.w,
            src.y + f64::from(ny) * src.h,
            side_of_override(nx, ny),
        )
    } else {
        let side = match entry_override {
            // Entry is pinned: exit takes the perpendicular axis
            // (or mirrors it, if the boxes are colinear).
            Some((enx, eny)) => {
                let entry_side = side_of_override(enx, eny);
                if colinear {
                    entry_side.opposite()
                } else {
                    facing_side(!entry_side.is_horizontal_side(), dx, dy)
                }
            }
            // Neither end pinned: exit leads, picked by the dominant
            // centre-to-centre axis.
            None => facing_side(dy.abs() >= dx.abs(), dx, dy),
        };
        let (x, y) = side_centre(src, side);
        (x, y, side)
    };

    let (tx, ty) = if let Some((nx, ny)) = entry_override {
        (tgt.x + f64::from(nx) * tgt.w, tgt.y + f64::from(ny) * tgt.h)
    } else {
        let side = if colinear {
            exit_side.opposite()
        } else {
            // Entry faces back toward the source, so the direction is
            // negated relative to the src -> tgt vector used above.
            facing_side(!exit_side.is_horizontal_side(), -dx, -dy)
        };
        side_centre(tgt, side)
    };

    (ex, ey, tx, ty)
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

    /// Build a plain, style-less `Vertex` at the given box — the new
    /// default-endpoint logic works purely off cell geometry, not a
    /// declared `points=` constraint set, so tests don't need one.
    fn plain_vertex(id: &str, x: f64, y: f64, w: f64, h: f64) -> Vertex {
        Vertex {
            id: id.into(),
            label: String::new(),
            style: String::new(),
            x,
            y,
            w,
            h,
        }
    }

    #[test]
    fn default_endpoints_snap_to_side_centres_same_row() {
        // Two boxes on the same row (aligned y): a straight horizontal
        // line, so both ends share the same orientation (left/right).
        let a = plain_vertex("a", 0.0, 0.0, 78.0, 78.0);
        let b = plain_vertex("b", 300.0, 0.0, 78.0, 78.0);
        let (sx, sy, tx, ty) = edge_endpoints(&a, &b, None, None);
        assert_eq!((sx, sy), (78.0, 39.0), "source should be A's right-mid");
        assert_eq!((tx, ty), (300.0, 39.0), "target should be B's left-mid");
    }

    #[test]
    fn default_endpoints_never_land_on_a_corner_when_diagonal() {
        // Two boxes offset both horizontally and vertically (issue #40's
        // reported pattern) — neither end may resolve to a corner, and the
        // route must land head-on: the exit side and entry side must be on
        // perpendicular axes so the router's single bend (see
        // `orthogonal_corner`) arrives travelling straight into the
        // entered side rather than sliding along it.
        let a = plain_vertex("a", 0.0, 0.0, 78.0, 78.0);
        let b = plain_vertex("b", 300.0, 200.0, 78.0, 78.0);
        let (sx, sy, tx, ty) = edge_endpoints(&a, &b, None, None);

        let is_corner = |x: f64, y: f64, v: &Vertex| {
            let touches_vertical_side = (x - v.x).abs() < 1e-9 || (x - (v.x + v.w)).abs() < 1e-9;
            let touches_horizontal_side = (y - v.y).abs() < 1e-9 || (y - (v.y + v.h)).abs() < 1e-9;
            touches_vertical_side && touches_horizontal_side
        };
        assert!(
            !is_corner(sx, sy, &a),
            "exit landed on a corner: ({sx}, {sy})"
        );
        assert!(
            !is_corner(tx, ty, &b),
            "entry landed on a corner: ({tx}, {ty})"
        );

        // |dy| (200) > |dx| (300... wait see below) decides the exit axis;
        // here |dx| dominates (300 > 200), so exit is A's right-mid.
        assert_eq!((sx, sy), (78.0, 39.0), "exit should be A's right-mid");
        // Perpendicular entry: exit is a left/right (vertical) side, so
        // entry must be a top/bottom (horizontal) side of B — B's top-mid,
        // since B sits below A.
        assert_eq!((tx, ty), (339.0, 200.0), "entry should be B's top-mid");
    }

    #[test]
    fn default_endpoints_pick_perpendicular_entry_when_vertical_offset_dominates() {
        // Mirror of the previous case with the dominant axis flipped:
        // B sits mostly below A rather than mostly beside it, so the exit
        // is a top/bottom side and the entry must be left/right.
        let a = plain_vertex("a", 0.0, 0.0, 78.0, 78.0);
        let b = plain_vertex("b", 200.0, 300.0, 78.0, 78.0);
        let (sx, sy, tx, ty) = edge_endpoints(&a, &b, None, None);
        assert_eq!((sx, sy), (39.0, 78.0), "exit should be A's bottom-mid");
        assert_eq!((tx, ty), (200.0, 339.0), "entry should be B's left-mid");
    }

    #[test]
    fn edge_endpoint_overrides_take_priority_over_defaults() {
        // Both ends explicitly pinned: the override wins verbatim even
        // though the boxes are diagonally offset (which would otherwise
        // pick different sides by default).
        let a = plain_vertex("a", 0.0, 0.0, 78.0, 78.0);
        let b = plain_vertex("b", 200.0, 300.0, 78.0, 78.0);
        let exit = Some((1.0_f32, 0.5_f32));
        let entry = Some((0.0_f32, 0.5_f32));
        let (sx, sy, tx, ty) = edge_endpoints(&a, &b, exit, entry);
        assert_eq!((sx, sy), (78.0, 39.0), "source must be right-mid (78, 39)");
        assert_eq!(
            (tx, ty),
            (200.0, 339.0),
            "target must be left-mid (200, 339)"
        );
    }

    #[test]
    fn one_sided_override_still_gets_a_perpendicular_default_partner() {
        // Only the exit is pinned (to A's bottom-mid); the entry is left
        // to default. Since A's bottom is a horizontal side, the entry
        // must default to a vertical (left/right) side of B, not a
        // corner and not another horizontal side.
        let a = plain_vertex("a", 0.0, 0.0, 78.0, 78.0);
        let b = plain_vertex("b", 300.0, 200.0, 78.0, 78.0);
        let exit = Some((0.5_f32, 1.0_f32));
        let (_, _, tx, ty) = edge_endpoints(&a, &b, exit, None);
        assert_eq!((tx, ty), (300.0, 239.0), "entry should be B's left-mid");
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
        // A point that isn't on any side of the cell (e.g. a cell centre,
        // which `edge_endpoints` never actually produces but this
        // lower-level helper doesn't assume) — there is no orientation to
        // pick.
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
