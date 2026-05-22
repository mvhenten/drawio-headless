//! Curated Kubernetes resource catalogue.
//!
//! Each function returns a [`Node`](crate::Node) preconfigured with a style
//! string in the upstream drawio convention for the Kubernetes icon set:
//!
//! ```text
//! shape=mxgraph.kubernetes.icon2;prIcon=<key>;
//! fillColor=#2875E2;strokeColor=#ffffff;...
//! ```
//!
//! `mxgraph.kubernetes.icon2` is the "icon-in-blue-tile" template, with the
//! `prIcon` attribute selecting the glyph (e.g. `pod`, `deploy`). The
//! renderer in `drawio-render` resolves `prIcon` against
//! `stencils/kubernetes.xml`.
//!
//! `kubernetesLabel=1` toggles a blank labelled tile underneath the icon
//! (so the icon appears at half height and the label area is reserved
//! within the tile). This catalogue uses the simpler icon-only form.

use crate::Node;

/// Default tile dimensions for Kubernetes resources. Matches the
/// `w * 0.5 x h * 0.48` ratio the upstream sidebar uses on a 100x100
/// base — rounded to a convenient 50x48.
pub const DEFAULT_K8S_TILE_W: f64 = 50.0;
/// Default tile height. The kubernetes `icon2` shape is taller than wide
/// in the canonical sidebar definition.
pub const DEFAULT_K8S_TILE_H: f64 = 48.0;

/// Kubernetes brand blue (matches `Sidebar-Kubernetes.js`).
pub const K8S_FILL: &str = "#2875E2";

fn k8s_style(pr_icon: &str) -> String {
    format!(
        "sketch=0;html=1;dashed=0;whitespace=wrap;\
         verticalLabelPosition=bottom;verticalAlign=top;\
         fillColor={K8S_FILL};strokeColor=#ffffff;\
         points=[[0.005,0.63,0],[0.1,0.2,0],[0.9,0.2,0],[0.5,0,0],\
         [0.995,0.63,0],[0.72,0.99,0],[0.5,1,0],[0.28,0.99,0]];\
         shape=mxgraph.kubernetes.icon2;prIcon={pr_icon};"
    )
}

fn k8s_node(id: &str, label: &str, pr_icon: &str) -> Node {
    Node {
        id: id.to_string(),
        label: label.to_string(),
        x: 0.0,
        y: 0.0,
        w: DEFAULT_K8S_TILE_W,
        h: DEFAULT_K8S_TILE_H,
        style: k8s_style(pr_icon),
    }
}

/// Pod — the smallest deployable unit.
pub fn pod(id: &str, label: &str) -> Node {
    k8s_node(id, label, "pod")
}

/// Deployment.
pub fn deployment(id: &str, label: &str) -> Node {
    k8s_node(id, label, "deploy")
}

/// Service (`svc`).
pub fn service(id: &str, label: &str) -> Node {
    k8s_node(id, label, "svc")
}

/// Ingress (`ing`).
pub fn ingress(id: &str, label: &str) -> Node {
    k8s_node(id, label, "ing")
}

/// `ConfigMap` (`cm`).
pub fn config_map(id: &str, label: &str) -> Node {
    k8s_node(id, label, "cm")
}

/// Secret.
pub fn secret(id: &str, label: &str) -> Node {
    k8s_node(id, label, "secret")
}

/// Namespace (`ns`).
pub fn namespace(id: &str, label: &str) -> Node {
    k8s_node(id, label, "ns")
}

/// Node.
pub fn node(id: &str, label: &str) -> Node {
    k8s_node(id, label, "node")
}

/// Persistent Volume (`pv`).
pub fn persistent_volume(id: &str, label: &str) -> Node {
    k8s_node(id, label, "pv")
}

/// `ReplicaSet` (`rs`).
pub fn replica_set(id: &str, label: &str) -> Node {
    k8s_node(id, label, "rs")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_k8s_style(node: &Node, pr_icon: &str) {
        let expected = format!("prIcon={pr_icon};");
        assert!(
            node.style.contains(&expected),
            "missing {expected} in style: {}",
            node.style,
        );
        assert!(
            node.style.contains("shape=mxgraph.kubernetes.icon2"),
            "missing K8s icon2 shape in style: {}",
            node.style,
        );
        assert!(
            node.style.contains(&format!("fillColor={K8S_FILL}")),
            "missing K8s fill in style: {}",
            node.style,
        );
    }

    #[test]
    fn pod_factory() {
        assert_k8s_style(&pod("p", "Pod"), "pod");
    }

    #[test]
    fn deployment_factory() {
        assert_k8s_style(&deployment("d", "Deploy"), "deploy");
    }

    #[test]
    fn service_factory() {
        assert_k8s_style(&service("s", "Svc"), "svc");
    }

    #[test]
    fn ingress_factory() {
        assert_k8s_style(&ingress("i", "Ing"), "ing");
    }

    #[test]
    fn config_map_factory() {
        assert_k8s_style(&config_map("cm", "Cfg"), "cm");
    }

    #[test]
    fn secret_factory() {
        assert_k8s_style(&secret("sec", "Sec"), "secret");
    }

    #[test]
    fn namespace_factory() {
        assert_k8s_style(&namespace("ns", "NS"), "ns");
    }

    #[test]
    fn node_factory() {
        assert_k8s_style(&node("n", "Node"), "node");
    }

    #[test]
    fn persistent_volume_factory() {
        assert_k8s_style(&persistent_volume("pv", "PV"), "pv");
    }

    #[test]
    fn replica_set_factory() {
        assert_k8s_style(&replica_set("rs", "RS"), "rs");
    }
}
