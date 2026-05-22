//! JSON-to-`drawio-author` glue for the `drawio-headless author` subcommand.
//!
//! Reads a small declarative JSON schema (see `docs/authoring-schema.md`)
//! and emits a `.drawio` XML string by driving the `drawio-author` crate.
//!
//! The author library deliberately has no `serde` dependency; this module is
//! the only place that knows about JSON.

use std::collections::HashSet;

use drawio_author::{Diagram, GroupKind, GroupOpts, Node, aws};
use serde::Deserialize;

/// Top-level schema: a named diagram with groups, nodes and edges.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    #[serde(default = "default_name")]
    pub name: String,
    #[serde(default)]
    pub groups: Vec<GroupSpec>,
    #[serde(default)]
    pub nodes: Vec<NodeSpec>,
    #[serde(default)]
    pub edges: Vec<EdgeSpec>,
}

fn default_name() -> String {
    "Diagram".to_string()
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeSpec {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub width: Option<f64>,
    pub height: Option<f64>,
    #[serde(default)]
    pub style: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupSpec {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EdgeSpec {
    pub source: String,
    pub target: String,
    pub exit_x: Option<f32>,
    pub exit_y: Option<f32>,
    pub entry_x: Option<f32>,
    pub entry_y: Option<f32>,
}

/// Errors surfaced from JSON parsing and schema validation.
#[derive(Debug)]
pub enum AuthorError {
    /// `serde_json` failed to parse the input.
    Json(serde_json::Error),
    /// Unknown `kind` for a node. Carries the offending kind and a list of
    /// known kinds (already filtered to close matches if any were found).
    UnknownNodeKind {
        kind: String,
        suggestions: Vec<&'static str>,
    },
    /// Unknown `kind` for a group.
    UnknownGroupKind {
        kind: String,
        suggestions: Vec<&'static str>,
    },
    /// A `raw` node was declared without a `style` string.
    RawNodeMissingStyle { id: String },
    /// An edge points to or from an id that no node declared.
    EdgeUnknownEndpoint {
        source: String,
        target: String,
        missing: String,
    },
    /// Two nodes / groups share the same id.
    DuplicateId(String),
}

impl std::fmt::Display for AuthorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(e) => write!(f, "invalid JSON: {e}"),
            Self::UnknownNodeKind { kind, suggestions } => {
                write!(f, "unknown node kind {kind:?}")?;
                if suggestions.is_empty() {
                    write!(f, ". Known kinds: {}", join_quoted(&NODE_KINDS))?;
                } else {
                    write!(f, " — did you mean {}?", join_quoted(suggestions))?;
                }
                Ok(())
            }
            Self::UnknownGroupKind { kind, suggestions } => {
                write!(f, "unknown group kind {kind:?}")?;
                if suggestions.is_empty() {
                    write!(f, ". Known kinds: {}", join_quoted(&GROUP_KINDS))?;
                } else {
                    write!(f, " — did you mean {}?", join_quoted(suggestions))?;
                }
                Ok(())
            }
            Self::RawNodeMissingStyle { id } => {
                write!(f, "node {id:?} has kind=\"raw\" but no \"style\" field")
            }
            Self::EdgeUnknownEndpoint {
                source,
                target,
                missing,
            } => write!(
                f,
                "edge {source:?} -> {target:?} references unknown id {missing:?}"
            ),
            Self::DuplicateId(id) => write!(f, "duplicate id {id:?}"),
        }
    }
}

impl std::error::Error for AuthorError {}

impl From<serde_json::Error> for AuthorError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

fn join_quoted(items: &[&'static str]) -> String {
    items
        .iter()
        .map(|s| format!("\"{s}\""))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Parse JSON and build a `.drawio` XML string. End-to-end entry point used
/// by the CLI.
pub fn build_xml(json: &str) -> Result<String, AuthorError> {
    let doc: Document = serde_json::from_str(json)?;
    build_xml_from_doc(&doc)
}

fn build_xml_from_doc(doc: &Document) -> Result<String, AuthorError> {
    let mut diagram = Diagram::new(&doc.name);
    let mut ids: HashSet<String> = HashSet::new();

    for g in &doc.groups {
        if !ids.insert(g.id.clone()) {
            return Err(AuthorError::DuplicateId(g.id.clone()));
        }
        let kind = parse_group_kind(&g.kind)?;
        diagram.add_group(GroupOpts::new(
            g.id.clone(),
            g.label.clone(),
            g.x,
            g.y,
            g.width,
            g.height,
            kind,
        ));
    }

    let mut node_refs = std::collections::HashMap::new();
    for n in &doc.nodes {
        if !ids.insert(n.id.clone()) {
            return Err(AuthorError::DuplicateId(n.id.clone()));
        }
        let node = build_node(n)?;
        let r = diagram.add_node(node);
        node_refs.insert(n.id.clone(), r);
    }

    for e in &doc.edges {
        let src = node_refs
            .get(&e.source)
            .ok_or_else(|| AuthorError::EdgeUnknownEndpoint {
                source: e.source.clone(),
                target: e.target.clone(),
                missing: e.source.clone(),
            })?;
        let tgt = node_refs
            .get(&e.target)
            .ok_or_else(|| AuthorError::EdgeUnknownEndpoint {
                source: e.source.clone(),
                target: e.target.clone(),
                missing: e.target.clone(),
            })?;
        let mut builder = diagram.connect(src, tgt);
        if let (Some(x), Some(y)) = (e.exit_x, e.exit_y) {
            builder = builder.exit(x, y);
        }
        if let (Some(x), Some(y)) = (e.entry_x, e.entry_y) {
            builder = builder.entry(x, y);
        }
        let _ = builder;
    }

    Ok(diagram.to_xml())
}

fn build_node(n: &NodeSpec) -> Result<Node, AuthorError> {
    if n.kind == "raw" {
        let style = n
            .style
            .as_deref()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| AuthorError::RawNodeMissingStyle { id: n.id.clone() })?;
        let w = n.width.unwrap_or(drawio_author::DEFAULT_AWS_TILE);
        let h = n.height.unwrap_or(drawio_author::DEFAULT_AWS_TILE);
        return Ok(Node::raw(
            n.id.clone(),
            n.x,
            n.y,
            w,
            h,
            n.label.clone(),
            style,
        ));
    }

    let factory = lookup_aws_factory(&n.kind)?;
    let mut node = factory(&n.id, &n.label);
    node.x = n.x;
    node.y = n.y;
    if let Some(w) = n.width {
        node.w = w;
    }
    if let Some(h) = n.height {
        node.h = h;
    }
    Ok(node)
}

/// Map a `kind` string to one of the AWS factory functions exposed by the
/// `drawio-author` crate. Returns the factory pointer so we can apply the
/// id/label without an extra match arm per call site.
fn lookup_aws_factory(kind: &str) -> Result<fn(&str, &str) -> Node, AuthorError> {
    let Some(rest) = kind.strip_prefix("aws.") else {
        return Err(AuthorError::UnknownNodeKind {
            kind: kind.to_string(),
            suggestions: suggest(kind, &NODE_KINDS),
        });
    };
    let f: fn(&str, &str) -> Node = match rest {
        "api_gateway" => aws::api_gateway,
        "lambda" => aws::lambda,
        "s3" => aws::s3,
        "dynamodb" => aws::dynamodb,
        "ec2" => aws::ec2,
        "sqs" => aws::sqs,
        "sns" => aws::sns,
        "cloudfront" => aws::cloudfront,
        "msk" => aws::msk,
        "iam" => aws::iam,
        "vpc" => aws::vpc,
        "eventbridge" => aws::eventbridge,
        "step_functions" => aws::step_functions,
        "appsync" => aws::appsync,
        "ecs" => aws::ecs,
        "eks" => aws::eks,
        "fargate" => aws::fargate,
        "app_runner" => aws::app_runner,
        "batch" => aws::batch,
        "rds" => aws::rds,
        "elasticache" => aws::elasticache,
        "efs" => aws::efs,
        "route_53" => aws::route_53,
        "elastic_load_balancing" => aws::elastic_load_balancing,
        "cognito" => aws::cognito,
        "secrets_manager" => aws::secrets_manager,
        "kms" => aws::kms,
        "kinesis" => aws::kinesis,
        "athena" => aws::athena,
        "cloudwatch" => aws::cloudwatch,
        _ => {
            return Err(AuthorError::UnknownNodeKind {
                kind: kind.to_string(),
                suggestions: suggest(kind, &NODE_KINDS),
            });
        }
    };
    Ok(f)
}

fn parse_group_kind(kind: &str) -> Result<GroupKind, AuthorError> {
    match kind {
        "aws.account" => Ok(GroupKind::AwsAccount),
        "aws.vpc" => Ok(GroupKind::AwsVpc),
        "aws.cloud" => Ok(GroupKind::AwsCloud),
        "generic" => Ok(GroupKind::Generic),
        other => Err(AuthorError::UnknownGroupKind {
            kind: other.to_string(),
            suggestions: suggest(other, &GROUP_KINDS),
        }),
    }
}

/// All accepted node `kind` strings.
pub const NODE_KINDS: [&str; 31] = [
    "raw",
    "aws.api_gateway",
    "aws.lambda",
    "aws.s3",
    "aws.dynamodb",
    "aws.ec2",
    "aws.sqs",
    "aws.sns",
    "aws.cloudfront",
    "aws.msk",
    "aws.iam",
    "aws.vpc",
    "aws.eventbridge",
    "aws.step_functions",
    "aws.appsync",
    "aws.ecs",
    "aws.eks",
    "aws.fargate",
    "aws.app_runner",
    "aws.batch",
    "aws.rds",
    "aws.elasticache",
    "aws.efs",
    "aws.route_53",
    "aws.elastic_load_balancing",
    "aws.cognito",
    "aws.secrets_manager",
    "aws.kms",
    "aws.kinesis",
    "aws.athena",
    "aws.cloudwatch",
];

/// All accepted group `kind` strings.
pub const GROUP_KINDS: [&str; 4] = ["aws.account", "aws.vpc", "aws.cloud", "generic"];

/// Pick up to three closest matches by Levenshtein distance ≤ 3. Returns an
/// empty Vec if nothing is close — keeps error messages clean for typos that
/// look nothing like the catalogue.
fn suggest(input: &str, known: &[&'static str]) -> Vec<&'static str> {
    let mut scored: Vec<(usize, &'static str)> = known
        .iter()
        .map(|k| (levenshtein(input, k), *k))
        .filter(|(d, _)| *d <= 3)
        .collect();
    scored.sort_by_key(|(d, _)| *d);
    scored.into_iter().take(3).map(|(_, s)| s).collect()
}

/// Tiny Levenshtein distance. Two-row DP. No allocations beyond two `Vec`s
/// the size of `b`. Kept private; we explicitly chose not to pull in a crate.
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr: Vec<usize> = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            curr[j + 1] = (curr[j] + 1).min(prev[j + 1] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_minimal_diagram() {
        let json = r#"{
            "name": "t",
            "nodes": [
                {"id": "api", "kind": "aws.api_gateway", "label": "API", "x": 0, "y": 0},
                {"id": "lam", "kind": "aws.lambda", "label": "Lambda", "x": 100, "y": 0}
            ],
            "edges": [{"source": "api", "target": "lam"}]
        }"#;
        let xml = build_xml(json).unwrap();
        assert!(xml.contains("source=\"api\" target=\"lam\""));
        assert!(xml.contains("resIcon=mxgraph.aws4.api_gateway"));
    }

    #[test]
    fn rejects_unknown_node_kind_with_suggestions() {
        let json = r#"{
            "nodes": [{"id": "x", "kind": "aws.lambba", "x": 0, "y": 0}]
        }"#;
        let err = build_xml(json).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("aws.lambba"), "msg: {msg}");
        assert!(msg.contains("aws.lambda"), "expected suggestion: {msg}");
    }

    #[test]
    fn rejects_raw_node_without_style() {
        let json = r#"{
            "nodes": [{"id": "x", "kind": "raw", "x": 0, "y": 0}]
        }"#;
        let err = build_xml(json).unwrap_err();
        assert!(matches!(err, AuthorError::RawNodeMissingStyle { .. }));
    }

    #[test]
    fn rejects_edge_pointing_at_unknown_id() {
        let json = r#"{
            "nodes": [{"id": "a", "kind": "aws.lambda", "x": 0, "y": 0}],
            "edges": [{"source": "a", "target": "b"}]
        }"#;
        let err = build_xml(json).unwrap_err();
        assert!(matches!(err, AuthorError::EdgeUnknownEndpoint { .. }));
    }

    #[test]
    fn raw_node_uses_provided_style_and_default_size() {
        let json = r#"{
            "nodes": [{
                "id": "r", "kind": "raw", "label": "Custom",
                "x": 10, "y": 20,
                "style": "shape=mxgraph.aws4.resourceIcon;resIcon=mxgraph.aws4.athena;"
            }]
        }"#;
        let xml = build_xml(json).unwrap();
        assert!(xml.contains("resIcon=mxgraph.aws4.athena"), "{xml}");
        assert!(xml.contains("width=\"78\" height=\"78\""));
    }

    #[test]
    fn group_kind_maps_to_aws_account() {
        let json = r#"{
            "groups": [{
                "id": "acc", "kind": "aws.account", "label": "Acc",
                "x": 0, "y": 0, "width": 100, "height": 100
            }]
        }"#;
        let xml = build_xml(json).unwrap();
        assert!(xml.contains("grIcon=mxgraph.aws4.group_account"), "{xml}");
    }

    #[test]
    fn duplicate_id_is_rejected() {
        let json = r#"{
            "nodes": [
                {"id": "x", "kind": "aws.lambda", "x": 0, "y": 0},
                {"id": "x", "kind": "aws.s3", "x": 0, "y": 0}
            ]
        }"#;
        let err = build_xml(json).unwrap_err();
        assert!(matches!(err, AuthorError::DuplicateId(_)));
    }
}
