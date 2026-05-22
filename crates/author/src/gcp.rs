//! Curated Google Cloud Platform service catalogue.
//!
//! Each function returns a [`Node`](crate::Node) preconfigured with a style
//! string in the upstream drawio convention for the GCP vector stencils:
//!
//! ```text
//! shape=mxgraph.gcp.<category>.<key>;strokeColor=none;
//! verticalLabelPosition=bottom;verticalAlign=top;align=center;html=0;
//! ```
//!
//! The renderer in `drawio-render` resolves `shape=mxgraph.gcp.<...>.<...>`
//! against `stencils/gcp.xml` (a concatenation of the upstream
//! per-category files; see `stencils/SOURCE-gcp`).
//!
//! Caveats
//! -------
//! The GCP stencils use mxStencil commands the renderer does not yet
//! implement (`<arc>`, `<save>`/`<restore>`, `<alpha>`, `<strokecolor>`,
//! `<fillcolor>`). Style strings are correct and round-trip through the
//! upstream drawio editor, but rasterised glyphs in this crate are
//! currently partial. Tracked in #7.

use crate::Node;

/// Default tile dimensions for GCP product icons. Matches the native
/// `w x h` of most `mxgraph.gcp.*` stencils (~129x114), rounded to a
/// convenient square.
pub const DEFAULT_GCP_TILE: f64 = 78.0;

/// GCP "Google Blue" — the canonical glyph colour drawio uses across the
/// `mxgraph.gcp.*` palette.
pub const GCP_FILL: &str = "#4285F4";

fn gcp_style(category: &str, shape_key: &str) -> String {
    format!(
        "verticalLabelPosition=bottom;html=0;verticalAlign=top;align=center;\
         strokeColor=none;fillColor={GCP_FILL};\
         shape=mxgraph.gcp.{category}.{shape_key};"
    )
}

fn gcp_node(id: &str, label: &str, category: &str, shape_key: &str) -> Node {
    Node {
        id: id.to_string(),
        label: label.to_string(),
        x: 0.0,
        y: 0.0,
        w: DEFAULT_GCP_TILE,
        h: DEFAULT_GCP_TILE,
        style: gcp_style(category, shape_key),
    }
}

/// App Engine (Compute).
pub fn app_engine(id: &str, label: &str) -> Node {
    gcp_node(id, label, "compute", "app_engine")
}

/// Cloud Functions (Compute / Serverless).
pub fn cloud_functions(id: &str, label: &str) -> Node {
    gcp_node(id, label, "compute", "cloud_functions")
}

/// Compute Engine (Compute).
pub fn compute_engine(id: &str, label: &str) -> Node {
    gcp_node(id, label, "compute", "compute_engine")
}

/// Container Engine / Google Kubernetes Engine (Compute).
pub fn gke(id: &str, label: &str) -> Node {
    gcp_node(id, label, "compute", "container_engine")
}

/// Cloud Storage (Storage).
pub fn cloud_storage(id: &str, label: &str) -> Node {
    gcp_node(id, label, "storage_databases", "cloud_storage")
}

/// `BigQuery` (Big Data / Analytics).
pub fn bigquery(id: &str, label: &str) -> Node {
    gcp_node(id, label, "big_data", "bigquery")
}

/// Cloud Pub/Sub (Big Data / Messaging).
pub fn pubsub(id: &str, label: &str) -> Node {
    gcp_node(id, label, "big_data", "cloud_pubsub")
}

/// Cloud SQL (Storage / Databases).
pub fn cloud_sql(id: &str, label: &str) -> Node {
    gcp_node(id, label, "storage_databases", "cloud_sql")
}

/// Cloud Datastore (Storage / Databases — predecessor to Firestore).
pub fn cloud_datastore(id: &str, label: &str) -> Node {
    gcp_node(id, label, "storage_databases", "cloud_datastore")
}

/// Cloud Bigtable (Storage / Databases).
pub fn bigtable(id: &str, label: &str) -> Node {
    gcp_node(id, label, "storage_databases", "cloud_bigtable")
}

/// Cloud CDN (Networking).
pub fn cloud_cdn(id: &str, label: &str) -> Node {
    gcp_node(id, label, "networking", "cloud_cdn")
}

/// Cloud Load Balancing (Networking).
pub fn cloud_load_balancing(id: &str, label: &str) -> Node {
    gcp_node(id, label, "networking", "cloud_load_balancing")
}

/// Cloud DNS (Networking).
pub fn cloud_dns(id: &str, label: &str) -> Node {
    gcp_node(id, label, "networking", "cloud_dns")
}

/// Cloud IAM (Identity & Security).
pub fn iam(id: &str, label: &str) -> Node {
    gcp_node(id, label, "identity_and_security", "cloud_iam")
}

/// Cloud Logging (Management Tools).
pub fn logging(id: &str, label: &str) -> Node {
    gcp_node(id, label, "management_tools", "logging")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_gcp_style(node: &Node, category: &str, shape_key: &str) {
        let expected = format!("shape=mxgraph.gcp.{category}.{shape_key};");
        assert!(
            node.style.contains(&expected),
            "missing {expected} in style: {}",
            node.style,
        );
        assert!(
            node.style.contains(&format!("fillColor={GCP_FILL}")),
            "missing GCP fill in style: {}",
            node.style,
        );
    }

    #[test]
    fn compute_engine_factory() {
        assert_gcp_style(&compute_engine("vm", "VM"), "compute", "compute_engine");
    }

    #[test]
    fn bigquery_factory() {
        assert_gcp_style(&bigquery("bq", "Warehouse"), "big_data", "bigquery");
    }

    #[test]
    fn cloud_storage_factory() {
        assert_gcp_style(
            &cloud_storage("gcs", "Bucket"),
            "storage_databases",
            "cloud_storage",
        );
    }

    #[test]
    fn pubsub_factory() {
        assert_gcp_style(&pubsub("ps", "Topic"), "big_data", "cloud_pubsub");
    }

    #[test]
    fn gke_factory() {
        assert_gcp_style(&gke("gke", "Cluster"), "compute", "container_engine");
    }

    #[test]
    fn cloud_functions_factory() {
        assert_gcp_style(&cloud_functions("fn", "Fn"), "compute", "cloud_functions");
    }

    #[test]
    fn iam_factory() {
        assert_gcp_style(&iam("iam", "Roles"), "identity_and_security", "cloud_iam");
    }

    #[test]
    fn cloud_load_balancing_factory() {
        assert_gcp_style(
            &cloud_load_balancing("lb", "LB"),
            "networking",
            "cloud_load_balancing",
        );
    }
}
