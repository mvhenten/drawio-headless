//! Programmatic authoring of `.drawio` XML diagrams.
//!
//! The library is deliberately small: build a [`Diagram`], push [`Node`]s and
//! edges via [`Diagram::connect`], then call [`Diagram::to_xml`].
//!
//! The emitted XML uses `compressed="false"` so the payload is plain XML
//! that downstream tools (e.g. the `drawio-render` crate) can read directly.

pub mod aws;

use std::fmt::Write as _;

/// Default AWS resource-icon tile dimensions (in drawio user units).
pub const DEFAULT_AWS_TILE: f64 = 78.0;

/// A single vertex (a shape) in the diagram.
#[derive(Debug, Clone)]
pub struct Node {
    pub id: String,
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub style: String,
}

impl Node {
    /// Low-level escape hatch for shapes that are not in any catalog.
    pub fn raw(
        id: impl Into<String>,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        label: impl Into<String>,
        style: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            x,
            y,
            w,
            h,
            style: style.into(),
        }
    }

    /// Builder helper: place the node's top-left at `(x, y)`.
    #[must_use]
    pub fn at(mut self, x: f64, y: f64) -> Self {
        self.x = x;
        self.y = y;
        self
    }

    /// Builder helper: set the node's width and height.
    #[must_use]
    pub fn size(mut self, w: f64, h: f64) -> Self {
        self.w = w;
        self.h = h;
        self
    }

    /// Builder helper: change the visible label.
    #[must_use]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }
}

/// A reference to a node that has been added to a diagram.
///
/// Returned from [`Diagram::add_node`] and consumed by [`Diagram::connect`].
#[derive(Debug, Clone)]
pub struct NodeRef {
    pub id: String,
}

#[derive(Debug, Clone)]
struct Edge {
    id: String,
    source: String,
    target: String,
}

/// A drawio diagram document. Currently holds a single page.
#[derive(Debug, Clone)]
pub struct Diagram {
    name: String,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    next_edge: usize,
}

impl Diagram {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            nodes: Vec::new(),
            edges: Vec::new(),
            next_edge: 1,
        }
    }

    /// Add a node to the diagram. Returns a [`NodeRef`] that can be passed to
    /// [`Diagram::connect`].
    pub fn add_node(&mut self, node: Node) -> NodeRef {
        let r = NodeRef {
            id: node.id.clone(),
        };
        self.nodes.push(node);
        r
    }

    /// Connect two nodes with an edge.
    pub fn connect(&mut self, source: &NodeRef, target: &NodeRef) {
        let id = format!("e{}", self.next_edge);
        self.next_edge += 1;
        self.edges.push(Edge {
            id,
            source: source.id.clone(),
            target: target.id.clone(),
        });
    }

    /// Serialize the diagram to a `.drawio` XML string with
    /// `compressed="false"` so the body is plain (non-deflated) XML.
    pub fn to_xml(&self) -> String {
        let mut out = String::with_capacity(1024);
        out.push_str("<mxfile host=\"drawio-headless\" compressed=\"false\" version=\"0.1.0\">\n");
        let _ = writeln!(out, "  <diagram id=\"p1\" name=\"{}\">", escape(&self.name));
        out.push_str(
            "    <mxGraphModel dx=\"800\" dy=\"600\" grid=\"1\" gridSize=\"10\" \
             pageWidth=\"850\" pageHeight=\"1100\" math=\"0\" shadow=\"0\">\n",
        );
        out.push_str("      <root>\n");
        out.push_str("        <mxCell id=\"0\"/>\n");
        out.push_str("        <mxCell id=\"1\" parent=\"0\"/>\n");

        for n in &self.nodes {
            let _ = writeln!(
                out,
                "        <mxCell id=\"{id}\" value=\"{label}\" vertex=\"1\" parent=\"1\" style=\"{style}\">",
                id = escape(&n.id),
                label = escape(&n.label),
                style = escape(&n.style),
            );
            let _ = writeln!(
                out,
                "          <mxGeometry x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" as=\"geometry\"/>",
                x = trim_num(n.x),
                y = trim_num(n.y),
                w = trim_num(n.w),
                h = trim_num(n.h),
            );
            out.push_str("        </mxCell>\n");
        }

        for e in &self.edges {
            let _ = writeln!(
                out,
                "        <mxCell id=\"{id}\" edge=\"1\" parent=\"1\" source=\"{s}\" target=\"{t}\" \
                 style=\"edgeStyle=orthogonalEdgeStyle;html=0;endArrow=open;startArrow=none;rounded=0;\">",
                id = escape(&e.id),
                s = escape(&e.source),
                t = escape(&e.target),
            );
            out.push_str("          <mxGeometry relative=\"1\" as=\"geometry\"/>\n");
            out.push_str("        </mxCell>\n");
        }

        out.push_str("      </root>\n");
        out.push_str("    </mxGraphModel>\n");
        out.push_str("  </diagram>\n");
        out.push_str("</mxfile>\n");
        out
    }
}

/// Minimal XML attribute escaping for the five core entities.
fn escape(s: &str) -> String {
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

/// Render a number without unnecessary trailing zeros (so `78.0` -> `78`).
fn trim_num(v: f64) -> String {
    if v.fract().abs() < f64::EPSILON {
        // For round values, format with no decimals.
        format!("{v:.0}")
    } else {
        format!("{v}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_specials_in_label_and_id() {
        let mut d = Diagram::new("MyArch");
        let r = d.add_node(Node::raw(
            "n<1>",
            10.0,
            10.0,
            78.0,
            78.0,
            "API & \"Auth\"",
            "shape=mxgraph.aws4.resourceIcon;",
        ));
        let _ = r;
        let xml = d.to_xml();
        assert!(xml.contains("API &amp; &quot;Auth&quot;"), "{xml}");
        assert!(xml.contains("id=\"n&lt;1&gt;\""), "{xml}");
    }

    #[test]
    fn emits_two_nodes_and_one_edge() {
        let mut d = Diagram::new("t");
        let a = d.add_node(aws::api_gateway("api", "API Gateway").at(80.0, 80.0));
        let l = d.add_node(aws::lambda("lam", "Lambda").at(320.0, 80.0));
        d.connect(&a, &l);
        let xml = d.to_xml();
        assert!(xml.contains("id=\"api\""));
        assert!(xml.contains("id=\"lam\""));
        assert!(xml.contains("source=\"api\" target=\"lam\""));
        assert!(xml.contains("compressed=\"false\""));
    }
}
