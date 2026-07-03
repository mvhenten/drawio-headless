//! Curated Azure service catalogue.
//!
//! Each function returns a [`Node`](crate::Node) preconfigured with a style
//! string in the upstream drawio convention for Azure stencils:
//!
//! ```text
//! shape=mxgraph.azure.<key>;fillColor=#00BEF2;strokeColor=none;
//! verticalLabelPosition=bottom;verticalAlign=top;align=center;html=0;
//! ```
//!
//! Unlike AWS, the legacy Azure stencil set does not use a `resourceIcon`
//! wrapper — the shape attribute itself names the stencil. The renderer in
//! `drawio-render` resolves `shape=mxgraph.azure.<key>` against
//! `stencils/azure.xml`.
//!
//! Caveats
//! -------
//! These shapes draw via stencil DSL commands the renderer only partially
//! supports today (see issue #7 and `crates/render/src/stencil.rs`). The
//! style strings are correct and round-trip through the upstream drawio
//! editor, but the rasterised glyphs may be missing detail until the DSL
//! coverage is extended.

use crate::Node;

/// Canonical Azure tile dimensions. The legacy stencils' native aspect
/// ratios vary; we pick a 50x50 square that matches what
/// `Sidebar-Azure.js` uses as the baseline (`w = h = 50`).
pub const DEFAULT_AZURE_TILE: f64 = 50.0;

/// Azure brand cyan, used as the default fill colour for all stencils in
/// the legacy `mxgraph.azure.*` palette.
pub const AZURE_FILL: &str = "#00BEF2";

fn azure_style(shape_key: &str) -> String {
    format!(
        "verticalLabelPosition=bottom;html=0;verticalAlign=top;align=center;\
         strokeColor=none;fillColor={AZURE_FILL};shape=mxgraph.azure.{shape_key};"
    )
}

fn azure_node(id: &str, label: &str, shape_key: &str) -> Node {
    Node {
        id: id.to_string(),
        label: label.to_string(),
        x: 0.0,
        y: 0.0,
        w: DEFAULT_AZURE_TILE,
        h: DEFAULT_AZURE_TILE,
        style: azure_style(shape_key),
    }
}

/// Azure Active Directory (Identity).
pub fn active_directory(id: &str, label: &str) -> Node {
    azure_node(id, label, "azure_active_directory")
}

/// Entra ID (Identity). Microsoft renamed Azure Active Directory to Entra ID
/// in 2023; the legacy stencil set still carries the old product name, so
/// this is the same glyph as [`active_directory`] under the current name —
/// the one authors reach for in a fresh diagram (see issue #29).
pub fn entra_id(id: &str, label: &str) -> Node {
    azure_node(id, label, "azure_active_directory")
}

/// Multi-Factor Authentication (Identity) — the MFA challenge that commonly
/// sits alongside Entra ID in hybrid identity diagrams.
pub fn multi_factor_authentication(id: &str, label: &str) -> Node {
    azure_node(id, label, "multi_factor_authentication")
}

/// Generic on-premises/Azure server (Compute) — the "on-prem" side of a
/// hybrid topology diagram (e.g. behind `ExpressRoute` or a VPN gateway).
pub fn server(id: &str, label: &str) -> Node {
    azure_node(id, label, "server")
}

/// Generic Storage account (Storage), distinct from the specific
/// [`storage_blob`]/[`storage_queue`] data-type glyphs.
pub fn storage(id: &str, label: &str) -> Node {
    azure_node(id, label, "storage")
}

/// Azure Cache (Databases / Redis).
pub fn cache(id: &str, label: &str) -> Node {
    azure_node(id, label, "azure_cache")
}

/// Azure Load Balancer (Networking).
pub fn load_balancer(id: &str, label: &str) -> Node {
    azure_node(id, label, "azure_load_balancer")
}

/// Azure Website / App Service (Compute).
pub fn website(id: &str, label: &str) -> Node {
    azure_node(id, label, "azure_website")
}

/// Cloud Service / App Service plan (Compute).
pub fn cloud_service(id: &str, label: &str) -> Node {
    azure_node(id, label, "cloud_service")
}

/// Content Delivery Network (Networking).
pub fn cdn(id: &str, label: &str) -> Node {
    azure_node(id, label, "content_delivery_network")
}

/// Express Route (Networking).
pub fn express_route(id: &str, label: &str) -> Node {
    azure_node(id, label, "express_route")
}

/// Notification Hub (Application Integration).
pub fn notification_hub(id: &str, label: &str) -> Node {
    azure_node(id, label, "notification_hub")
}

/// Service Bus (Application Integration).
pub fn service_bus(id: &str, label: &str) -> Node {
    azure_node(id, label, "service_bus")
}

/// SQL Database (Databases).
pub fn sql_database(id: &str, label: &str) -> Node {
    azure_node(id, label, "sql_database")
}

/// Storage Blob (Storage).
pub fn storage_blob(id: &str, label: &str) -> Node {
    azure_node(id, label, "storage_blob")
}

/// Storage Queue (Storage).
pub fn storage_queue(id: &str, label: &str) -> Node {
    azure_node(id, label, "storage_queue")
}

/// Traffic Manager (Networking).
pub fn traffic_manager(id: &str, label: &str) -> Node {
    azure_node(id, label, "traffic_manager")
}

/// Virtual Machine (Compute).
pub fn virtual_machine(id: &str, label: &str) -> Node {
    azure_node(id, label, "virtual_machine")
}

/// Virtual Network (Networking).
pub fn virtual_network(id: &str, label: &str) -> Node {
    azure_node(id, label, "virtual_network")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_azure_style(node: &Node, shape_key: &str) {
        let expected_shape = format!("shape=mxgraph.azure.{shape_key};");
        assert!(
            node.style.contains(&expected_shape),
            "missing {expected_shape} in style: {}",
            node.style,
        );
        assert!(
            node.style.contains(&format!("fillColor={AZURE_FILL}")),
            "missing Azure fill in style: {}",
            node.style,
        );
    }

    #[test]
    fn active_directory_factory() {
        assert_azure_style(&active_directory("ad", "Tenant"), "azure_active_directory");
    }

    #[test]
    fn entra_id_factory() {
        // Same underlying glyph as active_directory, under the current
        // product name.
        assert_azure_style(&entra_id("eid", "Entra ID"), "azure_active_directory");
    }

    #[test]
    fn multi_factor_authentication_factory() {
        assert_azure_style(
            &multi_factor_authentication("mfa", "MFA"),
            "multi_factor_authentication",
        );
    }

    #[test]
    fn server_factory() {
        assert_azure_style(&server("srv", "On-prem server"), "server");
    }

    #[test]
    fn storage_factory() {
        assert_azure_style(&storage("st", "Storage account"), "storage");
    }

    #[test]
    fn sql_database_factory() {
        assert_azure_style(&sql_database("db", "Orders"), "sql_database");
    }

    #[test]
    fn service_bus_factory() {
        assert_azure_style(&service_bus("sb", "Bus"), "service_bus");
    }

    #[test]
    fn virtual_machine_factory() {
        assert_azure_style(&virtual_machine("vm", "App"), "virtual_machine");
    }

    #[test]
    fn website_factory() {
        assert_azure_style(&website("w", "App"), "azure_website");
    }

    #[test]
    fn storage_blob_factory() {
        assert_azure_style(&storage_blob("sb", "Files"), "storage_blob");
    }

    #[test]
    fn traffic_manager_factory() {
        assert_azure_style(&traffic_manager("tm", "Geo"), "traffic_manager");
    }

    #[test]
    fn virtual_network_factory() {
        assert_azure_style(&virtual_network("vnet", "Net"), "virtual_network");
    }
}
