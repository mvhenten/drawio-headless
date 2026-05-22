//! Closed-loop integration tests for the non-AWS stencil libraries
//! (Azure, GCP, Kubernetes) added by issue #2.
//!
//! Each test authors a tiny diagram (2–3 shapes + an edge), renders to
//! SVG, rasterises to PNG, and asserts the result is non-trivial — i.e.
//! contains both the stencil glyph paths in the SVG and a non-trivial
//! number of non-background pixels in the PNG.
//!
//! Render fidelity caveat: Azure and GCP stencils use mxStencil commands
//! the renderer does not yet implement (`<arc>`, `<save>`, etc., tracked
//! in issue #7). The PNG assertions here therefore use a low threshold —
//! enough to prove "shapes are being looked up and at least partially
//! rendered" without depending on glyph fidelity.

use std::fs;
use std::path::{Path, PathBuf};

use drawio_author::{Diagram, Node, azure, gcp, k8s};
use resvg::usvg;

fn out_dir() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest
        .parent()
        .and_then(Path::parent)
        .unwrap()
        .to_path_buf();
    let dir = workspace_root.join("target").join("test-output");
    fs::create_dir_all(&dir).expect("create target/test-output");
    dir
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn rasterise_and_count_non_bg(svg: &str, png_path: &Path) -> u32 {
    let mut opts = usvg::Options::default();
    {
        let fontdb = opts.fontdb_mut();
        fontdb.load_system_fonts();
        let sans_family: Option<String> = fontdb
            .faces()
            .find(|face| {
                face.families
                    .iter()
                    .any(|(name, _)| name.to_lowercase().contains("sans"))
            })
            .map(|face| face.families[0].0.clone());
        if let Some(name) = sans_family {
            fontdb.set_sans_serif_family(name);
        }
    }
    let tree = usvg::Tree::from_str(svg, &opts).expect("usvg parse");
    let size = tree.size().to_int_size();
    let scale: f64 = 2.0;
    let pix_w = (f64::from(size.width()) * scale).ceil() as u32;
    let pix_h = (f64::from(size.height()) * scale).ceil() as u32;
    let mut pixmap = tiny_skia::Pixmap::new(pix_w, pix_h).expect("pixmap");
    pixmap.fill(tiny_skia::Color::WHITE);
    let transform = tiny_skia::Transform::from_scale(scale as f32, scale as f32);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    pixmap.save_png(png_path).expect("save png");

    let png_bytes = fs::read(png_path).expect("read png");
    let decoder = png::Decoder::new(std::io::Cursor::new(&png_bytes));
    let mut reader = decoder.read_info().expect("png decode");
    let info = reader.info().clone();
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut buf).expect("read frame");
    let data = &buf[..frame.buffer_size()];
    let channels = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        other => panic!("unexpected png color type: {other:?}"),
    };
    let mut non_bg = 0u32;
    let pix_w = info.width as usize;
    for row in 0..info.height as usize {
        for col in 0..pix_w {
            let idx = (row * pix_w + col) * channels;
            let red = data[idx];
            let green = data[idx + 1];
            let blue = data[idx + 2];
            let alpha = if channels == 4 { data[idx + 3] } else { 255 };
            if alpha == 0 {
                continue;
            }
            if !(red > 240 && green > 240 && blue > 240) {
                non_bg += 1;
            }
        }
    }
    non_bg
}

/// Lay three nodes out horizontally, connect first→second→third, then
/// render + rasterise. Returns the SVG and the path to the PNG.
fn three_shape_chain(diagram_name: &str, out_basename: &str, nodes: [Node; 3]) -> (String, u32) {
    let out = out_dir();
    let mut diagram = Diagram::new(diagram_name);
    let [a, b, c] = nodes;
    let a_ref = diagram.add_node(a.at(80.0, 80.0));
    let b_ref = diagram.add_node(b.at(280.0, 80.0));
    let c_ref = diagram.add_node(c.at(480.0, 80.0));
    diagram.connect(&a_ref, &b_ref);
    diagram.connect(&b_ref, &c_ref);
    let xml = diagram.to_xml();
    fs::write(out.join(format!("{out_basename}.drawio")), &xml).expect("write drawio");
    let svg = drawio_render::render(&xml).expect("render");
    fs::write(out.join(format!("{out_basename}.svg")), &svg).expect("write svg");
    let non_bg = rasterise_and_count_non_bg(&svg, &out.join(format!("{out_basename}.png")));
    (svg, non_bg)
}

#[test]
fn azure_three_shape_diagram_renders() {
    let (svg, non_bg) = three_shape_chain(
        "AzureChain",
        "azure-sample",
        [
            azure::website("web", "App Service"),
            azure::sql_database("db", "SQL DB"),
            azure::storage_blob("blob", "Blob"),
        ],
    );
    // The Azure cyan glyph should appear in the rasterised SVG output as
    // a path fill colour.
    assert!(
        svg.contains("fill=\"#00BEF2\""),
        "Azure glyph fill missing from svg: {svg}",
    );
    // At least one `<path>` must have made it out of the stencil DSL
    // (Azure stencils use `<arc>` heavily, so the renderer skips many
    // commands — but enough simple line/move/close sequences survive to
    // produce visible geometry).
    assert!(
        svg.contains("<path"),
        "no Azure stencil path emitted: {svg}"
    );
    assert!(
        non_bg > 100,
        "Azure rasterised render too sparse: {non_bg} non-bg pixels"
    );
}

#[test]
fn gcp_three_shape_diagram_renders() {
    let (svg, non_bg) = three_shape_chain(
        "GcpChain",
        "gcp-sample",
        [
            gcp::compute_engine("vm", "Compute Engine"),
            gcp::bigquery("bq", "BigQuery"),
            gcp::cloud_storage("gcs", "Cloud Storage"),
        ],
    );
    // The GCP blue glyph colour should appear in the rasterised output.
    assert!(
        svg.contains("fill=\"#4285F4\""),
        "GCP glyph fill missing from svg: {svg}",
    );
    assert!(svg.contains("<path"), "no GCP stencil path emitted: {svg}");
    assert!(
        non_bg > 100,
        "GCP rasterised render too sparse: {non_bg} non-bg pixels"
    );
}

#[test]
fn k8s_three_shape_diagram_renders() {
    let (svg, non_bg) = three_shape_chain(
        "K8sChain",
        "k8s-sample",
        [
            k8s::pod("p", "frontend-pod"),
            k8s::service("s", "frontend-svc"),
            k8s::deployment("d", "frontend"),
        ],
    );
    // Blue K8s tile fill appears in the rasterised output.
    assert!(
        svg.contains("fill=\"#2875E2\""),
        "K8s tile fill missing from svg: {svg}",
    );
    // White glyph stencil should also be present.
    assert!(
        svg.contains("fill=\"#ffffff\""),
        "K8s glyph fill missing from svg: {svg}",
    );
    // Each tile is filled, so we expect a healthy non-background count
    // even before the glyph paths land — the tile rectangles alone cover
    // ~3 * 50 * 48 = 7200 user units; scaled 2x ≈ 28k px.
    assert!(
        non_bg > 1_000,
        "K8s rasterised render too sparse: {non_bg} non-bg pixels"
    );
}
