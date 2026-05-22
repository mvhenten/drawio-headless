//! Integration tests for compressed `<diagram>` payload support.
//!
//! These exercise the public [`drawio_render::render`] entry point end-to-end
//! on real drawio-flavour XML. Unit-level pipeline tests live alongside
//! [`drawio_render::inflate`].

use std::fs;

/// Real drawio template, base64+deflate+url-encoded `<diagram>` body.
/// Vendored from `jgraph/drawio` (Apache-2.0). See `tests/fixtures/SOURCE`.
const COMPRESSED_SAMPLE: &str = include_str!("fixtures/compressed-sample.drawio");

#[test]
fn renders_real_compressed_drawio_file() {
    let svg = drawio_render::render(COMPRESSED_SAMPLE).expect("render compressed file");
    assert!(svg.starts_with("<svg"), "SVG should start with <svg>");
    assert!(svg.contains("<rect"), "expected at least one <rect>");
    assert!(svg.contains("<text"), "expected at least one <text>");
    // Sanity: non-trivial output.
    assert!(svg.len() > 500, "SVG output unexpectedly small: {svg}");
}

#[test]
fn uncompressed_xml_still_renders() {
    // Round-trip the author crate's output through render. The author always
    // emits compressed="false" with plain inline <mxGraphModel>.
    use drawio_author::{Diagram, aws};
    let mut d = Diagram::new("rt");
    let api = d.add_node(aws::api_gateway("api", "API Gateway").at(80.0, 80.0));
    let lam = d.add_node(aws::lambda("lam", "Lambda").at(320.0, 80.0));
    d.connect(&api, &lam);
    let xml = d.to_xml();
    assert!(
        xml.contains("compressed=\"false\""),
        "author should still emit uncompressed XML"
    );
    let svg = drawio_render::render(&xml).expect("render uncompressed");
    assert!(svg.starts_with("<svg"));
    assert!(svg.contains("<rect"));
}

#[test]
fn rendered_compressed_sample_can_be_written_to_disk() {
    // Smoke test: makes the artifact inspectable when running locally.
    // Failing this test only means we couldn't write the artifact; the
    // render itself is asserted elsewhere.
    let svg = drawio_render::render(COMPRESSED_SAMPLE).expect("render");
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    fs::create_dir_all(&dir).ok();
    let path = dir.join("compressed-sample.svg");
    fs::write(&path, &svg).ok();
}
