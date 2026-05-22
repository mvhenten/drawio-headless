//! Parse `mxfile/diagram/mxGraphModel/root/mxCell` into a small Rust model.
//!
//! Compressed `<diagram>` payloads (the drawio editor's default on save) are
//! inflated transparently via [`crate::inflate`] before the XML walk.
//! Cells with `id="0"` and `id="1"` (the implicit root and default layer)
//! are skipped.

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::RenderError;
use crate::inflate::{inflate_diagram_body, is_compressed_body};

#[derive(Debug, Clone)]
pub struct Vertex {
    pub id: String,
    pub label: String,
    pub style: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone)]
pub struct Edge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub style: String,
}

#[derive(Debug, Default, Clone)]
pub struct Model {
    pub vertices: Vec<Vertex>,
    pub edges: Vec<Edge>,
}

/// Decode an attribute value (handling XML entities) into an owned string.
fn attr_str(a: &quick_xml::events::attributes::Attribute) -> Result<String, RenderError> {
    // quick-xml 0.36 requires a Decoder; the default UTF-8 decoder is fine
    // for everything drawio ships.
    let s = a
        .decode_and_unescape_value(quick_xml::Reader::from_str("").decoder())
        .map_err(|e| RenderError::Xml(e.to_string()))?;
    Ok(s.into_owned())
}

#[derive(Default)]
struct Attrs {
    id: String,
    value: String,
    style: String,
    vertex: bool,
    edge: bool,
    source: String,
    target: String,
}

fn read_cell_attrs(elem: &BytesStart<'_>) -> Result<Attrs, RenderError> {
    let mut out = Attrs::default();
    for attr in elem.attributes() {
        let attr = attr.map_err(|err| RenderError::Xml(err.to_string()))?;
        let raw = attr_str(&attr)?;
        match attr.key.as_ref() {
            b"id" => out.id = raw,
            b"value" => out.value = raw,
            b"style" => out.style = raw,
            b"vertex" => out.vertex = raw == "1",
            b"edge" => out.edge = raw == "1",
            b"source" => out.source = raw,
            b"target" => out.target = raw,
            _ => {}
        }
    }
    Ok(out)
}

#[derive(Default, Clone, Copy)]
struct Geometry {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

fn read_geometry(elem: &BytesStart<'_>) -> Result<Geometry, RenderError> {
    let mut geom = Geometry::default();
    for attr in elem.attributes() {
        let attr = attr.map_err(|err| RenderError::Xml(err.to_string()))?;
        let raw = attr_str(&attr)?;
        match attr.key.as_ref() {
            b"x" => geom.x = raw.parse().unwrap_or(0.0),
            b"y" => geom.y = raw.parse().unwrap_or(0.0),
            b"width" => geom.w = raw.parse().unwrap_or(0.0),
            b"height" => geom.h = raw.parse().unwrap_or(0.0),
            _ => {}
        }
    }
    Ok(geom)
}

/// Parse a drawio XML string into a [`Model`].
///
/// Compressed `<diagram>` bodies are inflated in place (base64 ->
/// raw DEFLATE -> URL decode) before the structural walk.
pub fn parse(xml: &str) -> Result<Model, RenderError> {
    // If any <diagram> body is compressed, expand it back into an inline
    // <mxGraphModel> tree first, then parse the resulting XML uniformly.
    let prepared = expand_compressed_diagrams(xml)?;

    let mut reader = Reader::from_str(&prepared);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut model = Model::default();

    // The currently open vertex cell, waiting for its <mxGeometry> child.
    let mut pending: Option<Vertex> = None;

    loop {
        let evt = reader
            .read_event_into(&mut buf)
            .map_err(|e| RenderError::Xml(format!("at {}: {e}", reader.buffer_position())))?;
        match evt {
            Event::Eof => break,
            Event::Start(elem) => {
                let name = elem.name();
                if name.as_ref() == b"mxCell" {
                    let attrs = read_cell_attrs(&elem)?;
                    handle_cell_open(&mut model, &mut pending, attrs);
                } else if name.as_ref() == b"mxGeometry"
                    && let Some(vertex) = pending.as_mut()
                {
                    let geom = read_geometry(&elem)?;
                    apply_geometry(vertex, geom);
                }
            }
            Event::Empty(elem) => {
                let name = elem.name();
                if name.as_ref() == b"mxCell" {
                    // self-closing mxCell: commit immediately (e.g. id="0"/"1"
                    // or edges with no inline geometry text).
                    let attrs = read_cell_attrs(&elem)?;
                    let mut tmp_pending: Option<Vertex> = None;
                    handle_cell_open(&mut model, &mut tmp_pending, attrs);
                    if let Some(vertex) = tmp_pending.take() {
                        model.vertices.push(vertex);
                    }
                } else if name.as_ref() == b"mxGeometry"
                    && let Some(vertex) = pending.as_mut()
                {
                    let geom = read_geometry(&elem)?;
                    apply_geometry(vertex, geom);
                }
            }
            Event::End(elem) => {
                if elem.name().as_ref() == b"mxCell"
                    && let Some(vertex) = pending.take()
                {
                    model.vertices.push(vertex);
                }
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(model)
}

fn handle_cell_open(model: &mut Model, pending: &mut Option<Vertex>, attrs: Attrs) {
    if attrs.id == "0" || attrs.id == "1" {
        return;
    }
    if attrs.edge {
        if !attrs.source.is_empty() && !attrs.target.is_empty() {
            model.edges.push(Edge {
                id: attrs.id,
                source: attrs.source,
                target: attrs.target,
                style: attrs.style,
            });
        }
        return;
    }
    if attrs.vertex {
        *pending = Some(Vertex {
            id: attrs.id,
            label: attrs.value,
            style: attrs.style,
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        });
    }
}

fn apply_geometry(vertex: &mut Vertex, geom: Geometry) {
    vertex.x = geom.x;
    vertex.y = geom.y;
    vertex.w = geom.w;
    vertex.h = geom.h;
}

/// Walk `xml` and, for every `<diagram>...</diagram>` whose text body is
/// compressed (base64-encoded raw DEFLATE of URL-encoded XML), substitute
/// the inflated `<mxGraphModel>` payload in place. Returns the original
/// string unchanged if every `<diagram>` already contains plain XML.
fn expand_compressed_diagrams(xml: &str) -> Result<String, RenderError> {
    let mut out = String::with_capacity(xml.len());
    let mut cursor = 0usize;
    let bytes = xml.as_bytes();

    while let Some(open_rel) = find_subslice(&bytes[cursor..], b"<diagram") {
        let open = cursor + open_rel;
        // Find end of opening tag.
        let Some(open_end_rel) = find_subslice(&bytes[open..], b">") else {
            break;
        };
        let body_start = open + open_end_rel + 1;
        // Find closing </diagram>.
        let Some(close_rel) = find_subslice(&bytes[body_start..], b"</diagram>") else {
            break;
        };
        let body_end = body_start + close_rel;

        // Emit everything up to the body verbatim.
        out.push_str(&xml[cursor..body_start]);

        let body = &xml[body_start..body_end];
        if is_compressed_body(body) {
            let inflated = inflate_diagram_body(body)?;
            out.push_str(&inflated);
        } else {
            out.push_str(body);
        }

        cursor = body_end;
    }
    out.push_str(&xml[cursor..]);
    Ok(out)
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_diagram() {
        let xml = r#"
<mxfile host="x" compressed="false">
  <diagram id="p" name="P">
    <mxGraphModel>
      <root>
        <mxCell id="0"/>
        <mxCell id="1" parent="0"/>
        <mxCell id="a" value="A" vertex="1" parent="1" style="shape=mxgraph.aws4.resourceIcon;fillColor=#ED7100;">
          <mxGeometry x="10" y="20" width="78" height="78" as="geometry"/>
        </mxCell>
        <mxCell id="b" value="B" vertex="1" parent="1" style="fillColor=#E7157B;">
          <mxGeometry x="200" y="20" width="78" height="78" as="geometry"/>
        </mxCell>
        <mxCell id="e1" edge="1" parent="1" source="a" target="b" style="">
          <mxGeometry relative="1" as="geometry"/>
        </mxCell>
      </root>
    </mxGraphModel>
  </diagram>
</mxfile>
"#;
        let m = parse(xml).unwrap();
        assert_eq!(m.vertices.len(), 2);
        assert_eq!(m.edges.len(), 1);
        assert_eq!(m.vertices[0].id, "a");
        assert!((m.vertices[0].x - 10.0).abs() < 1e-9);
    }

    #[test]
    fn passthrough_when_uncompressed() {
        // No inflation should happen; the function is a no-op on the input.
        let xml = r"<mxfile><diagram><mxGraphModel><root/></mxGraphModel></diagram></mxfile>";
        let prepared = expand_compressed_diagrams(xml).unwrap();
        assert_eq!(prepared, xml);
    }
}
