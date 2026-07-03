//! Kubernetes deployment topology: a browser reaches an Ingress that fronts
//! a Service, Deployment, `ReplicaSet` and Pod living inside the "orders"
//! namespace, with a `ConfigMap` and Secret feeding the Deployment and a
//! cluster-scoped `PersistentVolume` attached to the Pod from outside it.
//!
//! Showcases a `GroupKind::Generic` container (grey dashed, no vendor
//! `grIcon`) as opposed to the AWS-account boundary in the cross-account
//! example, plus multiple entry sides on one node (Deployment takes edges
//! from its top, left and bottom).

use drawio_author::{Diagram, GroupKind, GroupOpts, client, k8s};

fn main() -> std::io::Result<()> {
    let mut d = Diagram::new("Kubernetes deployment topology");

    d.add_group(GroupOpts::new(
        "orders-ns",
        "Namespace: orders",
        200.0,
        280.0,
        270.0,
        510.0,
        GroupKind::Generic,
    ));

    let browser = d.add_node(client::browser("browser", "Browser").at(386.0, 40.0));
    let ingress = d.add_node(k8s::ingress("ingress", "Ingress").at(400.0, 180.0));

    let service = d.add_node(k8s::service("service", "Service").at(400.0, 300.0));
    let config_map = d.add_node(k8s::config_map("config", "Config").at(220.0, 420.0));
    let secret = d.add_node(k8s::secret("secret", "Secret").at(220.0, 520.0));
    let deployment = d.add_node(k8s::deployment("deployment", "Deployment").at(400.0, 450.0));
    let replica_set = d.add_node(k8s::replica_set("replicas", "ReplicaSet").at(400.0, 600.0));
    let pod = d.add_node(k8s::pod("pod", "Pod").at(400.0, 720.0));

    let volume = d.add_node(k8s::persistent_volume("volume", "Volume").at(700.0, 720.0));

    d.connect(&browser, &ingress);
    d.connect(&ingress, &service);
    d.connect(&service, &deployment);
    d.connect(&config_map, &deployment)
        .exit(1.0, 0.5)
        .entry(0.0, 0.3);
    d.connect(&secret, &deployment)
        .exit(1.0, 0.5)
        .entry(0.0, 0.7);
    d.connect(&deployment, &replica_set);
    d.connect(&replica_set, &pod);
    d.connect(&pod, &volume);

    drawio_headless_examples::write_artifacts("k8s-deployment", &d)
}
