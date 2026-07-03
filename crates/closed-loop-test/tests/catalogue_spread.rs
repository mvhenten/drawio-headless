//! Closed-loop coverage for the broader curated catalogue added by issue
//! #29: vendor-neutral client/actor shapes, vendor-neutral infrastructure
//! shapes, and the new Azure hybrid-identity entries.
//!
//! Each new kind is authored, rendered to SVG, and rasterised to PNG. The
//! tests assert:
//! - the SVG never emits the renderer's bare-fallback marker
//!   (`stroke="#999"`, the "Plain rect fallback for unknown shapes" branch
//!   in `drawio-render`) — proving every kind resolves a real glyph instead
//!   of degrading to a gray box;
//! - each tile contributes at least one stencil `<path>`;
//! - the rasterised PNG has substantial non-background coverage (a bare
//!   outline or a near-empty glyph would leave the tile mostly white).
//!
//! The Azure entries in particular exercise the `<arc>` stencil DSL command
//! added alongside this catalogue (see `crates/render/src/stencil.rs`,
//! issue #7) — `active_directory`/`entra_id`, `multi_factor_authentication`
//! and the legacy Azure "Cloud" shape all build their silhouette from
//! `<arc>` commands that previously rendered as near-empty outlines.

use std::fs;
use std::path::{Path, PathBuf};

use drawio_author::{Diagram, Node, azure, client, generic};
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

/// Lay `nodes` out left-to-right with a fixed pitch, chain-connect them, then
/// render + rasterise. Returns the SVG and the non-background pixel count.
fn chain(diagram_name: &str, out_basename: &str, nodes: Vec<Node>) -> (String, u32) {
    let out = out_dir();
    let mut diagram = Diagram::new(diagram_name);
    let mut refs = Vec::with_capacity(nodes.len());
    let mut x = 80.0;
    for node in nodes {
        refs.push(diagram.add_node(node.at(x, 80.0)));
        x += 160.0;
    }
    for pair in refs.windows(2) {
        diagram.connect(&pair[0], &pair[1]);
    }
    let xml = diagram.to_xml();
    fs::write(out.join(format!("{out_basename}.drawio")), &xml).expect("write drawio");
    let svg = drawio_render::render(&xml).expect("render");
    fs::write(out.join(format!("{out_basename}.svg")), &svg).expect("write svg");
    let non_bg = rasterise_and_count_non_bg(&svg, &out.join(format!("{out_basename}.png")));
    (svg, non_bg)
}

/// Shared assertion: no kind under test degraded to the renderer's bare
/// fallback rect, and every tile contributed real stencil geometry.
fn assert_no_fallback_and_solid(svg: &str, non_bg: u32, min_paths: usize, min_non_bg: u32) {
    assert!(
        !svg.contains("stroke=\"#999\""),
        "a shape fell back to the bare gray rect (stroke=#999): {svg}",
    );
    let path_count = svg.matches("<path").count();
    assert!(
        path_count >= min_paths,
        "expected at least {min_paths} stencil paths, got {path_count} in: {svg}",
    );
    assert!(
        non_bg >= min_non_bg,
        "rendered too sparse: {non_bg} non-bg pixels (min {min_non_bg})",
    );
}

#[test]
fn client_actor_catalogue_renders_solid() {
    let (svg, non_bg) = chain(
        "ClientActors",
        "client-actors-sample",
        vec![
            client::person("user", "End user"),
            client::browser("browser", "Browser"),
            client::mobile("mobile", "Mobile app"),
            client::external_system("ext", "Partner API"),
        ],
    );
    assert_no_fallback_and_solid(&svg, non_bg, 4, 2_000);
}

#[test]
fn generic_infrastructure_catalogue_renders_solid() {
    let (svg, non_bg) = chain(
        "GenericInfrastructure",
        "generic-infra-sample",
        vec![
            generic::cloud("cloud", "Internet"),
            generic::database("db", "Database"),
            generic::queue("q", "Job queue"),
            generic::document("doc", "Report"),
        ],
    );
    assert_no_fallback_and_solid(&svg, non_bg, 4, 2_000);
}

#[test]
fn azure_hybrid_identity_catalogue_renders_solid() {
    // These four all lean on `<arc>` for their silhouette (see issue #7) —
    // this is the regression test for that DSL command.
    let (svg, non_bg) = chain(
        "AzureHybridIdentity",
        "azure-hybrid-sample",
        vec![
            azure::entra_id("eid", "Entra ID"),
            azure::multi_factor_authentication("mfa", "MFA"),
            azure::server("srv", "On-prem server"),
            azure::storage("st", "Storage account"),
        ],
    );
    assert_no_fallback_and_solid(&svg, non_bg, 4, 2_000);
}
