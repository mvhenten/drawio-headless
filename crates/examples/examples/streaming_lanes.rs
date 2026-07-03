//! Streaming data pipeline with two independent lanes fed by one source:
//! a real-time lane (Lambda into `ElastiCache` for a live dashboard) and a
//! batch lane (S3 into Athena for ad-hoc queries).
//!
//! No edge labels distinguish the lanes — each lane is its own vertical
//! column of colinear nodes, per the layout rule that parallel paths read
//! by position, not by labelling the arrow.

use drawio_author::{Diagram, aws, client};

fn main() -> std::io::Result<()> {
    let mut d = Diagram::new("Streaming data pipeline");

    let source = d.add_node(client::external_system("source", "Upstream feed").at(440.0, 40.0));
    let stream = d.add_node(aws::kinesis("stream", "Event stream").at(440.0, 220.0));

    let processor = d.add_node(aws::lambda("processor", "Stream processor").at(200.0, 420.0));
    let cache = d.add_node(aws::elasticache("cache", "Live dashboard cache").at(200.0, 600.0));

    let raw = d.add_node(aws::s3("raw", "Raw data lake").at(680.0, 420.0));
    let athena = d.add_node(aws::athena("athena", "Ad-hoc queries").at(680.0, 600.0));

    d.connect(&source, &stream);
    // Fan out from the stream's bottom quarter-points (never a corner —
    // issue #49) so the two lanes' departures don't share an endpoint.
    d.connect(&stream, &processor)
        .exit(0.25, 1.0)
        .entry(0.5, 0.0);
    d.connect(&stream, &raw).exit(0.75, 1.0).entry(0.5, 0.0);
    d.connect(&processor, &cache);
    d.connect(&raw, &athena);

    drawio_headless_examples::write_artifacts("streaming-lanes", &d)
}
