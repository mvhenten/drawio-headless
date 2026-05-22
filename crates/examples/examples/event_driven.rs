//! Event-driven image processing: S3 upload triggers a Lambda that resizes
//! the image, writes metadata to `DynamoDB` and publishes a completion event
//! to SNS subscribers.

use drawio_author::{Diagram, aws};

fn main() -> std::io::Result<()> {
    let mut d = Diagram::new("Event-driven image processing");

    let uploads = d.add_node(aws::s3("uploads", "S3 (uploads)").at(320.0, 80.0));
    let resize = d.add_node(aws::lambda("resize", "Resize Lambda").at(320.0, 280.0));
    let metadata = d.add_node(aws::dynamodb("metadata", "Image metadata").at(140.0, 480.0));
    let topic = d.add_node(aws::sns("topic", "Notifications").at(500.0, 480.0));

    d.connect(&uploads, &resize);
    d.connect(&resize, &metadata);
    d.connect(&resize, &topic);

    drawio_headless_examples::write_artifacts("event-driven", &d)
}
