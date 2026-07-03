//! Parse drawio's mxStencil mini-DSL and emit equivalent SVG.
//!
//! The DSL lives in `<foreground>` elements of `<shape>` entries inside files
//! like `stencils/aws4.xml`. v0 supports the subset actually used by the AWS
//! catalog tiles we ship:
//!
//! - `<path>`, `<move>`, `<line>`, `<curve>`, `<quad>`, `<arc>`, `<close/>`
//! - `<fill/>`, `<stroke/>`, `<fillstroke/>` (painters; we treat all as paint)
//! - `<ellipse>`, `<rect>`, `<roundrect>` (primitives that produce their own
//!   SVG nodes)
//!
//! `<arc>` maps directly onto an SVG elliptical arc (`A rx ry x-axis-rotation
//! large-arc-flag sweep-flag x y`) — drawio's mxStencil DSL and SVG share the
//! same arc parameterisation, so no curve-fitting is needed.
//!
//! Other commands (e.g. `<save>`/`<restore>`, `<alpha>`, `<strokecolor>`,
//! `<fillcolor>`) are silently skipped — see issue #7. Some libraries
//! (notably Azure and GCP) rely on these and will render with partial
//! fidelity.
//!
//! Multiple libraries
//! ------------------
//! Each stencil file declares a `<shapes name="mxgraph.<library>[.<sub>]">`
//! wrapper. Stencils are keyed inside [`StencilLibrary`] by the *suffix* of
//! that wrapper name (after the library prefix passed to [`StencilLibrary::from_xml`])
//! joined with the stencil's own `name` attribute. Concretely:
//!
//! - `<shapes name="mxgraph.aws4">` with `<shape name="lambda">` → key `lambda`.
//! - `<shapes name="mxgraph.azure">` with `<shape name="Virtual Machine">` →
//!   key `virtual_machine`.
//! - `<shapes name="mxgraph.gcp.compute">` with `<shape name="App Engine">` →
//!   key `compute.app_engine`.
//!
//! Stencil names in the source file use spaces and mixed case
//! (e.g. `"api gateway"`), while drawio's lookup keys (`resIcon`, `prIcon`, or
//! the bare `shape=` suffix) use lower-case underscores (`api_gateway`). The
//! lookup normalises both directions.

use std::collections::HashMap;
use std::fmt::Write as _;

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::RenderError;

/// One parsed stencil. Keeps the foreground commands ready to render.
#[derive(Debug, Clone)]
pub struct Stencil {
    pub name: String,
    pub w: f64,
    pub h: f64,
    pub commands: Vec<Cmd>,
}

#[derive(Debug, Clone)]
pub enum Cmd {
    PathBegin,
    Move(f64, f64),
    Line(f64, f64),
    Curve {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        x3: f64,
        y3: f64,
    },
    Quad {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
    },
    Arc {
        rx: f64,
        ry: f64,
        x_axis_rotation: f64,
        large_arc_flag: bool,
        sweep_flag: bool,
        x: f64,
        y: f64,
    },
    Close,
    /// End-of-path paint. The renderer applies the current fill colour.
    Paint,
    Ellipse {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    },
    Rect {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    },
    RoundRect {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        arc: f64,
    },
}

/// Library of all stencils loaded from a single XML source.
#[derive(Debug, Default, Clone)]
pub struct StencilLibrary {
    /// Keys are dotted relative paths from the library prefix down to each
    /// stencil's normalised name (lowercase, spaces/dashes -> underscores).
    /// For single-namespace libraries (AWS, Azure, Kubernetes) the key is
    /// just the stencil name. For multi-namespace libraries (GCP) the key
    /// is `<category>.<stencil>`, e.g. `compute.app_engine`.
    by_key: HashMap<String, Stencil>,
}

impl StencilLibrary {
    /// Load all `<shape>` entries from a stencil XML document, treating
    /// `<shapes name="<library_prefix>[.<sub>]">` wrapper elements as
    /// category-scoping. The library prefix is stripped from each wrapper's
    /// `name` to derive the relative category path; bare stencils with no
    /// category form keys equal to their normalised stencil name.
    pub fn from_xml(xml: &str, library_prefix: &str) -> Result<Self, RenderError> {
        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(true);
        let mut buf = Vec::new();

        let mut lib = Self::default();
        let mut current: Option<Stencil> = None;
        let mut in_foreground = false;
        let mut category: String = String::new();

        loop {
            let evt = reader
                .read_event_into(&mut buf)
                .map_err(|e| RenderError::Xml(format!("stencil parse: {e}")))?;
            match evt {
                Event::Eof => break,
                Event::Start(elem) | Event::Empty(elem) => {
                    if elem.name().as_ref() == b"shapes" {
                        category = read_category(&elem, &reader, library_prefix)?;
                    } else {
                        handle_open(&elem, &reader, &mut current, &mut in_foreground)?;
                    }
                }
                Event::End(elem) => match elem.name().as_ref() {
                    b"shape" => {
                        if let Some(stencil) = current.take() {
                            let stencil_key = normalise_stencil_key(&stencil.name);
                            let full = if category.is_empty() {
                                stencil_key
                            } else {
                                format!("{category}.{stencil_key}")
                            };
                            lib.by_key.insert(full, stencil);
                        }
                        in_foreground = false;
                    }
                    b"shapes" => category.clear(),
                    b"foreground" => in_foreground = false,
                    _ => {}
                },
                _ => {}
            }
            buf.clear();
        }

        Ok(lib)
    }

    /// Look up a stencil by a dotted lookup path *relative to the library
    /// prefix*. For example, given an AWS library loaded with prefix
    /// `mxgraph.aws4`:
    /// - `lookup("lambda")` resolves the `lambda` stencil.
    /// - `lookup("mxgraph.aws4.lambda")` also resolves it (leading library
    ///   prefix is tolerated).
    ///
    /// For GCP loaded with prefix `mxgraph.gcp`:
    /// - `lookup("compute.app_engine")` resolves the App Engine stencil
    ///   from the `mxgraph.gcp.compute` namespace.
    pub fn lookup(&self, path: &str) -> Option<&Stencil> {
        // Tolerate callers passing the full `mxgraph.<lib>.<...>` style by
        // checking both the verbatim normalised key and progressively
        // shorter suffixes.
        let normalised = normalise_lookup_path(path);
        if let Some(s) = self.by_key.get(&normalised) {
            return Some(s);
        }
        // Try stripping leading segments one at a time — handles
        // `mxgraph.aws4.lambda` -> `lambda` without the caller needing to
        // know the library prefix.
        let mut rest = normalised.as_str();
        while let Some(idx) = rest.find('.') {
            rest = &rest[idx + 1..];
            if let Some(s) = self.by_key.get(rest) {
                return Some(s);
            }
        }
        None
    }

    /// Number of stencils loaded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_key.len()
    }

    /// Whether the library is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_key.is_empty()
    }
}

fn read_category(
    elem: &BytesStart<'_>,
    reader: &Reader<&[u8]>,
    library_prefix: &str,
) -> Result<String, RenderError> {
    for attr in elem.attributes() {
        let attr = attr.map_err(|err| RenderError::Xml(err.to_string()))?;
        if attr.key.as_ref() == b"name" {
            let value = attr
                .decode_and_unescape_value(reader.decoder())
                .map_err(|er| RenderError::Xml(er.to_string()))?;
            return Ok(category_suffix(&value, library_prefix));
        }
    }
    Ok(String::new())
}

/// Extract the relative category portion of a `<shapes name="...">` value
/// by stripping the library prefix. Returns an empty string when the value
/// equals the prefix (single-namespace library) or does not match it.
fn category_suffix(shapes_name: &str, library_prefix: &str) -> String {
    if let Some(rest) = shapes_name.strip_prefix(library_prefix) {
        let rest = rest.strip_prefix('.').unwrap_or(rest);
        normalise_lookup_path(rest)
    } else {
        String::new()
    }
}

fn normalise_lookup_path(path: &str) -> String {
    path.trim().to_ascii_lowercase().replace([' ', '-'], "_")
}

fn handle_open(
    elem: &BytesStart<'_>,
    reader: &Reader<&[u8]>,
    current: &mut Option<Stencil>,
    in_foreground: &mut bool,
) -> Result<(), RenderError> {
    let local_owned = elem.name().as_ref().to_vec();
    let local: &[u8] = &local_owned;
    match local {
        b"shape" => {
            *current = Some(parse_shape(elem, reader)?);
        }
        b"foreground" => {
            *in_foreground = true;
        }
        b"path" => {
            if *in_foreground && let Some(s) = current.as_mut() {
                s.commands.push(Cmd::PathBegin);
            }
        }
        b"move" | b"line" | b"curve" | b"quad" | b"arc" | b"close" | b"fill" | b"stroke"
        | b"fillstroke" | b"ellipse" | b"rect" | b"roundrect" => {
            if !*in_foreground {
                return Ok(());
            }
            let Some(stencil) = current.as_mut() else {
                return Ok(());
            };
            let attrs = collect_attrs(elem, reader)?;
            stencil.commands.push(build_cmd(local, &attrs));
        }
        _ => {}
    }
    Ok(())
}

fn parse_shape(elem: &BytesStart<'_>, reader: &Reader<&[u8]>) -> Result<Stencil, RenderError> {
    let mut name = String::new();
    let mut width = 0.0;
    let mut height = 0.0;
    for attr in elem.attributes() {
        let attr = attr.map_err(|err| RenderError::Xml(err.to_string()))?;
        let raw = attr
            .decode_and_unescape_value(reader.decoder())
            .map_err(|er| RenderError::Xml(er.to_string()))?
            .into_owned();
        match attr.key.as_ref() {
            b"name" => name = raw,
            b"w" => width = raw.parse().unwrap_or(0.0),
            b"h" => height = raw.parse().unwrap_or(0.0),
            _ => {}
        }
    }
    Ok(Stencil {
        name,
        w: width,
        h: height,
        commands: Vec::new(),
    })
}

fn build_cmd(local: &[u8], attrs: &HashMap<String, String>) -> Cmd {
    match local {
        b"move" => Cmd::Move(num(attrs, "x"), num(attrs, "y")),
        b"line" => Cmd::Line(num(attrs, "x"), num(attrs, "y")),
        b"curve" => Cmd::Curve {
            x1: num(attrs, "x1"),
            y1: num(attrs, "y1"),
            x2: num(attrs, "x2"),
            y2: num(attrs, "y2"),
            x3: num(attrs, "x3"),
            y3: num(attrs, "y3"),
        },
        b"quad" => Cmd::Quad {
            x1: num(attrs, "x1"),
            y1: num(attrs, "y1"),
            x2: num(attrs, "x2"),
            y2: num(attrs, "y2"),
        },
        b"arc" => Cmd::Arc {
            rx: num(attrs, "rx"),
            ry: num(attrs, "ry"),
            x_axis_rotation: num(attrs, "x-axis-rotation"),
            large_arc_flag: flag(attrs, "large-arc-flag"),
            sweep_flag: flag(attrs, "sweep-flag"),
            x: num(attrs, "x"),
            y: num(attrs, "y"),
        },
        b"close" => Cmd::Close,
        b"fill" | b"stroke" | b"fillstroke" => Cmd::Paint,
        b"ellipse" => Cmd::Ellipse {
            x: num(attrs, "x"),
            y: num(attrs, "y"),
            w: num(attrs, "w"),
            h: num(attrs, "h"),
        },
        b"rect" => Cmd::Rect {
            x: num(attrs, "x"),
            y: num(attrs, "y"),
            w: num(attrs, "w"),
            h: num(attrs, "h"),
        },
        b"roundrect" => Cmd::RoundRect {
            x: num(attrs, "x"),
            y: num(attrs, "y"),
            w: num(attrs, "w"),
            h: num(attrs, "h"),
            arc: num(attrs, "arcsize"),
        },
        _ => unreachable!("checked by caller"),
    }
}

fn collect_attrs(
    elem: &BytesStart<'_>,
    reader: &Reader<&[u8]>,
) -> Result<HashMap<String, String>, RenderError> {
    let mut map = HashMap::new();
    for attr in elem.attributes() {
        let attr = attr.map_err(|err| RenderError::Xml(err.to_string()))?;
        let raw = attr
            .decode_and_unescape_value(reader.decoder())
            .map_err(|er| RenderError::Xml(er.to_string()))?
            .into_owned();
        let key = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
        map.insert(key, raw);
    }
    Ok(map)
}

fn num(map: &HashMap<String, String>, key: &str) -> f64 {
    map.get(key).and_then(|s| s.parse().ok()).unwrap_or(0.0)
}

/// Parse a `0`/`1` mxStencil boolean attribute (used by `<arc>`'s
/// `large-arc-flag` and `sweep-flag`). Any value other than `"1"` is `false`,
/// matching the DSL's convention.
fn flag(map: &HashMap<String, String>, key: &str) -> bool {
    map.get(key).map(String::as_str) == Some("1")
}

fn normalise_stencil_key(name: &str) -> String {
    normalise_lookup_path(name)
}

/// Coordinate transform from a stencil's native (`stencil.w`, `stencil.h`)
/// space onto the destination tile, with a uniform padding inset.
#[derive(Clone, Copy)]
struct Transform {
    cell_x: f64,
    cell_y: f64,
    pad_x: f64,
    pad_y: f64,
    sx: f64,
    sy: f64,
}

impl Transform {
    fn new(
        stencil: &Stencil,
        cell_x: f64,
        cell_y: f64,
        cell_w: f64,
        cell_h: f64,
        pad: f64,
        aspect_fixed: bool,
    ) -> Self {
        let pad_x = cell_w * pad;
        let pad_y = cell_h * pad;
        let draw_w = cell_w - 2.0 * pad_x;
        let draw_h = cell_h - 2.0 * pad_y;
        let sx = if stencil.w > 0.0 {
            draw_w / stencil.w
        } else {
            1.0
        };
        let sy = if stencil.h > 0.0 {
            draw_h / stencil.h
        } else {
            1.0
        };
        if !aspect_fixed {
            return Self {
                cell_x,
                cell_y,
                pad_x,
                pad_y,
                sx,
                sy,
            };
        }
        let s = sx.min(sy);
        let extra_x = (draw_w - stencil.w * s) / 2.0;
        let extra_y = (draw_h - stencil.h * s) / 2.0;
        Self {
            cell_x,
            cell_y,
            pad_x: pad_x + extra_x,
            pad_y: pad_y + extra_y,
            sx: s,
            sy: s,
        }
    }
    fn tx(self, x: f64) -> f64 {
        self.cell_x + self.pad_x + x * self.sx
    }
    fn ty(self, y: f64) -> f64 {
        self.cell_y + self.pad_y + y * self.sy
    }
}

/// Destination tile bounds a stencil is rendered into.
#[derive(Clone, Copy)]
pub struct CellBounds {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Render a stencil into an SVG `<g>` element, mapping its native
/// (`stencil.w`, `stencil.h`) coordinate system onto the destination tile.
///
/// `pad_ratio` controls inset (e.g. 0.12 = 12% padding inside the tile).
pub fn render_stencil_to_svg(
    stencil: &Stencil,
    cell: CellBounds,
    pad_ratio: f64,
    glyph_color: &str,
    aspect_fixed: bool,
) -> String {
    let tr = Transform::new(
        stencil,
        cell.x,
        cell.y,
        cell.w,
        cell.h,
        pad_ratio,
        aspect_fixed,
    );
    let mut out = String::new();
    out.push_str("<g>");
    let mut path = String::new();
    for cmd in &stencil.commands {
        emit_cmd(cmd, tr, glyph_color, &mut out, &mut path);
    }
    out.push_str("</g>");
    out
}

/// Append an SVG elliptical arc command (`A rx ry x-axis-rotation
/// large-arc-flag sweep-flag x y`) to `path`. Split out of [`emit_cmd`] to
/// keep that function under clippy's line-count lint.
#[allow(clippy::too_many_arguments)]
fn write_arc(
    path: &mut String,
    tr: Transform,
    rx: f64,
    ry: f64,
    x_axis_rotation: f64,
    large_arc_flag: bool,
    sweep_flag: bool,
    x: f64,
    y: f64,
) {
    let _ = write!(
        path,
        "A {} {} {} {} {} {} {} ",
        (rx * tr.sx).abs(),
        (ry * tr.sy).abs(),
        x_axis_rotation,
        i32::from(large_arc_flag),
        i32::from(sweep_flag),
        tr.tx(x),
        tr.ty(y)
    );
}

fn emit_cmd(cmd: &Cmd, tr: Transform, glyph_color: &str, out: &mut String, path: &mut String) {
    match cmd {
        Cmd::PathBegin => path.clear(),
        Cmd::Move(x, y) => {
            let _ = write!(path, "M {} {} ", tr.tx(*x), tr.ty(*y));
        }
        Cmd::Line(x, y) => {
            let _ = write!(path, "L {} {} ", tr.tx(*x), tr.ty(*y));
        }
        Cmd::Curve {
            x1,
            y1,
            x2,
            y2,
            x3,
            y3,
        } => {
            let _ = write!(
                path,
                "C {} {} {} {} {} {} ",
                tr.tx(*x1),
                tr.ty(*y1),
                tr.tx(*x2),
                tr.ty(*y2),
                tr.tx(*x3),
                tr.ty(*y3)
            );
        }
        Cmd::Quad { x1, y1, x2, y2 } => {
            let _ = write!(
                path,
                "Q {} {} {} {} ",
                tr.tx(*x1),
                tr.ty(*y1),
                tr.tx(*x2),
                tr.ty(*y2)
            );
        }
        Cmd::Arc {
            rx,
            ry,
            x_axis_rotation,
            large_arc_flag,
            sweep_flag,
            x,
            y,
        } => write_arc(
            path,
            tr,
            *rx,
            *ry,
            *x_axis_rotation,
            *large_arc_flag,
            *sweep_flag,
            *x,
            *y,
        ),
        Cmd::Close => path.push_str("Z "),
        Cmd::Paint => {
            if !path.is_empty() {
                let _ = write!(
                    out,
                    "<path d=\"{}\" fill=\"{}\" fill-rule=\"evenodd\" stroke=\"none\"/>",
                    path.trim(),
                    glyph_color
                );
                path.clear();
            }
        }
        Cmd::Ellipse { x, y, w, h } => emit_ellipse(tr, glyph_color, out, *x, *y, *w, *h),
        Cmd::Rect { x, y, w, h } => emit_rect(tr, glyph_color, out, *x, *y, *w, *h, 0.0),
        Cmd::RoundRect { x, y, w, h, arc } => {
            emit_rect(tr, glyph_color, out, *x, *y, *w, *h, *arc);
        }
    }
}

/// Emit an `<ellipse>` primitive (the `<ellipse>` stencil command).
#[allow(clippy::many_single_char_names)]
fn emit_ellipse(
    tr: Transform,
    glyph_color: &str,
    out: &mut String,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) {
    let cx = tr.tx(x + w / 2.0);
    let cy = tr.ty(y + h / 2.0);
    let rx = (w * tr.sx) / 2.0;
    let ry = (h * tr.sy) / 2.0;
    let _ = write!(
        out,
        "<ellipse cx=\"{cx}\" cy=\"{cy}\" rx=\"{rx}\" ry=\"{ry}\" fill=\"{glyph_color}\"/>"
    );
}

/// Emit a `<rect>` primitive (the `<rect>`/`<roundrect>` stencil commands —
/// `corner_arc` is `0.0` for a plain rect).
#[allow(clippy::too_many_arguments, clippy::many_single_char_names)]
fn emit_rect(
    tr: Transform,
    glyph_color: &str,
    out: &mut String,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    corner_arc: f64,
) {
    let r = corner_arc * tr.sx;
    let _ = write!(
        out,
        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" ry=\"{}\" fill=\"{}\"/>",
        tr.tx(x),
        tr.ty(y),
        w * tr.sx,
        h * tr.sy,
        r,
        r,
        glyph_color
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_lambda_stencil() {
        let xml = std::fs::read_to_string("../../stencils/aws4.xml").expect("stencil file");
        let lib = StencilLibrary::from_xml(&xml, "mxgraph.aws4").unwrap();
        let s = lib.lookup("mxgraph.aws4.lambda").expect("lambda present");
        assert!(s.w > 0.0 && s.h > 0.0);
        assert!(!s.commands.is_empty());
    }

    #[test]
    fn loads_api_gateway_stencil() {
        let xml = std::fs::read_to_string("../../stencils/aws4.xml").expect("stencil file");
        let lib = StencilLibrary::from_xml(&xml, "mxgraph.aws4").unwrap();
        let s = lib.lookup("mxgraph.aws4.api_gateway").expect("present");
        assert!(s.w > 0.0 && s.h > 0.0);
    }

    #[test]
    fn loads_azure_virtual_machine() {
        let xml = std::fs::read_to_string("../../stencils/azure.xml").expect("stencil file");
        let lib = StencilLibrary::from_xml(&xml, "mxgraph.azure").unwrap();
        // Azure declares the stencil name as "Virtual Machine"; normalisation
        // makes it `virtual_machine`. Lookup tolerates the full prefix.
        assert!(lib.lookup("virtual_machine").is_some());
        assert!(lib.lookup("mxgraph.azure.virtual_machine").is_some());
        // SQL Database is one of the popular catalogued shapes.
        assert!(lib.lookup("sql_database").is_some());
        // Library is non-trivial in size.
        assert!(lib.len() >= 80, "azure shape count: {}", lib.len());
    }

    #[test]
    fn loads_kubernetes_pod_and_api() {
        let xml = std::fs::read_to_string("../../stencils/kubernetes.xml").expect("stencil file");
        let lib = StencilLibrary::from_xml(&xml, "mxgraph.kubernetes").unwrap();
        assert!(lib.lookup("pod").is_some());
        assert!(lib.lookup("api").is_some());
        // K8s sidebar references `prIcon=pod` directly with no library
        // prefix; the bare name must resolve.
        assert!(lib.lookup("deploy").is_some());
    }

    #[test]
    fn loads_gcp_with_category_paths() {
        let xml = std::fs::read_to_string("../../stencils/gcp.xml").expect("stencil file");
        let lib = StencilLibrary::from_xml(&xml, "mxgraph.gcp").unwrap();
        // The full `mxgraph.gcp.compute.app_engine` style id should resolve.
        assert!(lib.lookup("mxgraph.gcp.compute.app_engine").is_some());
        assert!(lib.lookup("compute.app_engine").is_some());
        // BigQuery lives under big_data.
        assert!(lib.lookup("big_data.bigquery").is_some());
        // Cloud Storage under storage_databases.
        assert!(lib.lookup("storage_databases.cloud_storage").is_some());
    }

    #[test]
    fn renders_path() {
        let s = Stencil {
            name: "t".into(),
            w: 10.0,
            h: 10.0,
            commands: vec![
                Cmd::PathBegin,
                Cmd::Move(0.0, 0.0),
                Cmd::Line(10.0, 10.0),
                Cmd::Close,
                Cmd::Paint,
            ],
        };
        let cell = CellBounds {
            x: 100.0,
            y: 100.0,
            w: 78.0,
            h: 78.0,
        };
        let svg = render_stencil_to_svg(&s, cell, 0.0, "#fff", false);
        assert!(svg.contains("<path"));
        assert!(svg.contains("fill=\"#fff\""));
    }

    #[test]
    fn renders_arc_as_svg_elliptical_arc_command() {
        // Fixture stencil: a rounded-corner tab built from `<arc>` segments,
        // the shape drawio's own "document" and "queue" AWS glyphs and the
        // Azure "Cloud" stencil all rely on (see issue #7 — `<arc>` was
        // previously silently skipped, leaving these glyphs' rounded
        // corners/silhouette missing).
        let s = Stencil {
            name: "rounded-tab".into(),
            w: 10.0,
            h: 10.0,
            commands: vec![
                Cmd::PathBegin,
                Cmd::Move(1.5, 0.0),
                Cmd::Line(8.5, 0.0),
                Cmd::Arc {
                    rx: 1.5,
                    ry: 1.5,
                    x_axis_rotation: 0.0,
                    large_arc_flag: false,
                    sweep_flag: true,
                    x: 10.0,
                    y: 1.5,
                },
                Cmd::Line(10.0, 10.0),
                Cmd::Line(0.0, 10.0),
                Cmd::Line(0.0, 1.5),
                Cmd::Arc {
                    rx: 1.5,
                    ry: 1.5,
                    x_axis_rotation: 0.0,
                    large_arc_flag: false,
                    sweep_flag: true,
                    x: 1.5,
                    y: 0.0,
                },
                Cmd::Close,
                Cmd::Paint,
            ],
        };
        let cell = CellBounds {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        };
        let svg = render_stencil_to_svg(&s, cell, 0.0, "#fff", false);
        // Emitted as a real SVG "A rx ry rot large sweep x y" arc command —
        // not skipped, not approximated with a line.
        assert!(
            svg.contains("A 1.5 1.5 0 0 1 10 1.5"),
            "expected SVG arc command with unscaled radii; got: {svg}"
        );
        assert!(
            svg.contains("A 1.5 1.5 0 0 1 1.5 0"),
            "expected second SVG arc command; got: {svg}"
        );
    }

    #[test]
    fn arc_flags_parse_only_literal_one_as_true() {
        let mut attrs = HashMap::new();
        attrs.insert("large-arc-flag".to_string(), "1".to_string());
        attrs.insert("sweep-flag".to_string(), "0".to_string());
        assert!(flag(&attrs, "large-arc-flag"));
        assert!(!flag(&attrs, "sweep-flag"));
        assert!(!flag(&attrs, "missing"));
    }

    #[test]
    fn aspect_fixed_scales_uniformly_and_centers() {
        let s = Stencil {
            name: "t".into(),
            w: 76.0,
            h: 76.0,
            commands: vec![
                Cmd::PathBegin,
                Cmd::Move(0.0, 0.0),
                Cmd::Line(76.0, 0.0),
                Cmd::Line(76.0, 76.0),
                Cmd::Line(0.0, 76.0),
                Cmd::Close,
                Cmd::Paint,
            ],
        };
        let cell = CellBounds {
            x: 0.0,
            y: 0.0,
            w: 240.0,
            h: 70.0,
        };
        let svg = render_stencil_to_svg(&s, cell, 0.0, "#fff", true);
        let coords = extract_path_coords(&svg);
        let (min_x, max_x, min_y, max_y) = bbox(&coords);
        let width = max_x - min_x;
        let height = max_y - min_y;
        assert!(
            (width - height).abs() < 1e-6,
            "expected square bbox, got {width}x{height}"
        );
        let left_gap = min_x - 0.0;
        let right_gap = 240.0 - max_x;
        assert!(
            (left_gap - right_gap).abs() < 1e-6,
            "expected icon centered horizontally, left={left_gap} right={right_gap}"
        );
    }

    fn extract_path_coords(svg: &str) -> Vec<(f64, f64)> {
        let start = svg.find("d=\"").expect("path d attribute") + 3;
        let end = svg[start..].find('"').expect("closing quote") + start;
        let d = &svg[start..end];
        d.split_whitespace()
            .filter_map(|tok| tok.parse::<f64>().ok())
            .collect::<Vec<_>>()
            .chunks(2)
            .map(|pair| (pair[0], pair[1]))
            .collect()
    }

    fn bbox(coords: &[(f64, f64)]) -> (f64, f64, f64, f64) {
        let mut min_x = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for &(x, y) in coords {
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            min_y = min_y.min(y);
            max_y = max_y.max(y);
        }
        (min_x, max_x, min_y, max_y)
    }
}
