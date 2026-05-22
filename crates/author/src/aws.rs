//! Curated AWS service catalog.
//!
//! Each function returns a [`Node`](crate::Node) preconfigured with the AWS
//! resource-icon style string used by the upstream drawio app
//! (`shape=mxgraph.aws4.resourceIcon`). `resIcon` identifies the specific
//! glyph (e.g. `mxgraph.aws4.lambda`). The renderer in `drawio-render` looks
//! these up against the vendored `stencils/aws4.xml`.

use crate::{DEFAULT_AWS_TILE, Node};

/// Build a resource-icon style string with the given fill colour and
/// `resIcon` identifier.
fn res_icon_style(fill: &str, res_icon: &str) -> String {
    format!(
        "sketch=0;outlineConnect=0;fontColor=#232F3E;fillColor={fill};\
         strokeColor=#ffffff;dashed=0;verticalLabelPosition=bottom;\
         verticalAlign=top;align=center;html=0;fontSize=12;aspect=fixed;\
         shape=mxgraph.aws4.resourceIcon;resIcon={res_icon};"
    )
}

fn aws_node(id: &str, label: &str, fill: &str, res_icon: &str) -> Node {
    Node {
        id: id.to_string(),
        label: label.to_string(),
        x: 0.0,
        y: 0.0,
        w: DEFAULT_AWS_TILE,
        h: DEFAULT_AWS_TILE,
        style: res_icon_style(fill, res_icon),
    }
}

/// API Gateway tile.
pub fn api_gateway(id: &str, label: &str) -> Node {
    aws_node(id, label, "#E7157B", "mxgraph.aws4.api_gateway")
}

/// Lambda tile.
pub fn lambda(id: &str, label: &str) -> Node {
    aws_node(id, label, "#ED7100", "mxgraph.aws4.lambda")
}

/// S3 tile.
pub fn s3(id: &str, label: &str) -> Node {
    aws_node(id, label, "#7AA116", "mxgraph.aws4.s3")
}

/// `DynamoDB` tile.
pub fn dynamodb(id: &str, label: &str) -> Node {
    aws_node(id, label, "#C925D1", "mxgraph.aws4.dynamodb")
}

/// EC2 tile.
pub fn ec2(id: &str, label: &str) -> Node {
    aws_node(id, label, "#ED7100", "mxgraph.aws4.ec2")
}
