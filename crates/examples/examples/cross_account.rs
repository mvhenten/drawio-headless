//! Cross-account VPC link: Lambda in Account A consumes MSK in Account B
//! via an assumed IAM role.

use drawio_author::{Diagram, GroupKind, GroupOpts, aws};

fn main() -> std::io::Result<()> {
    let mut d = Diagram::new("Cross-account VPC link");

    d.add_group(GroupOpts::new(
        "acct-a",
        "Account A — consumer",
        60.0,
        60.0,
        420.0,
        360.0,
        GroupKind::AwsAccount,
    ));
    d.add_group(GroupOpts::new(
        "acct-b",
        "Account B — streaming",
        540.0,
        60.0,
        460.0,
        360.0,
        GroupKind::AwsAccount,
    ));

    let consumer = d.add_node(aws::lambda("consumer", "Stream consumer").at(220.0, 200.0));
    let role = d.add_node(aws::iam("role", "AssumeRole").at(620.0, 130.0));
    let kafka = d.add_node(aws::msk("kafka", "MSK cluster").at(820.0, 200.0));

    d.connect(&consumer, &role);
    d.connect(&role, &kafka);
    d.connect(&consumer, &kafka);

    drawio_headless_examples::write_artifacts("cross-account", &d)
}
