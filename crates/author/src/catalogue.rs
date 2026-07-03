//! Static catalogue metadata for every curated factory in this crate.
//!
//! The CLI consumes [`ENTRIES`] to power `list-shapes` without reflecting on
//! the factory functions themselves. Each [`Entry`] carries the library
//! prefix (`aws`, `azure`, `gcp`, `k8s`, `client`, `generic`), the factory
//! key (matching the function name in the relevant module), and a
//! free-form `category` label used to group output for humans.
//!
//! `client` and `generic` are vendor-neutral: browsers/mobile apps/people/
//! external systems, and cloud/database/queue/document shapes respectively
//! (issue #29). Their glyphs are still sourced from vendor stencil files
//! (mostly AWS4's unbranded "General Icons"), but the catalogue groups them
//! by what they represent, not by where the vector art came from.
//!
//! The list is hand-maintained; tests in the CLI assert key well-known
//! members exist so accidental deletions don't pass silently.

/// One row in the curated factory catalogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    /// Library namespace: `"aws"`, `"azure"`, `"gcp"`, `"k8s"`, `"client"`,
    /// or `"generic"`.
    pub library: &'static str,
    /// Factory key as it appears in `<library>.<key>` form (e.g. `lambda`).
    pub key: &'static str,
    /// Human-friendly grouping label (e.g. `"Compute"`).
    pub category: &'static str,
}

impl Entry {
    /// Render the full `library.key` form, the same shape kinds the CLI's
    /// `author` subcommand accepts.
    #[must_use]
    pub fn qualified_kind(&self) -> String {
        format!("{}.{}", self.library, self.key)
    }
}

/// Every curated factory exposed by the `aws`, `azure`, `gcp`, and `k8s`
/// modules, in stable display order (grouped by library, then category, then
/// alphabetical within a category).
pub const ENTRIES: &[Entry] = &[
    // -- AWS --
    Entry {
        library: "aws",
        key: "api_gateway",
        category: "Application Integration",
    },
    Entry {
        library: "aws",
        key: "appsync",
        category: "Application Integration",
    },
    Entry {
        library: "aws",
        key: "eventbridge",
        category: "Application Integration",
    },
    Entry {
        library: "aws",
        key: "sns",
        category: "Application Integration",
    },
    Entry {
        library: "aws",
        key: "sqs",
        category: "Application Integration",
    },
    Entry {
        library: "aws",
        key: "step_functions",
        category: "Application Integration",
    },
    Entry {
        library: "aws",
        key: "app_runner",
        category: "Compute",
    },
    Entry {
        library: "aws",
        key: "batch",
        category: "Compute",
    },
    Entry {
        library: "aws",
        key: "ec2",
        category: "Compute",
    },
    Entry {
        library: "aws",
        key: "ecs",
        category: "Compute",
    },
    Entry {
        library: "aws",
        key: "eks",
        category: "Compute",
    },
    Entry {
        library: "aws",
        key: "fargate",
        category: "Compute",
    },
    Entry {
        library: "aws",
        key: "lambda",
        category: "Compute",
    },
    Entry {
        library: "aws",
        key: "dynamodb",
        category: "Database",
    },
    Entry {
        library: "aws",
        key: "elasticache",
        category: "Database",
    },
    Entry {
        library: "aws",
        key: "rds",
        category: "Database",
    },
    Entry {
        library: "aws",
        key: "efs",
        category: "Storage",
    },
    Entry {
        library: "aws",
        key: "s3",
        category: "Storage",
    },
    Entry {
        library: "aws",
        key: "cloudfront",
        category: "Networking & Content Delivery",
    },
    Entry {
        library: "aws",
        key: "elastic_load_balancing",
        category: "Networking & Content Delivery",
    },
    Entry {
        library: "aws",
        key: "route_53",
        category: "Networking & Content Delivery",
    },
    Entry {
        library: "aws",
        key: "vpc",
        category: "Networking & Content Delivery",
    },
    Entry {
        library: "aws",
        key: "cognito",
        category: "Security, Identity & Compliance",
    },
    Entry {
        library: "aws",
        key: "iam",
        category: "Security, Identity & Compliance",
    },
    Entry {
        library: "aws",
        key: "kms",
        category: "Security, Identity & Compliance",
    },
    Entry {
        library: "aws",
        key: "secrets_manager",
        category: "Security, Identity & Compliance",
    },
    Entry {
        library: "aws",
        key: "athena",
        category: "Analytics",
    },
    Entry {
        library: "aws",
        key: "kinesis",
        category: "Analytics",
    },
    Entry {
        library: "aws",
        key: "msk",
        category: "Analytics",
    },
    Entry {
        library: "aws",
        key: "opensearch",
        category: "Analytics",
    },
    Entry {
        library: "aws",
        key: "cloudwatch",
        category: "Management & Governance",
    },
    // -- Azure (legacy mxgraph.azure.* set) --
    Entry {
        library: "azure",
        key: "active_directory",
        category: "Identity",
    },
    Entry {
        library: "azure",
        key: "entra_id",
        category: "Identity",
    },
    Entry {
        library: "azure",
        key: "multi_factor_authentication",
        category: "Identity",
    },
    Entry {
        library: "azure",
        key: "cache",
        category: "Database",
    },
    Entry {
        library: "azure",
        key: "sql_database",
        category: "Database",
    },
    Entry {
        library: "azure",
        key: "cloud_service",
        category: "Compute",
    },
    Entry {
        library: "azure",
        key: "virtual_machine",
        category: "Compute",
    },
    Entry {
        library: "azure",
        key: "website",
        category: "Compute",
    },
    Entry {
        library: "azure",
        key: "server",
        category: "Compute",
    },
    Entry {
        library: "azure",
        key: "storage_blob",
        category: "Storage",
    },
    Entry {
        library: "azure",
        key: "storage_queue",
        category: "Storage",
    },
    Entry {
        library: "azure",
        key: "storage",
        category: "Storage",
    },
    Entry {
        library: "azure",
        key: "cdn",
        category: "Networking",
    },
    Entry {
        library: "azure",
        key: "express_route",
        category: "Networking",
    },
    Entry {
        library: "azure",
        key: "load_balancer",
        category: "Networking",
    },
    Entry {
        library: "azure",
        key: "traffic_manager",
        category: "Networking",
    },
    Entry {
        library: "azure",
        key: "virtual_network",
        category: "Networking",
    },
    Entry {
        library: "azure",
        key: "notification_hub",
        category: "Integration",
    },
    Entry {
        library: "azure",
        key: "service_bus",
        category: "Integration",
    },
    // -- Clients / actors (vendor-neutral) --
    Entry {
        library: "client",
        key: "browser",
        category: "Actors",
    },
    Entry {
        library: "client",
        key: "mobile",
        category: "Actors",
    },
    Entry {
        library: "client",
        key: "person",
        category: "Actors",
    },
    Entry {
        library: "client",
        key: "external_system",
        category: "Actors",
    },
    // -- Generic infrastructure (vendor-neutral) --
    Entry {
        library: "generic",
        key: "cloud",
        category: "Infrastructure",
    },
    Entry {
        library: "generic",
        key: "database",
        category: "Infrastructure",
    },
    Entry {
        library: "generic",
        key: "queue",
        category: "Infrastructure",
    },
    Entry {
        library: "generic",
        key: "document",
        category: "Infrastructure",
    },
    // -- GCP --
    Entry {
        library: "gcp",
        key: "app_engine",
        category: "Compute",
    },
    Entry {
        library: "gcp",
        key: "cloud_functions",
        category: "Compute",
    },
    Entry {
        library: "gcp",
        key: "compute_engine",
        category: "Compute",
    },
    Entry {
        library: "gcp",
        key: "gke",
        category: "Compute",
    },
    Entry {
        library: "gcp",
        key: "cloud_storage",
        category: "Storage",
    },
    Entry {
        library: "gcp",
        key: "bigquery",
        category: "Big Data",
    },
    Entry {
        library: "gcp",
        key: "bigtable",
        category: "Big Data",
    },
    Entry {
        library: "gcp",
        key: "pubsub",
        category: "Big Data",
    },
    Entry {
        library: "gcp",
        key: "cloud_datastore",
        category: "Database",
    },
    Entry {
        library: "gcp",
        key: "cloud_sql",
        category: "Database",
    },
    Entry {
        library: "gcp",
        key: "cloud_cdn",
        category: "Networking",
    },
    Entry {
        library: "gcp",
        key: "cloud_dns",
        category: "Networking",
    },
    Entry {
        library: "gcp",
        key: "cloud_load_balancing",
        category: "Networking",
    },
    Entry {
        library: "gcp",
        key: "iam",
        category: "Identity & Security",
    },
    Entry {
        library: "gcp",
        key: "logging",
        category: "Management Tools",
    },
    // -- Kubernetes --
    Entry {
        library: "k8s",
        key: "pod",
        category: "Workloads",
    },
    Entry {
        library: "k8s",
        key: "deployment",
        category: "Workloads",
    },
    Entry {
        library: "k8s",
        key: "replica_set",
        category: "Workloads",
    },
    Entry {
        library: "k8s",
        key: "service",
        category: "Networking",
    },
    Entry {
        library: "k8s",
        key: "ingress",
        category: "Networking",
    },
    Entry {
        library: "k8s",
        key: "config_map",
        category: "Configuration",
    },
    Entry {
        library: "k8s",
        key: "secret",
        category: "Configuration",
    },
    Entry {
        library: "k8s",
        key: "namespace",
        category: "Cluster",
    },
    Entry {
        library: "k8s",
        key: "node",
        category: "Cluster",
    },
    Entry {
        library: "k8s",
        key: "persistent_volume",
        category: "Storage",
    },
];

/// Filter [`ENTRIES`] by library prefix. Pass `"all"` (or any unrecognised
/// value) to get every entry back.
#[must_use]
pub fn for_library(library: &str) -> Vec<Entry> {
    if library == "all" {
        return ENTRIES.to_vec();
    }
    ENTRIES
        .iter()
        .copied()
        .filter(|e| e.library == library)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn well_known_entries_are_present() {
        let qualified: Vec<String> = ENTRIES.iter().map(Entry::qualified_kind).collect();
        for needle in [
            "aws.lambda",
            "aws.api_gateway",
            "azure.sql_database",
            "azure.entra_id",
            "gcp.cloud_functions",
            "k8s.pod",
            "client.browser",
            "client.person",
            "generic.cloud",
            "generic.database",
        ] {
            assert!(
                qualified.iter().any(|k| k == needle),
                "missing {needle} in catalogue: {qualified:?}",
            );
        }
    }

    #[test]
    fn entries_have_unique_qualified_kinds() {
        let mut seen = std::collections::HashSet::new();
        for e in ENTRIES {
            let q = e.qualified_kind();
            assert!(seen.insert(q.clone()), "duplicate catalogue entry: {q}");
        }
    }

    #[test]
    fn for_library_filters() {
        let aws = for_library("aws");
        assert!(aws.iter().all(|e| e.library == "aws"));
        assert!(aws.iter().any(|e| e.key == "lambda"));
        let all = for_library("all");
        assert_eq!(all.len(), ENTRIES.len());
    }
}
