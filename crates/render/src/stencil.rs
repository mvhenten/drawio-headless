//! Parse drawio's mxStencil mini-DSL and emit equivalent SVG.
//!
//! The DSL lives in `<foreground>` elements of `<shape>` entries inside files
//! like `stencils/aws4.xml`. v0 supports the subset actually used by the AWS
//! catalog tiles we ship:
//!
//! - `<path>`, `<move>`, `<line>`, `<curve>`, `<quad>`, `<close/>`
//! - `<fill/>`, `<stroke/>`, `<fillstroke/>` (painters; we treat all as paint)
//! - `<ellipse>`, `<rect>`, `<roundrect>` (primitives that produce their own
//!   SVG nodes)
//!
//! Other commands (e.g. `<arc>`, `<save>`/`<restore>`, `<alpha>`,
//! `<strokecolor>`, `<fillcolor>`) are silently skipped — see issue #7. Some
//! libraries (notably Azure and GCP) rely on these and will render with
//! partial fidelity.
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
        b"move" | b"line" | b"curve" | b"quad" | b"close" | b"fill" | b"stroke" | b"fillstroke"
        | b"ellipse" | b"rect" | b"roundrect" => {
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
    ) -> Self {
        let pad_x = cell_w * pad;
        let pad_y = cell_h * pad;
        let draw_w = cell_w - 2.0 * pad_x;
        let draw_h = cell_h - 2.0 * pad_y;
        Self {
            cell_x,
            cell_y,
            pad_x,
            pad_y,
            sx: if stencil.w > 0.0 {
                draw_w / stencil.w
            } else {
                1.0
            },
            sy: if stencil.h > 0.0 {
                draw_h / stencil.h
            } else {
                1.0
            },
        }
    }
    fn tx(self, x: f64) -> f64 {
        self.cell_x + self.pad_x + x * self.sx
    }
    fn ty(self, y: f64) -> f64 {
        self.cell_y + self.pad_y + y * self.sy
    }
}

/// Render a stencil into an SVG `<g>` element, mapping its native
/// (`stencil.w`, `stencil.h`) coordinate system onto the destination tile.
///
/// `pad_ratio` controls inset (e.g. 0.12 = 12% padding inside the tile).
pub fn render_stencil_to_svg(
    stencil: &Stencil,
    cell_x: f64,
    cell_y: f64,
    cell_w: f64,
    cell_h: f64,
    pad_ratio: f64,
    glyph_color: &str,
) -> String {
    let tr = Transform::new(stencil, cell_x, cell_y, cell_w, cell_h, pad_ratio);
    let mut out = String::new();
    out.push_str("<g>");
    let mut path = String::new();
    for cmd in &stencil.commands {
        emit_cmd(cmd, tr, glyph_color, &mut out, &mut path);
    }
    out.push_str("</g>");
    out
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
        Cmd::Ellipse { x, y, w, h } => {
            let cx = tr.tx(*x + *w / 2.0);
            let cy = tr.ty(*y + *h / 2.0);
            let rx = (*w * tr.sx) / 2.0;
            let ry = (*h * tr.sy) / 2.0;
            let _ = write!(
                out,
                "<ellipse cx=\"{cx}\" cy=\"{cy}\" rx=\"{rx}\" ry=\"{ry}\" fill=\"{glyph_color}\"/>"
            );
        }
        Cmd::Rect { x, y, w, h } => {
            let _ = write!(
                out,
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"/>",
                tr.tx(*x),
                tr.ty(*y),
                *w * tr.sx,
                *h * tr.sy,
                glyph_color
            );
        }
        Cmd::RoundRect { x, y, w, h, arc } => {
            let r = arc * tr.sx;
            let _ = write!(
                out,
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" ry=\"{}\" fill=\"{}\"/>",
                tr.tx(*x),
                tr.ty(*y),
                *w * tr.sx,
                *h * tr.sy,
                r,
                r,
                glyph_color
            );
        }
    }
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
        let svg = render_stencil_to_svg(&s, 100.0, 100.0, 78.0, 78.0, 0.0, "#fff");
        assert!(svg.contains("<path"));
        assert!(svg.contains("fill=\"#fff\""));
    }
}
