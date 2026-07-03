//! Hybrid identity & on-prem integration: a user reaches an on-premises app
//! server through Entra ID (with an MFA challenge on the mobile path),
//! after which the server talks to its own database, storage account and
//! job queue, plus a partner API reached straight off the identity hop.
//!
//! This is the showcase for the broader curated catalogue added in issue
//! #29: vendor-neutral client/actor shapes (`client::*`), vendor-neutral
//! infrastructure shapes (`generic::*`), and the new Azure hybrid-identity
//! entries (`azure::entra_id`, `azure::multi_factor_authentication`,
//! `azure::server`, `azure::storage`) — twelve new kinds in one diagram.

use drawio_author::{Diagram, azure, client, generic};

fn main() -> std::io::Result<()> {
    let mut d = Diagram::new("Hybrid identity & on-prem integration");

    let person = d.add_node(client::person("person", "End user").at(180.0, 40.0));
    let browser = d.add_node(client::browser("browser", "Browser").at(300.0, 40.0));
    let mobile = d.add_node(client::mobile("mobile", "Mobile app").at(560.0, 40.0));

    let cloud = d.add_node(generic::cloud("cloud", "Internet").at(289.0, 220.0));
    let mfa = d.add_node(azure::multi_factor_authentication("mfa", "MFA").at(574.0, 220.0));

    let entra_id = d.add_node(azure::entra_id("entra-id", "Entra ID").at(444.0, 420.0));
    let partner = d.add_node(client::external_system("partner", "Partner API").at(820.0, 420.0));

    let server = d.add_node(azure::server("server", "On-prem server").at(444.0, 600.0));

    let database = d.add_node(generic::database("db", "Database").at(140.0, 780.0));
    let storage = d.add_node(azure::storage("storage", "Storage account").at(444.0, 780.0));
    let queue = d.add_node(generic::queue("queue", "Job queue").at(760.0, 780.0));

    let document = d.add_node(generic::document("doc", "Report").at(760.0, 960.0));

    d.connect(&person, &browser);
    d.connect(&person, &mobile);
    d.connect(&browser, &cloud);
    d.connect(&mobile, &cloud);
    d.connect(&mobile, &mfa);
    d.connect(&cloud, &entra_id);
    d.connect(&mfa, &entra_id);
    d.connect(&entra_id, &server);
    d.connect(&entra_id, &partner);
    d.connect(&server, &database);
    d.connect(&server, &storage);
    d.connect(&server, &queue);
    d.connect(&queue, &document);

    drawio_headless_examples::write_artifacts("hybrid-identity", &d)
}
