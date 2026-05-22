//! Pet shop e-commerce architecture.
//!
//! `CloudFront` fronts an API Gateway plus the static S3 image bucket. The
//! gateway invokes a Lambda function which writes orders to `DynamoDB` and
//! enqueues async work onto SQS.

use drawio_author::{Diagram, aws};

fn main() -> std::io::Result<()> {
    let mut d = Diagram::new("Pet shop e-commerce");

    let cdn = d.add_node(aws::cloudfront("cdn", "CloudFront").at(80.0, 120.0));
    let images = d.add_node(aws::s3("images", "S3 (images)").at(80.0, 360.0));
    let api = d.add_node(aws::api_gateway("api", "API Gateway").at(320.0, 120.0));
    let fn_orders = d.add_node(aws::lambda("fn", "Order Lambda").at(560.0, 120.0));
    let ddb = d.add_node(aws::dynamodb("ddb", "DynamoDB").at(800.0, 120.0));
    let queue = d.add_node(aws::sqs("queue", "Order queue").at(560.0, 360.0));

    d.connect(&cdn, &api);
    d.connect(&cdn, &images);
    d.connect(&api, &fn_orders);
    d.connect(&fn_orders, &ddb);
    d.connect(&fn_orders, &queue);

    drawio_headless_examples::write_artifacts("petshop", &d)
}
