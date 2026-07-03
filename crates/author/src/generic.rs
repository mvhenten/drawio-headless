//! Vendor-neutral infrastructure catalogue: cloud, database, queue, document.
//!
//! `database`, `queue` and `document` reuse the AWS4 stencil set's "General
//! Icons" glyphs — the same solid tile treatment `client` uses — since AWS's
//! own catalogue already ships grayscale, unbranded versions of these
//! concepts for depicting non-AWS parts of a diagram.
//!
//! `cloud` has no AWS-general equivalent, so it is sourced from the legacy
//! Azure stencil set's plain "Cloud" shape (`mxgraph.azure.cloud`) and
//! rendered through the Azure raw-stencil path (a solid, coloured
//! silhouette, no surrounding tile). That stencil's rounded outline is built
//! entirely from `<arc>` commands, so it depends on the `<arc>` stencil DSL
//! support added alongside this catalogue (see
//! `crates/render/src/stencil.rs`, issue #7) — without it the shape was a
//! near-empty outline.

use crate::Node;
use crate::aws::aws_node;

/// Neutral slate-gray fill shared with the `client` catalogue — keeps
/// unbranded shapes visually distinct from any vendor's brand colour.
pub const GENERIC_FILL: &str = "#5A6C86";

/// Database (cylinder).
pub fn database(id: &str, label: &str) -> Node {
    aws_node(id, label, GENERIC_FILL, "mxgraph.aws4.database")
}

/// Queue.
pub fn queue(id: &str, label: &str) -> Node {
    aws_node(id, label, GENERIC_FILL, "mxgraph.aws4.queue")
}

/// Document.
pub fn document(id: &str, label: &str) -> Node {
    aws_node(id, label, GENERIC_FILL, "mxgraph.aws4.document")
}

/// Default tile size for [`cloud`], matching the legacy Azure "Cloud"
/// stencil's native aspect ratio (100.34 x 66.09), rounded to a convenient
/// 100x66.
pub const CLOUD_W: f64 = 100.0;
/// See [`CLOUD_W`].
pub const CLOUD_H: f64 = 66.0;

/// Vendor-neutral cloud silhouette (e.g. "the internet" / an unspecified
/// hosting environment).
pub fn cloud(id: &str, label: &str) -> Node {
    Node {
        id: id.to_string(),
        label: label.to_string(),
        x: 0.0,
        y: 0.0,
        w: CLOUD_W,
        h: CLOUD_H,
        style: format!(
            "verticalLabelPosition=bottom;html=0;verticalAlign=top;align=center;\
             strokeColor=none;fillColor={GENERIC_FILL};shape=mxgraph.azure.cloud;"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_generic_style(node: &Node, res_icon: &str) {
        assert!(
            node.style.contains(&format!("resIcon={res_icon}")),
            "missing resIcon={res_icon} in style: {}",
            node.style,
        );
        assert!(
            node.style.contains(&format!("fillColor={GENERIC_FILL}")),
            "missing generic fill in style: {}",
            node.style,
        );
        assert!(node.style.contains("shape=mxgraph.aws4.resourceIcon"));
    }

    #[test]
    fn database_factory() {
        assert_generic_style(&database("db", "Database"), "mxgraph.aws4.database");
    }

    #[test]
    fn queue_factory() {
        assert_generic_style(&queue("q", "Job queue"), "mxgraph.aws4.queue");
    }

    #[test]
    fn document_factory() {
        assert_generic_style(&document("d", "Report"), "mxgraph.aws4.document");
    }

    #[test]
    fn cloud_factory() {
        let node = cloud("c", "Internet");
        assert!(node.style.contains("shape=mxgraph.azure.cloud;"));
        assert!(node.style.contains(&format!("fillColor={GENERIC_FILL}")));
        assert!((node.w - CLOUD_W).abs() < f64::EPSILON);
        assert!((node.h - CLOUD_H).abs() < f64::EPSILON);
    }
}
