//! Event-driven image processing: S3 upload triggers a Lambda that resizes
//! the image, writes metadata to `DynamoDB` and publishes a completion event
//! to SNS subscribers.
//!
//! Demonstrates per-edge `exit/entry` overrides: the two fan-out edges from
//! the Resize Lambda are pinned to separate bottom corners so the routes
//! don't share an endpoint and visibly stay parallel.

use drawio_author::{Diagram, aws};

fn main() -> std::io::Result<()> {
    let mut d = Diagram::new("Event-driven image processing");

    let uploads = d.add_node(aws::s3("uploads", "S3 (uploads)").at(320.0, 80.0));
    let resize = d.add_node(aws::lambda("resize", "Resize Lambda").at(320.0, 280.0));
    let metadata = d.add_node(aws::dynamodb("metadata", "Image metadata").at(140.0, 480.0));
    let topic = d.add_node(aws::sns("topic", "Notifications").at(500.0, 480.0));

    // S3 trigger straight down into the Lambda's top edge.
    d.connect(&uploads, &resize);
    // Fan out from the Lambda's bottom corners into the two siblings'
    // top-mid attachments. Without these explicit overrides, the picker
    // would land both source endpoints on the Lambda's bottom-mid or
    // bottom-left, depending on the constraint set.
    d.connect(&resize, &metadata).exit(0.0, 1.0).entry(0.5, 0.0);
    d.connect(&resize, &topic).exit(1.0, 1.0).entry(0.5, 0.0);

    drawio_headless_examples::write_artifacts("event-driven", &d)
}
