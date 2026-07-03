//! Parse drawio's mxStencil mini-DSL and emit equivalent SVG.
//!
//! The DSL lives in `<foreground>` elements of `<shape>` entries inside files
//! like `stencils/aws4.xml`. Supported commands:
//!
//! - `<path>`, `<move>`, `<line>`, `<curve>`, `<quad>`, `<arc>`, `<close/>`
//! - `<fill/>`, `<stroke/>`, `<fillstroke/>` — paint the accumulated path
//!   using the current graphics state (see below).
//! - `<ellipse>`, `<rect>`, `<roundrect>` (primitives that produce their own
//!   SVG nodes, painted immediately with the current fill colour)
//! - `<save/>` / `<restore/>` — push/pop the graphics state (fill colour,
//!   stroke colour, stroke width, dashed flag), mirroring
//!   `mxAbstractCanvas2D.save`/`.restore`.
//! - `<strokecolor>`, `<fillcolor>`, `<fontcolor>`, `<strokewidth>`,
//!   `<dashed>` — style overrides that mutate the graphics state; a
//!   subsequent paint command picks up the new values.
//! - `<text>` — literal (non-placeholder) text, painted with the current
//!   font colour.
//!
//! `<arc>` maps directly onto an SVG elliptical arc (`A rx ry x-axis-rotation
//! large-arc-flag sweep-flag x y`) — drawio's mxStencil DSL and SVG share the
//! same arc parameterisation, so no curve-fitting is needed.
//!
//! `<image>` cannot be supported (no raster embedding in this crate) and is
//! rejected with [`RenderError::UnsupportedStencilCmd`] as soon as it is
//! parsed, rather than silently skipped — see issue #7.
//!
//! Commands outside the candidate list tracked by issue #7 (`<alpha>`,
//! `<dashpattern>`, `<linecap>`, `<linejoin>`, `<miterlimit>`, `<fontstyle>`,
//! `<fontfamily>`, `<fontsize>`, `<include-shape>`) remain silently skipped.
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
    /// Push a copy of the current graphics state (fill/stroke colour,
    /// stroke width, dashed flag) onto a stack — `<save/>`.
    Save,
    /// Pop the graphics state stack, restoring the previous values —
    /// `<restore/>`. A no-op if the stack is empty (mirrors
    /// `mxAbstractCanvas2D.restore`, which only pops when non-empty).
    Restore,
    /// Fill the accumulated path with the current fill colour — `<fill/>`.
    Fill,
    /// Stroke the accumulated path with the current stroke colour/width —
    /// `<stroke/>`.
    Stroke,
    /// Fill *and* stroke the accumulated path — `<fillstroke/>`.
    FillStroke,
    /// `<strokecolor color="..."/>` — updates the graphics state.
    SetStrokeColor(String),
    /// `<fillcolor color="..."/>` — updates the graphics state.
    SetFillColor(String),
    /// `<fontcolor color="..."/>` — updates the graphics state (used by
    /// `<text>`).
    SetFontColor(String),
    /// `<strokewidth width="..." fixed="0|1"/>`. `fixed="1"` uses `width`
    /// verbatim; otherwise it is scaled by the transform's minimum axis
    /// scale, matching `mxStencil`'s `minScale` multiplier.
    SetStrokeWidth {
        width: f64,
        fixed: bool,
    },
    /// `<dashed dashed="0|1"/>` — updates the graphics state.
    SetDashed(bool),
    /// `<text x="" y="" str="" align="" valign=""/>` — literal text, painted
    /// with the current font colour. Placeholder substitution
    /// (`evaluateTextAttribute` in upstream mxStencil) is not implemented;
    /// `str` is used verbatim.
    Text {
        x: f64,
        y: f64,
        text: String,
        align: String,
        valign: String,
    },
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
        b"image" => {
            // No raster embedding in this crate — surface the dedicated
            // error rather than silently dropping the glyph detail (issue
            // #7). Only counts inside <foreground>; <background> content is
            // not parsed at all today, so it would never reach here anyway.
            if *in_foreground {
                return Err(RenderError::UnsupportedStencilCmd("image".to_string()));
            }
        }
        b"move" | b"line" | b"curve" | b"quad" | b"arc" | b"close" | b"fill" | b"stroke"
        | b"fillstroke" | b"ellipse" | b"rect" | b"roundrect" | b"save" | b"restore"
        | b"strokecolor" | b"fillcolor" | b"fontcolor" | b"strokewidth" | b"dashed" | b"text" => {
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
        b"save" => Cmd::Save,
        b"restore" => Cmd::Restore,
        b"fill" => Cmd::Fill,
        b"stroke" => Cmd::Stroke,
        b"fillstroke" => Cmd::FillStroke,
        b"strokecolor" => Cmd::SetStrokeColor(color(attrs, "color")),
        b"fillcolor" => Cmd::SetFillColor(color(attrs, "color")),
        b"fontcolor" => Cmd::SetFontColor(color(attrs, "color")),
        b"strokewidth" => Cmd::SetStrokeWidth {
            width: num(attrs, "width"),
            fixed: flag(attrs, "fixed"),
        },
        b"dashed" => Cmd::SetDashed(flag(attrs, "dashed")),
        b"text" => Cmd::Text {
            x: num(attrs, "x"),
            y: num(attrs, "y"),
            text: attrs.get("str").cloned().unwrap_or_default(),
            align: attrs
                .get("align")
                .cloned()
                .unwrap_or_else(|| "left".to_string()),
            valign: attrs
                .get("valign")
                .cloned()
                .unwrap_or_else(|| "top".to_string()),
        },
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

/// Read a colour attribute (`<strokecolor color="..."/>` and friends).
/// `"none"` is a valid SVG paint keyword (transparent), so colour values are
/// passed through verbatim rather than translated. A missing attribute
/// defaults to `"none"` — a no-op paint, the safest fallback for malformed
/// input.
fn color(map: &HashMap<String, String>, key: &str) -> String {
    map.get(key).cloned().unwrap_or_else(|| "none".to_string())
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

/// Graphics state mutated by `<save>`/`<restore>` and the style-override
/// commands (`<strokecolor>`, `<fillcolor>`, `<fontcolor>`, `<strokewidth>`,
/// `<dashed>`), mirroring the handful of fields `mxAbstractCanvas2D`'s state
/// stack tracks. `stroke_width` is stored already scaled (see
/// [`Cmd::SetStrokeWidth`]), matching upstream's `setStrokeWidth`, which
/// receives an already-multiplied value.
#[derive(Clone)]
struct PaintState {
    fill_color: String,
    stroke_color: String,
    stroke_width: f64,
    dashed: bool,
    font_color: String,
}

/// Render a stencil into an SVG `<g>` element, mapping its native
/// (`stencil.w`, `stencil.h`) coordinate system onto the destination tile.
///
/// `pad_ratio` controls inset (e.g. 0.12 = 12% padding inside the tile).
/// `glyph_color` seeds the initial fill/stroke/font colour; `<fillcolor>`,
/// `<strokecolor>`, and `<fontcolor>` commands inside the stencil override it
/// from there (optionally scoped with `<save>`/`<restore>`).
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
    let mut state = PaintState {
        fill_color: glyph_color.to_string(),
        stroke_color: glyph_color.to_string(),
        stroke_width: 1.0,
        dashed: false,
        font_color: glyph_color.to_string(),
    };
    let mut stack: Vec<PaintState> = Vec::new();
    for cmd in &stencil.commands {
        emit_cmd(cmd, tr, &mut state, &mut stack, &mut out, &mut path);
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

#[allow(clippy::too_many_arguments)]
fn emit_cmd(
    cmd: &Cmd,
    tr: Transform,
    state: &mut PaintState,
    stack: &mut Vec<PaintState>,
    out: &mut String,
    path: &mut String,
) {
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
        Cmd::Save => stack.push(state.clone()),
        Cmd::Restore => {
            if let Some(prev) = stack.pop() {
                *state = prev;
            }
        }
        Cmd::SetFillColor(c) => state.fill_color.clone_from(c),
        Cmd::SetStrokeColor(c) => state.stroke_color.clone_from(c),
        Cmd::SetFontColor(c) => state.font_color.clone_from(c),
        Cmd::SetStrokeWidth { width, fixed } => {
            let scale = if *fixed { 1.0 } else { tr.sx.min(tr.sy) };
            state.stroke_width = width * scale;
        }
        Cmd::SetDashed(v) => state.dashed = *v,
        Cmd::Fill => emit_paint(path, out, state, PaintMode::Fill),
        Cmd::Stroke => emit_paint(path, out, state, PaintMode::Stroke),
        Cmd::FillStroke => emit_paint(path, out, state, PaintMode::FillStroke),
        Cmd::Text {
            x,
            y,
            text,
            align,
            valign,
        } => emit_text(tr, state, out, *x, *y, text, align, valign),
        Cmd::Ellipse { x, y, w, h } => emit_ellipse(tr, &state.fill_color, out, *x, *y, *w, *h),
        Cmd::Rect { x, y, w, h } => emit_rect(tr, &state.fill_color, out, *x, *y, *w, *h, 0.0),
        Cmd::RoundRect { x, y, w, h, arc } => {
            emit_rect(tr, &state.fill_color, out, *x, *y, *w, *h, *arc);
        }
    }
}

/// Which paint(s) [`emit_paint`] applies to the accumulated path — one per
/// `<fill/>`/`<stroke/>`/`<fillstroke/>` command.
#[derive(Clone, Copy)]
enum PaintMode {
    Fill,
    Stroke,
    FillStroke,
}

/// Paint the accumulated `path` and clear it, using the colours/width/dash
/// currently in `state`. A no-op on an empty path (mirrors the original
/// `<fill/>`-only behaviour when a shape never opened a `<path>`, e.g. after
/// a no-attribute `<rect/>` placeholder some GCP stencils emit).
fn emit_paint(path: &mut String, out: &mut String, state: &PaintState, mode: PaintMode) {
    if path.trim().is_empty() {
        return;
    }
    let d = path.trim();
    let dash = dash_attr(state);
    match mode {
        PaintMode::Fill => {
            let _ = write!(
                out,
                "<path d=\"{d}\" fill=\"{}\" fill-rule=\"evenodd\" stroke=\"none\"/>",
                state.fill_color
            );
        }
        PaintMode::Stroke => {
            let _ = write!(
                out,
                "<path d=\"{d}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\"{dash}/>",
                state.stroke_color, state.stroke_width
            );
        }
        PaintMode::FillStroke => {
            let _ = write!(
                out,
                "<path d=\"{d}\" fill=\"{}\" fill-rule=\"evenodd\" stroke=\"{}\" \
                 stroke-width=\"{}\"{dash}/>",
                state.fill_color, state.stroke_color, state.stroke_width
            );
        }
    }
    path.clear();
}

/// SVG `stroke-dasharray` attribute (empty string when not dashed), scaled
/// off the current stroke width — mirrors upstream's default `"3 3"` dash
/// pattern multiplied by `strokeWidth * scale`.
fn dash_attr(state: &PaintState) -> String {
    if !state.dashed {
        return String::new();
    }
    let unit = 3.0 * state.stroke_width.max(0.1);
    format!(" stroke-dasharray=\"{unit} {unit}\"")
}

/// Emit a `<text>` primitive (the `<text>` stencil command). Literal text
/// only — no placeholder substitution (`%name%`-style attributes upstream
/// supports via `evaluateTextAttribute`).
#[allow(clippy::too_many_arguments)]
fn emit_text(
    tr: Transform,
    state: &PaintState,
    out: &mut String,
    x: f64,
    y: f64,
    text: &str,
    align: &str,
    valign: &str,
) {
    let anchor = match align {
        "center" => "middle",
        "right" => "end",
        _ => "start",
    };
    let baseline = match valign {
        "middle" => "middle",
        "bottom" => "auto",
        _ => "hanging",
    };
    let font_size = (12.0 * tr.sy.min(tr.sx)).max(1.0);
    let _ = write!(
        out,
        "<text x=\"{}\" y=\"{}\" font-family=\"sans-serif\" font-size=\"{font_size}\" \
         fill=\"{}\" text-anchor=\"{anchor}\" dominant-baseline=\"{baseline}\">{}</text>",
        tr.tx(x),
        tr.ty(y),
        state.font_color,
        crate::escape_text(text)
    );
}

/// Emit an `<ellipse>` primitive (the `<ellipse>` stencil command).
#[allow(clippy::many_single_char_names)]
fn emit_ellipse(tr: Transform, fill_color: &str, out: &mut String, x: f64, y: f64, w: f64, h: f64) {
    let cx = tr.tx(x + w / 2.0);
    let cy = tr.ty(y + h / 2.0);
    let rx = (w * tr.sx) / 2.0;
    let ry = (h * tr.sy) / 2.0;
    let _ = write!(
        out,
        "<ellipse cx=\"{cx}\" cy=\"{cy}\" rx=\"{rx}\" ry=\"{ry}\" fill=\"{fill_color}\"/>"
    );
}

/// Emit a `<rect>` primitive (the `<rect>`/`<roundrect>` stencil commands —
/// `corner_arc` is `0.0` for a plain rect).
#[allow(clippy::too_many_arguments, clippy::many_single_char_names)]
fn emit_rect(
    tr: Transform,
    fill_color: &str,
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
        fill_color
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
                Cmd::Fill,
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
                Cmd::Fill,
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
                Cmd::Fill,
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

    #[test]
    fn save_restore_scopes_style_overrides() {
        // Fixture: a shape that paints once with the default glyph colour,
        // overrides the fill colour inside a <save>/<restore> scope, then
        // paints a third time after <restore> — the override must not leak
        // past the matching <restore/>.
        let s = Stencil {
            name: "t".into(),
            w: 10.0,
            h: 10.0,
            commands: vec![
                Cmd::PathBegin,
                Cmd::Move(0.0, 0.0),
                Cmd::Line(1.0, 1.0),
                Cmd::Close,
                Cmd::Fill,
                Cmd::Save,
                Cmd::SetFillColor("#ff0000".to_string()),
                Cmd::PathBegin,
                Cmd::Move(2.0, 2.0),
                Cmd::Line(3.0, 3.0),
                Cmd::Close,
                Cmd::Fill,
                Cmd::Restore,
                Cmd::PathBegin,
                Cmd::Move(4.0, 4.0),
                Cmd::Line(5.0, 5.0),
                Cmd::Close,
                Cmd::Fill,
            ],
        };
        let cell = CellBounds {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        };
        let svg = render_stencil_to_svg(&s, cell, 0.0, "#00ff00", false);
        assert_eq!(
            svg.matches("fill=\"#ff0000\"").count(),
            1,
            "override should paint exactly once inside the save/restore scope; got: {svg}"
        );
        assert_eq!(
            svg.matches("fill=\"#00ff00\"").count(),
            2,
            "glyph colour should paint before the save and after the restore; got: {svg}"
        );
    }

    #[test]
    fn restore_without_matching_save_is_a_no_op() {
        // Mirrors mxAbstractCanvas2D.restore(), which only pops when the
        // stack is non-empty — a stray <restore/> must not panic or clear
        // state.
        let s = Stencil {
            name: "t".into(),
            w: 10.0,
            h: 10.0,
            commands: vec![
                Cmd::Restore,
                Cmd::PathBegin,
                Cmd::Move(0.0, 0.0),
                Cmd::Line(1.0, 1.0),
                Cmd::Close,
                Cmd::Fill,
            ],
        };
        let cell = CellBounds {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        };
        let svg = render_stencil_to_svg(&s, cell, 0.0, "#123456", false);
        assert!(svg.contains("fill=\"#123456\""));
    }

    #[test]
    fn strokecolor_and_fillcolor_override_fillstroke() {
        let s = Stencil {
            name: "t".into(),
            w: 10.0,
            h: 10.0,
            commands: vec![
                Cmd::SetStrokeColor("#0000ff".to_string()),
                Cmd::SetFillColor("#ff00ff".to_string()),
                Cmd::PathBegin,
                Cmd::Move(0.0, 0.0),
                Cmd::Line(1.0, 1.0),
                Cmd::Close,
                Cmd::FillStroke,
            ],
        };
        let cell = CellBounds {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        };
        let svg = render_stencil_to_svg(&s, cell, 0.0, "#000000", false);
        assert!(
            svg.contains("fill=\"#ff00ff\""),
            "expected overridden fill colour; got: {svg}"
        );
        assert!(
            svg.contains("stroke=\"#0000ff\""),
            "expected overridden stroke colour; got: {svg}"
        );
    }

    #[test]
    fn strokecolor_none_is_passed_through_as_svg_keyword() {
        // "none" is a valid SVG paint value (transparent) — drawio's stencils
        // use it directly (e.g. gcp.xml's BigQuery shape), so it must not be
        // translated or dropped.
        let s = Stencil {
            name: "t".into(),
            w: 10.0,
            h: 10.0,
            commands: vec![
                Cmd::SetStrokeColor("none".to_string()),
                Cmd::PathBegin,
                Cmd::Move(0.0, 0.0),
                Cmd::Line(1.0, 1.0),
                Cmd::Close,
                Cmd::Stroke,
            ],
        };
        let cell = CellBounds {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        };
        let svg = render_stencil_to_svg(&s, cell, 0.0, "#000000", false);
        assert!(svg.contains("stroke=\"none\""));
    }

    #[test]
    fn strokewidth_scales_unless_fixed() {
        let scaled = Stencil {
            name: "t".into(),
            w: 10.0,
            h: 10.0,
            commands: vec![
                Cmd::SetStrokeWidth {
                    width: 2.0,
                    fixed: false,
                },
                Cmd::PathBegin,
                Cmd::Move(0.0, 0.0),
                Cmd::Line(1.0, 1.0),
                Cmd::Close,
                Cmd::Stroke,
            ],
        };
        let cell = CellBounds {
            x: 0.0,
            y: 0.0,
            w: 20.0,
            h: 20.0,
        };
        // stencil is 10x10 mapped onto a 20x20 tile with no padding: scale
        // is 2.0 on both axes, so an unfixed width of 2.0 becomes 4.0.
        let svg = render_stencil_to_svg(&scaled, cell, 0.0, "#000000", false);
        assert!(
            svg.contains("stroke-width=\"4\""),
            "expected width scaled by the tile's 2x scale; got: {svg}"
        );

        let fixed = Stencil {
            name: "t".into(),
            w: 10.0,
            h: 10.0,
            commands: vec![
                Cmd::SetStrokeWidth {
                    width: 2.0,
                    fixed: true,
                },
                Cmd::PathBegin,
                Cmd::Move(0.0, 0.0),
                Cmd::Line(1.0, 1.0),
                Cmd::Close,
                Cmd::Stroke,
            ],
        };
        let svg = render_stencil_to_svg(&fixed, cell, 0.0, "#000000", false);
        assert!(
            svg.contains("stroke-width=\"2\""),
            "fixed width must ignore the tile scale; got: {svg}"
        );
    }

    #[test]
    fn dashed_emits_scaled_stroke_dasharray() {
        let s = Stencil {
            name: "t".into(),
            w: 10.0,
            h: 10.0,
            commands: vec![
                Cmd::SetDashed(true),
                Cmd::PathBegin,
                Cmd::Move(0.0, 0.0),
                Cmd::Line(1.0, 1.0),
                Cmd::Close,
                Cmd::Stroke,
            ],
        };
        let cell = CellBounds {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        };
        let svg = render_stencil_to_svg(&s, cell, 0.0, "#000000", false);
        assert!(
            svg.contains("stroke-dasharray="),
            "expected a dash pattern; got: {svg}"
        );

        let not_dashed = Stencil {
            commands: vec![
                Cmd::PathBegin,
                Cmd::Move(0.0, 0.0),
                Cmd::Line(1.0, 1.0),
                Cmd::Stroke,
            ],
            ..s
        };
        let svg = render_stencil_to_svg(&not_dashed, cell, 0.0, "#000000", false);
        assert!(!svg.contains("stroke-dasharray="));
    }

    #[test]
    fn fontcolor_colors_text_command() {
        let s = Stencil {
            name: "t".into(),
            w: 10.0,
            h: 10.0,
            commands: vec![
                Cmd::SetFontColor("#abcdef".to_string()),
                Cmd::Text {
                    x: 1.0,
                    y: 1.0,
                    text: "hi".to_string(),
                    align: "center".to_string(),
                    valign: "middle".to_string(),
                },
            ],
        };
        let cell = CellBounds {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        };
        let svg = render_stencil_to_svg(&s, cell, 0.0, "#000000", false);
        assert!(svg.contains("<text"));
        assert!(svg.contains("fill=\"#abcdef\""));
        assert!(svg.contains(">hi</text>"));
        assert!(svg.contains("text-anchor=\"middle\""));
    }

    #[test]
    fn text_escapes_content() {
        let s = Stencil {
            name: "t".into(),
            w: 10.0,
            h: 10.0,
            commands: vec![Cmd::Text {
                x: 0.0,
                y: 0.0,
                text: "<a & b>".to_string(),
                align: "left".to_string(),
                valign: "top".to_string(),
            }],
        };
        let cell = CellBounds {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        };
        let svg = render_stencil_to_svg(&s, cell, 0.0, "#000000", false);
        assert!(svg.contains("&lt;a &amp; b&gt;"));
    }

    #[test]
    fn image_command_is_rejected_as_unsupported() {
        // <image> needs raster embedding this crate does not have — it must
        // surface RenderError::UnsupportedStencilCmd rather than silently
        // dropping the glyph detail (issue #7).
        let xml = r#"<shapes name="mxgraph.test">
<shape name="has-image" w="10" h="10">
    <foreground>
        <image x="0" y="0" w="10" h="10" src="data:image/png;base64,AA=="/>
    </foreground>
</shape>
</shapes>"#;
        let err = StencilLibrary::from_xml(xml, "mxgraph.test").unwrap_err();
        assert!(matches!(err, RenderError::UnsupportedStencilCmd(cmd) if cmd == "image"));
    }

    #[test]
    fn image_outside_foreground_is_ignored() {
        // <background> content is not parsed at all today (only
        // <foreground> is walked), so an <image> there must not error either
        // — it never reaches the handler.
        let xml = r#"<shapes name="mxgraph.test">
<shape name="bg-image" w="10" h="10">
    <background>
        <image x="0" y="0" w="10" h="10" src="foo.png"/>
    </background>
    <foreground>
        <rect x="0" y="0" w="10" h="10"/>
        <fill/>
    </foreground>
</shape>
</shapes>"#;
        let lib = StencilLibrary::from_xml(xml, "mxgraph.test").expect("parses fine");
        assert!(lib.lookup("bg-image").is_some());
    }

    #[test]
    fn renders_real_aws_work_package_stencil_with_style_overrides() {
        // Real-world fixture: aws4.xml's "work package" shape is the only
        // stencil in that library exercising strokecolor/strokewidth/dashed
        // together with a bare <stroke/> and <fillcolor> before
        // <fillstroke/> (see issue #7).
        let xml = std::fs::read_to_string("../../stencils/aws4.xml").expect("stencil file");
        let lib = StencilLibrary::from_xml(&xml, "mxgraph.aws4").unwrap();
        let s = lib.lookup("work_package").expect("work package present");
        let cell = CellBounds {
            x: 0.0,
            y: 0.0,
            w: 78.0,
            h: 78.0,
        };
        let svg = render_stencil_to_svg(s, cell, 0.0, "#232F3E", false);
        assert!(
            svg.contains("stroke=\"#00ff00\""),
            "expected the stencil's own strokecolor override; got: {svg}"
        );
        assert!(
            svg.contains("fill=\"#00ff00\""),
            "expected the stencil's own fillcolor override on the final \
             fillstroke; got: {svg}"
        );
        assert!(svg.contains("stroke-dasharray="), "expected dashed stroke");
    }

    #[test]
    fn renders_real_gcp_bigquery_stencil_with_save_restore() {
        // Real-world fixture: gcp.xml's BigQuery shape wraps a shadow layer
        // in <save>/<fillcolor>/.../<restore>, then resumes drawing with the
        // glyph colour restored (see issue #7).
        let xml = std::fs::read_to_string("../../stencils/gcp.xml").expect("stencil file");
        let lib = StencilLibrary::from_xml(&xml, "mxgraph.gcp").unwrap();
        let s = lib.lookup("big_data.bigquery").expect("bigquery present");
        let cell = CellBounds {
            x: 0.0,
            y: 0.0,
            w: 129.0,
            h: 113.0,
        };
        let svg = render_stencil_to_svg(s, cell, 0.0, "#4285F4", false);
        // The shadow layer inside save/restore overrides fill to black.
        assert!(
            svg.contains("fill=\"#000000\""),
            "expected the save-scoped fillcolor override; got: {svg}"
        );
        // The white highlight detail drawn after restore.
        assert!(
            svg.contains("fill=\"#fff\""),
            "expected the post-restore fillcolor override; got: {svg}"
        );
        // The glyph's own default-coloured silhouette (painted before the
        // first <save/>, using no override).
        assert!(svg.contains("fill=\"#4285F4\""));
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
