//! Classic three-tier web app: browser and mobile clients hit a load
//! balancer fronting an app tier, which reads/writes a vendor-neutral
//! database and pushes async work onto a vendor-neutral queue.
//!
//! Mixes the `client::*` and `generic::*` catalogues with AWS tiles for the
//! load balancer and app tier, demonstrating fan-in (two clients into one
//! balancer) and fan-out (one app tier into two backing services) in a
//! single diagram.

use drawio_author::{Diagram, aws, client, generic};

fn main() -> std::io::Result<()> {
    let mut d = Diagram::new("Three-tier web app");

    let browser = d.add_node(client::browser("browser", "Browser").at(200.0, 40.0));
    let mobile = d.add_node(client::mobile("mobile", "Mobile app").at(440.0, 40.0));

    let lb = d.add_node(aws::elastic_load_balancing("lb", "Load balancer").at(320.0, 240.0));
    let app = d.add_node(aws::ecs("app", "App tier").at(320.0, 440.0));

    let database = d.add_node(generic::database("db", "Database").at(140.0, 640.0));
    let queue = d.add_node(generic::queue("queue", "Job queue").at(560.0, 640.0));

    d.connect(&browser, &lb).exit(0.5, 1.0).entry(0.0, 0.5);
    d.connect(&mobile, &lb).exit(0.5, 1.0).entry(1.0, 0.5);
    d.connect(&lb, &app);
    d.connect(&app, &database).exit(0.0, 1.0).entry(0.5, 0.0);
    d.connect(&app, &queue).exit(1.0, 1.0).entry(0.5, 0.0);

    drawio_headless_examples::write_artifacts("three-tier-web", &d)
}
