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
pub(crate) fn res_icon_style(fill: &str, res_icon: &str) -> String {
    format!(
        "sketch=0;points=[[0,0,0],[0.25,0,0],[0.5,0,0],[0.75,0,0],[1,0,0],\
         [0,1,0],[0.25,1,0],[0.5,1,0],[0.75,1,0],[1,1,0],\
         [0,0.25,0],[0,0.5,0],[0,0.75,0],[1,0.25,0],[1,0.5,0],[1,0.75,0]];\
         outlineConnect=0;fontColor=#232F3E;fillColor={fill};\
         strokeColor=#ffffff;dashed=0;verticalLabelPosition=bottom;\
         verticalAlign=top;align=center;html=0;fontSize=12;aspect=fixed;\
         shape=mxgraph.aws4.resourceIcon;resIcon={res_icon};"
    )
}

pub(crate) fn aws_node(id: &str, label: &str, fill: &str, res_icon: &str) -> Node {
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

/// SQS tile (Application Integration).
pub fn sqs(id: &str, label: &str) -> Node {
    aws_node(id, label, "#E7157B", "mxgraph.aws4.sqs")
}

/// SNS tile (Application Integration).
pub fn sns(id: &str, label: &str) -> Node {
    aws_node(id, label, "#E7157B", "mxgraph.aws4.sns")
}

/// `CloudFront` tile (Networking & Content Delivery).
pub fn cloudfront(id: &str, label: &str) -> Node {
    aws_node(id, label, "#8C4FFF", "mxgraph.aws4.cloudfront")
}

/// MSK tile (Analytics) — Amazon Managed Streaming for Apache Kafka.
pub fn msk(id: &str, label: &str) -> Node {
    aws_node(
        id,
        label,
        "#8C4FFF",
        "mxgraph.aws4.managed_streaming_for_kafka",
    )
}

/// IAM tile (Security, Identity & Compliance).
pub fn iam(id: &str, label: &str) -> Node {
    aws_node(
        id,
        label,
        "#DD344C",
        "mxgraph.aws4.identity_and_access_management",
    )
}

/// VPC tile (Networking & Content Delivery).
pub fn vpc(id: &str, label: &str) -> Node {
    aws_node(id, label, "#8C4FFF", "mxgraph.aws4.virtual_private_cloud")
}

/// `EventBridge` tile (Application Integration).
pub fn eventbridge(id: &str, label: &str) -> Node {
    aws_node(id, label, "#E7157B", "mxgraph.aws4.eventbridge")
}

/// Step Functions tile (Application Integration).
pub fn step_functions(id: &str, label: &str) -> Node {
    aws_node(id, label, "#E7157B", "mxgraph.aws4.step_functions")
}

/// `AppSync` tile (Application Integration).
pub fn appsync(id: &str, label: &str) -> Node {
    aws_node(id, label, "#E7157B", "mxgraph.aws4.appsync")
}

/// ECS tile (Compute) — Amazon Elastic Container Service.
pub fn ecs(id: &str, label: &str) -> Node {
    aws_node(id, label, "#ED7100", "mxgraph.aws4.ecs")
}

/// EKS tile (Compute) — Amazon Elastic Kubernetes Service.
pub fn eks(id: &str, label: &str) -> Node {
    aws_node(id, label, "#ED7100", "mxgraph.aws4.eks")
}

/// Fargate tile (Compute).
pub fn fargate(id: &str, label: &str) -> Node {
    aws_node(id, label, "#ED7100", "mxgraph.aws4.fargate")
}

/// App Runner tile (Compute).
pub fn app_runner(id: &str, label: &str) -> Node {
    aws_node(id, label, "#ED7100", "mxgraph.aws4.app_runner")
}

/// AWS Batch tile (Compute).
pub fn batch(id: &str, label: &str) -> Node {
    aws_node(id, label, "#ED7100", "mxgraph.aws4.batch")
}

/// RDS tile (Database) — Amazon Relational Database Service.
pub fn rds(id: &str, label: &str) -> Node {
    aws_node(id, label, "#C925D1", "mxgraph.aws4.rds")
}

/// `ElastiCache` tile (Database).
pub fn elasticache(id: &str, label: &str) -> Node {
    aws_node(id, label, "#C925D1", "mxgraph.aws4.elasticache")
}

/// EFS tile (Storage) — Amazon Elastic File System.
///
/// The catalogue exposes this as `efs` but the underlying stencil is named
/// `elastic file system` (no bare `efs` stencil exists at the top level —
/// only `efs standard` / `efs infrequentaccess` variants).
pub fn efs(id: &str, label: &str) -> Node {
    aws_node(id, label, "#7AA116", "mxgraph.aws4.elastic_file_system")
}

/// Route 53 tile (Networking & Content Delivery).
pub fn route_53(id: &str, label: &str) -> Node {
    aws_node(id, label, "#8C4FFF", "mxgraph.aws4.route_53")
}

/// Elastic Load Balancing tile (Networking & Content Delivery).
pub fn elastic_load_balancing(id: &str, label: &str) -> Node {
    aws_node(id, label, "#8C4FFF", "mxgraph.aws4.elastic_load_balancing")
}

/// Cognito tile (Security, Identity & Compliance).
pub fn cognito(id: &str, label: &str) -> Node {
    aws_node(id, label, "#DD344C", "mxgraph.aws4.cognito")
}

/// Secrets Manager tile (Security, Identity & Compliance).
pub fn secrets_manager(id: &str, label: &str) -> Node {
    aws_node(id, label, "#DD344C", "mxgraph.aws4.secrets_manager")
}

/// KMS tile (Security, Identity & Compliance) — AWS Key Management Service.
///
/// The stencil is registered under the full product name; no `kms`
/// short-form exists in `aws4.xml`.
pub fn kms(id: &str, label: &str) -> Node {
    aws_node(id, label, "#DD344C", "mxgraph.aws4.key_management_service")
}

/// Kinesis tile (Analytics).
pub fn kinesis(id: &str, label: &str) -> Node {
    aws_node(id, label, "#8C4FFF", "mxgraph.aws4.kinesis")
}

/// Athena tile (Analytics).
pub fn athena(id: &str, label: &str) -> Node {
    aws_node(id, label, "#8C4FFF", "mxgraph.aws4.athena")
}

/// `OpenSearch` Service tile (Analytics).
///
/// drawio labels this service group "`OpenSearch` Service" but still keys the
/// tile on the pre-rename `elasticsearch_service` stencil; use that name so
/// the glyph resolves against the vendored `aws4.xml`.
pub fn opensearch(id: &str, label: &str) -> Node {
    aws_node(id, label, "#8C4FFF", "mxgraph.aws4.elasticsearch_service")
}

/// `CloudWatch` tile (Management & Governance).
pub fn cloudwatch(id: &str, label: &str) -> Node {
    aws_node(id, label, "#E7157B", "mxgraph.aws4.cloudwatch")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_aws_style(node: &Node, res_icon: &str, fill: &str) {
        assert!(
            node.style.contains(&format!("resIcon={res_icon}")),
            "missing resIcon={res_icon} in style: {}",
            node.style,
        );
        assert!(
            node.style.contains(&format!("fillColor={fill}")),
            "missing fillColor={fill} in style: {}",
            node.style,
        );
        assert!(node.style.contains("shape=mxgraph.aws4.resourceIcon"));
        // Canonical 16-point AWS resource-icon constraint set: 5 along the
        // top edge, 5 along the bottom, 3 interior on each vertical side.
        assert!(
            node.style.contains(
                "points=[[0,0,0],[0.25,0,0],[0.5,0,0],[0.75,0,0],[1,0,0],\
                 [0,1,0],[0.25,1,0],[0.5,1,0],[0.75,1,0],[1,1,0],\
                 [0,0.25,0],[0,0.5,0],[0,0.75,0],[1,0.25,0],[1,0.5,0],[1,0.75,0]]"
            ),
            "missing 16-point connection set in style: {}",
            node.style,
        );
    }

    #[test]
    fn lambda_factory() {
        assert_aws_style(&lambda("l", "Lambda"), "mxgraph.aws4.lambda", "#ED7100");
    }

    #[test]
    fn sqs_factory() {
        assert_aws_style(&sqs("q", "Orders"), "mxgraph.aws4.sqs", "#E7157B");
    }

    #[test]
    fn sns_factory() {
        assert_aws_style(&sns("t", "Notify"), "mxgraph.aws4.sns", "#E7157B");
    }

    #[test]
    fn cloudfront_factory() {
        assert_aws_style(
            &cloudfront("cf", "CDN"),
            "mxgraph.aws4.cloudfront",
            "#8C4FFF",
        );
    }

    #[test]
    fn msk_factory() {
        assert_aws_style(
            &msk("k", "Kafka"),
            "mxgraph.aws4.managed_streaming_for_kafka",
            "#8C4FFF",
        );
    }

    #[test]
    fn iam_factory() {
        assert_aws_style(
            &iam("r", "Role"),
            "mxgraph.aws4.identity_and_access_management",
            "#DD344C",
        );
    }

    #[test]
    fn vpc_factory() {
        assert_aws_style(
            &vpc("v", "VPC"),
            "mxgraph.aws4.virtual_private_cloud",
            "#8C4FFF",
        );
    }

    #[test]
    fn eventbridge_factory() {
        assert_aws_style(
            &eventbridge("eb", "Events"),
            "mxgraph.aws4.eventbridge",
            "#E7157B",
        );
    }

    #[test]
    fn step_functions_factory() {
        assert_aws_style(
            &step_functions("sf", "Workflow"),
            "mxgraph.aws4.step_functions",
            "#E7157B",
        );
    }

    #[test]
    fn appsync_factory() {
        assert_aws_style(&appsync("as", "GraphQL"), "mxgraph.aws4.appsync", "#E7157B");
    }

    #[test]
    fn ecs_factory() {
        assert_aws_style(&ecs("c", "Containers"), "mxgraph.aws4.ecs", "#ED7100");
    }

    #[test]
    fn eks_factory() {
        assert_aws_style(&eks("k", "Kubernetes"), "mxgraph.aws4.eks", "#ED7100");
    }

    #[test]
    fn fargate_factory() {
        assert_aws_style(&fargate("f", "Fargate"), "mxgraph.aws4.fargate", "#ED7100");
    }

    #[test]
    fn app_runner_factory() {
        assert_aws_style(
            &app_runner("ar", "App Runner"),
            "mxgraph.aws4.app_runner",
            "#ED7100",
        );
    }

    #[test]
    fn batch_factory() {
        assert_aws_style(&batch("b", "Batch"), "mxgraph.aws4.batch", "#ED7100");
    }

    #[test]
    fn rds_factory() {
        assert_aws_style(&rds("db", "Postgres"), "mxgraph.aws4.rds", "#C925D1");
    }

    #[test]
    fn elasticache_factory() {
        assert_aws_style(
            &elasticache("ec", "Cache"),
            "mxgraph.aws4.elasticache",
            "#C925D1",
        );
    }

    #[test]
    fn efs_factory() {
        assert_aws_style(
            &efs("fs", "File system"),
            "mxgraph.aws4.elastic_file_system",
            "#7AA116",
        );
    }

    #[test]
    fn route_53_factory() {
        assert_aws_style(&route_53("r53", "DNS"), "mxgraph.aws4.route_53", "#8C4FFF");
    }

    #[test]
    fn elastic_load_balancing_factory() {
        assert_aws_style(
            &elastic_load_balancing("elb", "ELB"),
            "mxgraph.aws4.elastic_load_balancing",
            "#8C4FFF",
        );
    }

    #[test]
    fn cognito_factory() {
        assert_aws_style(&cognito("cog", "Users"), "mxgraph.aws4.cognito", "#DD344C");
    }

    #[test]
    fn secrets_manager_factory() {
        assert_aws_style(
            &secrets_manager("sm", "Secrets"),
            "mxgraph.aws4.secrets_manager",
            "#DD344C",
        );
    }

    #[test]
    fn kms_factory() {
        assert_aws_style(
            &kms("kms", "Keys"),
            "mxgraph.aws4.key_management_service",
            "#DD344C",
        );
    }

    #[test]
    fn kinesis_factory() {
        assert_aws_style(&kinesis("ki", "Stream"), "mxgraph.aws4.kinesis", "#8C4FFF");
    }

    #[test]
    fn athena_factory() {
        assert_aws_style(&athena("at", "Query"), "mxgraph.aws4.athena", "#8C4FFF");
    }

    #[test]
    fn opensearch_factory() {
        assert_aws_style(
            &opensearch("os", "Search"),
            "mxgraph.aws4.elasticsearch_service",
            "#8C4FFF",
        );
    }

    #[test]
    fn cloudwatch_factory() {
        assert_aws_style(
            &cloudwatch("cw", "Metrics"),
            "mxgraph.aws4.cloudwatch",
            "#E7157B",
        );
    }
}
