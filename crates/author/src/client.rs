//! Curated client/actor catalogue.
//!
//! Every architecture diagram needs a handful of vendor-neutral "edges" —
//! the browser or mobile app a user drives, the person themselves, some
//! external system the diagram doesn't own — but no vendor's icon catalogue
//! models them, so authors reached for the wrong vendor's glyph (see issue
//! #29: an AWS IAM icon standing in for an end user).
//!
//! These glyphs are sourced from the AWS4 stencil set's "General Icons"
//! category — the same grayscale silhouettes AWS's own reference
//! architectures use to depict people, clients and the internet sitting
//! outside the AWS boundary. Reusing [`aws::aws_node`](crate::aws) gets the
//! same solid tile treatment (coloured square + white glyph) as every AWS
//! factory, so these render solid regardless of stencil DSL coverage.

use crate::Node;
use crate::aws::aws_node;

/// Neutral slate-gray fill for client/actor tiles. Matches the muted tone
/// [`GroupKind::Generic`](crate::GroupKind::Generic) already uses, keeping
/// unbranded shapes visually distinct from any vendor's brand colour.
pub const CLIENT_FILL: &str = "#5A6C86";

/// Browser / web client.
pub fn browser(id: &str, label: &str) -> Node {
    aws_node(id, label, CLIENT_FILL, "mxgraph.aws4.client")
}

/// Mobile app / mobile client.
pub fn mobile(id: &str, label: &str) -> Node {
    aws_node(id, label, CLIENT_FILL, "mxgraph.aws4.mobile_client")
}

/// End user / person / actor.
pub fn person(id: &str, label: &str) -> Node {
    aws_node(id, label, CLIENT_FILL, "mxgraph.aws4.user")
}

/// Generic external system reachable over the network — the diagram's
/// boundary, not owned by whatever's being drawn.
pub fn external_system(id: &str, label: &str) -> Node {
    aws_node(id, label, CLIENT_FILL, "mxgraph.aws4.internet")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_client_style(node: &Node, res_icon: &str) {
        assert!(
            node.style.contains(&format!("resIcon={res_icon}")),
            "missing resIcon={res_icon} in style: {}",
            node.style,
        );
        assert!(
            node.style.contains(&format!("fillColor={CLIENT_FILL}")),
            "missing client fill in style: {}",
            node.style,
        );
        assert!(node.style.contains("shape=mxgraph.aws4.resourceIcon"));
    }

    #[test]
    fn browser_factory() {
        assert_client_style(&browser("b", "Browser"), "mxgraph.aws4.client");
    }

    #[test]
    fn mobile_factory() {
        assert_client_style(&mobile("m", "Mobile app"), "mxgraph.aws4.mobile_client");
    }

    #[test]
    fn person_factory() {
        assert_client_style(&person("p", "End user"), "mxgraph.aws4.user");
    }

    #[test]
    fn external_system_factory() {
        assert_client_style(
            &external_system("e", "Partner API"),
            "mxgraph.aws4.internet",
        );
    }
}
