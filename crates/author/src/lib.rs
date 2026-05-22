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
    /// Source-side connection-point override (`exitX`, `exitY`).
    exit: Option<(f32, f32)>,
    /// Target-side connection-point override (`entryX`, `entryY`).
    entry: Option<(f32, f32)>,
}

/// Chainable builder returned from [`Diagram::connect`]. Mutates the
/// just-inserted edge so callers can pin the source/target attachment
/// points without an extra round-trip through the diagram.
///
/// Existing call sites that ignore the return value (`d.connect(&a, &b);`)
/// continue to work — the builder simply does nothing further when dropped.
///
/// # Example
///
/// ```
/// # use drawio_author::{Diagram, aws};
/// let mut d = Diagram::new("demo");
/// let a = d.add_node(aws::api_gateway("api", "API").at(0.0, 0.0));
/// let b = d.add_node(aws::lambda("lam", "Lambda").at(300.0, 0.0));
/// d.connect(&a, &b)
///     .exit(1.0, 0.5)   // edge leaves source's right-mid
///     .entry(0.0, 0.5); // edge enters target's left-mid
/// ```
pub struct EdgeBuilder<'a> {
    edge: &'a mut Edge,
}

impl EdgeBuilder<'_> {
    /// Pin the source-side attachment point. `(x, y)` are normalised in
    /// `[0.0, 1.0]` on the source cell's bounding box; values outside the
    /// range are clamped (matching drawio).
    ///
    /// Returning `Self` is for chaining; the mutation lands on the
    /// underlying edge immediately, so discarding the return value
    /// (e.g. `.exit(1.0, 0.5);`) is fine.
    #[allow(clippy::return_self_not_must_use)]
    pub fn exit(self, x: f32, y: f32) -> Self {
        self.edge.exit = Some((clamp_unit(x), clamp_unit(y)));
        self
    }

    /// Pin the target-side attachment point. `(x, y)` are normalised in
    /// `[0.0, 1.0]` on the target cell's bounding box; values outside the
    /// range are clamped (matching drawio).
    ///
    /// Returning `Self` is for chaining; the mutation lands on the
    /// underlying edge immediately, so discarding the return value is
    /// fine.
    #[allow(clippy::return_self_not_must_use)]
    pub fn entry(self, x: f32, y: f32) -> Self {
        self.edge.entry = Some((clamp_unit(x), clamp_unit(y)));
        self
    }
}

fn clamp_unit(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Variants of AWS group / boundary containers.
///
/// Each variant maps to a specific `grIcon` value and an AWS-canonical
/// stroke/font colour combination, matching what the upstream drawio app
/// emits from its AWS4 sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupKind {
    /// Dashed pink rectangle representing an AWS Account boundary.
    AwsAccount,
    /// Solid purple rectangle representing a VPC boundary.
    AwsVpc,
    /// Solid dark rectangle representing the AWS Cloud boundary.
    AwsCloud,
    /// Generic dashed grey container with no `grIcon`.
    Generic,
}

impl GroupKind {
    fn style(self) -> &'static str {
        match self {
            Self::AwsAccount => {
                "points=[[0,0],[0.25,0],[0.5,0],[0.75,0],[1,0],[1,0.25],\
                [1,0.5],[1,0.75],[1,1],[0.75,1],[0.5,1],[0.25,1],[0,1],[0,0.75],[0,0.5],[0,0.25]];\
                outlineConnect=0;gradientColor=none;html=0;whiteSpace=wrap;fontSize=12;\
                fontStyle=0;container=1;pointerEvents=0;collapsible=0;recursiveResize=0;\
                shape=mxgraph.aws4.group;grIcon=mxgraph.aws4.group_account;\
                strokeColor=#CD2264;fillColor=none;verticalAlign=top;align=left;spacingLeft=30;\
                fontColor=#CD2264;dashed=1;"
            }
            Self::AwsVpc => {
                "points=[[0,0],[0.25,0],[0.5,0],[0.75,0],[1,0],[1,0.25],\
                [1,0.5],[1,0.75],[1,1],[0.75,1],[0.5,1],[0.25,1],[0,1],[0,0.75],[0,0.5],[0,0.25]];\
                outlineConnect=0;gradientColor=none;html=0;whiteSpace=wrap;fontSize=12;\
                fontStyle=0;container=1;pointerEvents=0;collapsible=0;recursiveResize=0;\
                shape=mxgraph.aws4.group;grIcon=mxgraph.aws4.group_vpc;\
                strokeColor=#8C4FFF;fillColor=none;verticalAlign=top;align=left;spacingLeft=30;\
                fontColor=#8C4FFF;dashed=0;"
            }
            Self::AwsCloud => {
                "points=[[0,0],[0.25,0],[0.5,0],[0.75,0],[1,0],[1,0.25],\
                [1,0.5],[1,0.75],[1,1],[0.75,1],[0.5,1],[0.25,1],[0,1],[0,0.75],[0,0.5],[0,0.25]];\
                outlineConnect=0;gradientColor=none;html=0;whiteSpace=wrap;fontSize=12;\
                fontStyle=0;container=1;pointerEvents=0;collapsible=0;recursiveResize=0;\
                shape=mxgraph.aws4.group;grIcon=mxgraph.aws4.group_aws_cloud_alt;\
                strokeColor=#232F3E;fillColor=none;verticalAlign=top;align=left;spacingLeft=30;\
                fontColor=#232F3E;dashed=0;"
            }
            Self::Generic => {
                "outlineConnect=0;html=0;whiteSpace=wrap;fontSize=12;\
                fontStyle=0;container=1;pointerEvents=0;collapsible=0;recursiveResize=0;\
                shape=mxgraph.aws4.group;\
                strokeColor=#7D8998;fillColor=none;verticalAlign=top;align=left;spacingLeft=10;\
                fontColor=#5A6C86;dashed=1;"
            }
        }
    }
}

/// Options for creating a [`Group`] container via [`Diagram::add_group`].
#[derive(Debug, Clone)]
pub struct GroupOpts {
    pub id: String,
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub kind: GroupKind,
}

impl GroupOpts {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        kind: GroupKind,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            x,
            y,
            w,
            h,
            kind,
        }
    }
}

#[derive(Debug, Clone)]
struct Group {
    id: String,
    label: String,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    kind: GroupKind,
}

/// A reference to a group container.
///
/// Returned from [`Diagram::add_group`]; currently informational only —
/// children are inferred by bounding-box containment at render time.
#[derive(Debug, Clone)]
pub struct GroupRef {
    pub id: String,
}

/// A drawio diagram document. Currently holds a single page.
#[derive(Debug, Clone)]
pub struct Diagram {
    name: String,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    groups: Vec<Group>,
    next_edge: usize,
}

impl Diagram {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            nodes: Vec::new(),
            edges: Vec::new(),
            groups: Vec::new(),
            next_edge: 1,
        }
    }

    /// Add a group / boundary container. Groups are rendered behind nodes;
    /// children are inferred by geometric containment at render time, so it
    /// is sufficient to place the group's bounding box around the nodes that
    /// belong to it.
    pub fn add_group(&mut self, opts: GroupOpts) -> GroupRef {
        let r = GroupRef {
            id: opts.id.clone(),
        };
        self.groups.push(Group {
            id: opts.id,
            label: opts.label,
            x: opts.x,
            y: opts.y,
            w: opts.w,
            h: opts.h,
            kind: opts.kind,
        });
        r
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

    /// Connect two nodes with an edge. Returns an [`EdgeBuilder`] for
    /// optional per-edge tweaks (e.g. pinning the source/target attachment
    /// point via `.exit(x, y)` / `.entry(x, y)`). Discarding the return
    /// value (the legacy `d.connect(&a, &b);` form) is fully supported.
    pub fn connect(&mut self, source: &NodeRef, target: &NodeRef) -> EdgeBuilder<'_> {
        let id = format!("e{}", self.next_edge);
        self.next_edge += 1;
        self.edges.push(Edge {
            id,
            source: source.id.clone(),
            target: target.id.clone(),
            exit: None,
            entry: None,
        });
        EdgeBuilder {
            edge: self.edges.last_mut().expect("just pushed"),
        }
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

        for g in &self.groups {
            let _ = writeln!(
                out,
                "        <mxCell id=\"{id}\" value=\"{label}\" vertex=\"1\" parent=\"1\" style=\"{style}\">",
                id = escape(&g.id),
                label = escape(&g.label),
                style = escape(g.kind.style()),
            );
            let _ = writeln!(
                out,
                "          <mxGeometry x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" as=\"geometry\"/>",
                x = trim_num(g.x),
                y = trim_num(g.y),
                w = trim_num(g.w),
                h = trim_num(g.h),
            );
            out.push_str("        </mxCell>\n");
        }

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
            let mut style = String::from(
                "edgeStyle=orthogonalEdgeStyle;html=0;endArrow=open;startArrow=none;rounded=0;",
            );
            if let Some((x, y)) = e.exit {
                let _ = write!(style, "exitX={x};exitY={y};exitDx=0;exitDy=0;");
            }
            if let Some((x, y)) = e.entry {
                let _ = write!(style, "entryX={x};entryY={y};entryDx=0;entryDy=0;");
            }
            let _ = writeln!(
                out,
                "        <mxCell id=\"{id}\" edge=\"1\" parent=\"1\" source=\"{s}\" target=\"{t}\" \
                 style=\"{style}\">",
                id = escape(&e.id),
                s = escape(&e.source),
                t = escape(&e.target),
                style = escape(&style),
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

    #[test]
    fn connect_returns_builder_emitting_exit_entry_attrs() {
        let mut d = Diagram::new("t");
        let a = d.add_node(aws::api_gateway("api", "API").at(0.0, 0.0));
        let b = d.add_node(aws::lambda("lam", "Lambda").at(300.0, 0.0));
        d.connect(&a, &b).exit(1.0, 0.5).entry(0.0, 0.5);
        let xml = d.to_xml();
        assert!(xml.contains("exitX=1;exitY=0.5"), "{xml}");
        assert!(xml.contains("entryX=0;entryY=0.5"), "{xml}");
    }

    #[test]
    fn connect_without_builder_calls_omits_entry_exit_keys() {
        let mut d = Diagram::new("t");
        let a = d.add_node(aws::api_gateway("api", "API").at(0.0, 0.0));
        let b = d.add_node(aws::lambda("lam", "Lambda").at(300.0, 0.0));
        // Legacy form: no chained builder calls. Must compile and must
        // NOT emit any exit/entry attributes.
        d.connect(&a, &b);
        let xml = d.to_xml();
        assert!(!xml.contains("exitX="), "{xml}");
        assert!(!xml.contains("entryX="), "{xml}");
    }

    #[test]
    fn edge_builder_clamps_out_of_range_values() {
        let mut d = Diagram::new("t");
        let a = d.add_node(aws::api_gateway("api", "API").at(0.0, 0.0));
        let b = d.add_node(aws::lambda("lam", "Lambda").at(300.0, 0.0));
        d.connect(&a, &b).exit(1.7, -0.4).entry(2.0, 0.0);
        let xml = d.to_xml();
        // 1.7 -> 1, -0.4 -> 0, 2.0 -> 1.
        assert!(xml.contains("exitX=1;exitY=0;"), "{xml}");
        assert!(xml.contains("entryX=1;entryY=0;"), "{xml}");
    }

    #[test]
    fn emits_group_before_nodes() {
        let mut d = Diagram::new("g");
        d.add_group(GroupOpts::new(
            "acct-a",
            "Account A",
            40.0,
            40.0,
            320.0,
            200.0,
            GroupKind::AwsAccount,
        ));
        d.add_node(aws::lambda("lam", "Lambda").at(120.0, 120.0));
        let xml = d.to_xml();
        let group_pos = xml.find("id=\"acct-a\"").expect("group id");
        let node_pos = xml.find("id=\"lam\"").expect("node id");
        assert!(group_pos < node_pos, "group should precede node in XML");
        assert!(xml.contains("shape=mxgraph.aws4.group"));
        assert!(xml.contains("grIcon=mxgraph.aws4.group_account"));
        assert!(xml.contains("value=\"Account A\""));
    }
}
