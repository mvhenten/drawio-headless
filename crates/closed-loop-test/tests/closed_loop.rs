//! Closed-loop integration test:
//!
//! 1. Author a diagram programmatically (API Gateway + Lambda + edge).
//! 2. Serialise to `.drawio` XML.
//! 3. Render to SVG.
//! 4. Rasterise to PNG via `resvg` / `tiny-skia`.
//! 5. Assert the PNG is well-formed, has reasonable dimensions, and contains
//!    a non-trivial number of non-background pixels.
//!
//! Artifacts are written to `target/test-output/` for human inspection.

use std::fs;
use std::path::{Path, PathBuf};

use drawio_author::{Diagram, GroupKind, GroupOpts, aws};
use resvg::usvg;

fn out_dir() -> PathBuf {
    // CARGO_TARGET_TMPDIR isn't always defined for integration tests; instead
    // walk up to the workspace `target/` directory deterministically.
    // Manifest dir for this test crate is .../crates/closed-loop-test.
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

#[test]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn closed_loop_author_render_rasterise() {
    let out = out_dir();

    // 1. Author the diagram.
    let mut diagram = Diagram::new("ClosedLoop");
    // Label deliberately contains XML specials to exercise escaping.
    let api = diagram.add_node(aws::api_gateway("api", "API & \"Auth\"").at(80.0, 80.0));
    let lam = diagram.add_node(aws::lambda("lam", "Lambda").at(320.0, 80.0));
    diagram.connect(&api, &lam);

    // 2. Serialise.
    let xml = diagram.to_xml();
    let drawio_path = out.join("api-lambda.drawio");
    fs::write(&drawio_path, &xml).expect("write .drawio");
    assert!(xml.contains("API &amp; &quot;Auth&quot;"), "XML escaping");
    assert!(xml.contains("resIcon=mxgraph.aws4.api_gateway"));
    assert!(xml.contains("resIcon=mxgraph.aws4.lambda"));

    // 3. Render to SVG.
    let svg = drawio_render::render(&xml).expect("render");
    let svg_path = out.join("api-lambda.svg");
    fs::write(&svg_path, &svg).expect("write svg");
    assert!(svg.starts_with("<svg"), "svg prefix");
    // The Lambda glyph should have produced at least one <path>:
    assert!(svg.contains("<path"), "stencil glyph path");

    // 4. Rasterise to PNG. Load system fonts and map the "sans-serif"
    //    generic family to the first sans-serif face we find; without this
    //    resvg silently drops <text> elements (the SVG output of
    //    `drawio-render` still contains them — browsers render them fine).
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
    let tree = usvg::Tree::from_str(&svg, &opts).expect("usvg parse");
    let size = tree.size().to_int_size();
    // Inflate a little so labels fit nicely. f64 then back to u32 ceil so we
    // don't lose precision through f32 / u32 round-trips.
    let scale: f64 = 2.0;
    let pix_w = (f64::from(size.width()) * scale).ceil() as u32;
    let pix_h = (f64::from(size.height()) * scale).ceil() as u32;
    let mut pixmap = tiny_skia::Pixmap::new(pix_w, pix_h).expect("pixmap");
    pixmap.fill(tiny_skia::Color::WHITE);
    let transform = tiny_skia::Transform::from_scale(scale as f32, scale as f32);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    let png_path = out.join("api-lambda.png");
    pixmap.save_png(&png_path).expect("save png");

    // 5. Assertions on the PNG.
    let png_bytes = fs::read(&png_path).expect("read png back");
    assert!(png_bytes.len() > 100, "png file size sanity");

    // Decode and validate dimensions + pixel content.
    let decoder = png::Decoder::new(std::io::Cursor::new(&png_bytes));
    let mut reader = decoder.read_info().expect("png decode");
    let info = reader.info().clone();
    assert!(
        info.width >= 200 && info.height >= 100,
        "png too small: {}x{}",
        info.width,
        info.height
    );
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut buf).expect("read frame");
    let data = &buf[..frame.buffer_size()];
    let channels = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        other => panic!("unexpected png color type: {other:?}"),
    };
    let (non_bg, orange_in_lambda_tile) = count_pixels(data, &info, channels);

    println!(
        "PNG {}x{}: non-background pixels = {non_bg}, orange-in-lambda-tile = {orange_in_lambda_tile}",
        info.width, info.height,
    );

    assert!(
        non_bg > 500,
        "expected substantial non-background pixels, got {non_bg}"
    );
    assert!(
        orange_in_lambda_tile > 100,
        "expected AWS-orange pixels inside the Lambda tile, got {orange_in_lambda_tile}"
    );
}

#[test]
fn group_boundary_renders_as_dashed_rect() {
    let out = out_dir();
    let mut diagram = Diagram::new("GroupBoundary");
    diagram.add_group(GroupOpts::new(
        "acct-a",
        "Account A",
        40.0,
        40.0,
        320.0,
        200.0,
        GroupKind::AwsAccount,
    ));
    diagram.add_node(aws::lambda("lam", "Lambda").at(100.0, 120.0));
    diagram.add_node(aws::s3("s3", "Bucket").at(220.0, 120.0));
    let xml = diagram.to_xml();
    fs::write(out.join("group-boundary.drawio"), &xml).expect("write");
    assert!(xml.contains("shape=mxgraph.aws4.group"));
    let svg = drawio_render::render(&xml).expect("render");
    fs::write(out.join("group-boundary.svg"), &svg).expect("write svg");
    assert!(svg.contains("stroke-dasharray"), "dashed boundary: {svg}");
    assert!(svg.contains("Account A"), "group label: {svg}");
}

/// Walk the raw RGB(A) image data. Return (non-background pixel count,
/// AWS-orange pixel count inside the Lambda tile bounding box).
fn count_pixels(data: &[u8], info: &png::Info<'_>, channels: usize) -> (u32, u32) {
    // The Lambda cell sits at (320..398, 80..158) in SVG user units. The
    // viewBox origin is offset by the renderer's margin (-24, -24). At our 2x
    // scale, the Lambda tile maps to roughly:
    //   px_x = (320 - 56) * 2 = 528  ..  (398 - 56) * 2 = 684
    //   px_y = (80  - 56) * 2 = 48   ..  (158 - 56) * 2 = 204
    let pix_w = info.width as usize;
    let mut non_bg = 0u32;
    let mut orange = 0u32;
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
            // Background is white (255,255,255).
            if !(red > 240 && green > 240 && blue > 240) {
                non_bg += 1;
            }
            // Inside the Lambda tile bounds, count "AWS orange-ish" pixels.
            // The exact hue is #ED7100 (237, 113, 0).
            if (528..=684).contains(&col)
                && (48..=204).contains(&row)
                && red > 200
                && (60..=180).contains(&green)
                && blue < 60
            {
                orange += 1;
            }
        }
    }
    (non_bg, orange)
}
