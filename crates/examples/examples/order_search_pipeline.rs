//! Order search indexing pipeline: an SQS queue feeds an ingest Lambda that
//! writes to `DynamoDB`, and a second Lambda indexes those orders into
//! `OpenSearch`.
//!
//! A single straight lane through five distinct AWS service tiles —
//! `sqs`, `lambda` (twice), `dynamodb`, `opensearch` — with every edge
//! already colinear, so no exit/entry overrides are needed at all.

use drawio_author::{Diagram, aws};

fn main() -> std::io::Result<()> {
    let mut d = Diagram::new("Order search indexing pipeline");

    let queue = d.add_node(aws::sqs("queue", "Order events").at(80.0, 120.0));
    let ingest = d.add_node(aws::lambda("ingest", "Ingest Lambda").at(320.0, 120.0));
    let orders = d.add_node(aws::dynamodb("orders", "Orders table").at(560.0, 120.0));
    let indexer = d.add_node(aws::lambda("indexer", "Indexer Lambda").at(800.0, 120.0));
    let search = d.add_node(aws::opensearch("search", "Order search index").at(1040.0, 120.0));

    d.connect(&queue, &ingest);
    d.connect(&ingest, &orders);
    d.connect(&orders, &indexer);
    d.connect(&indexer, &search);

    drawio_headless_examples::write_artifacts("order-search-pipeline", &d)
}
