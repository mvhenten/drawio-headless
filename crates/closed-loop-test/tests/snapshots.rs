//! Pixel-diff snapshot tests for visual regression.
//!
//! Each test authors a diagram, renders it to SVG, rasterises to PNG, and
//! pixel-diffs the result against a committed golden in `tests/snapshots/`.
//!
//! Workflow
//! --------
//!
//! ```sh
//! # Run snapshot tests like any other test
//! cargo test --workspace
//!
//! # Regenerate goldens after an intentional visual change
//! INSTA_UPDATE=1 cargo test -p closed-loop-test
//! ```
//!
//! On a failing snapshot, the actual render and a per-pixel diff image are
//! written to `target/test-output/snapshots-diff/<name>.{actual.png,diff.png}`
//! so a human can inspect the regression.
//!
//! Determinism note
//! ----------------
//!
//! No system fonts are loaded into `usvg`, which causes resvg to drop all
//! `<text>` elements. Snapshots therefore only contain the geometric
//! stencils, edges and group boundaries — the parts of the render that are
//! pixel-deterministic across machines. Text rasterisation (e.g. label
//! anti-aliasing under fontdb) is the main source of cross-machine jitter
//! and is intentionally excluded here.

use std::fs;
use std::path::{Path, PathBuf};

use drawio_author::{Diagram, GroupKind, GroupOpts, aws};
use resvg::usvg;

/// Maximum per-channel delta (in 0..=255) before a pixel is counted as
/// differing. Tuned just above pure equality to absorb harmless rounding
/// noise from resvg's vector rasteriser without masking real regressions.
const MAX_CHANNEL_DELTA: u8 = 5;

/// Maximum fraction of pixels allowed to differ before a snapshot fails.
/// 0.5% of total pixels.
const MAX_DIFF_FRACTION: f64 = 0.005;

/// Env var that, when set to `1`, overwrites the golden with the new render
/// instead of comparing. The name matches the `insta` convention so it's
/// familiar to Rust devs, even though we don't depend on `insta` itself.
const UPDATE_ENV: &str = "INSTA_UPDATE";

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn snapshot_dir() -> PathBuf {
    manifest_dir().join("tests").join("snapshots")
}

fn diff_out_dir() -> PathBuf {
    let workspace_root = manifest_dir()
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf();
    let dir = workspace_root
        .join("target")
        .join("test-output")
        .join("snapshots-diff");
    fs::create_dir_all(&dir).expect("create snapshots-diff dir");
    dir
}

fn update_mode() -> bool {
    matches!(std::env::var(UPDATE_ENV).ok().as_deref(), Some("1"))
}

/// Render a diagram to a `Pixmap` deterministically: no system fonts, no
/// upscaling. This is the only place rasterisation parameters live, so
/// goldens stay consistent across tests.
fn render_to_pixmap(diagram: &Diagram) -> tiny_skia::Pixmap {
    let xml = diagram.to_xml();
    let svg = drawio_render::render(&xml).expect("render svg");

    // Empty fontdb: resvg drops <text> elements rather than substituting a
    // system font, which removes the main source of cross-machine jitter.
    let opts = usvg::Options::default();
    let tree = usvg::Tree::from_str(&svg, &opts).expect("usvg parse");
    let size = tree.size().to_int_size();
    let mut pixmap = tiny_skia::Pixmap::new(size.width(), size.height()).expect("allocate pixmap");
    pixmap.fill(tiny_skia::Color::WHITE);
    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());
    pixmap
}

/// Decode a PNG file into an RGBA byte buffer plus `(width, height)`.
fn decode_png(path: &Path) -> (Vec<u8>, u32, u32) {
    let bytes = fs::read(path).unwrap_or_else(|e| panic!("read golden {}: {e}", path.display()));
    decode_png_bytes(&bytes)
}

/// Decode in-memory PNG bytes into an RGBA byte buffer plus `(width, height)`.
fn decode_png_bytes(bytes: &[u8]) -> (Vec<u8>, u32, u32) {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().expect("png decode");
    let info = reader.info().clone();
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut buf).expect("read frame");
    let raw = &buf[..frame.buffer_size()];
    // Normalise to RGBA so the diff loop is uniform.
    let rgba = match info.color_type {
        png::ColorType::Rgba => raw.to_vec(),
        png::ColorType::Rgb => {
            let mut out = Vec::with_capacity(raw.len() / 3 * 4);
            for chunk in raw.chunks_exact(3) {
                out.extend_from_slice(chunk);
                out.push(255);
            }
            out
        }
        other => panic!("unexpected golden color type: {other:?}"),
    };
    (rgba, info.width, info.height)
}

/// Compare `actual` (raw un-premultiplied RGBA bytes) against `golden`
/// (raw un-premultiplied RGBA bytes). Returns
/// `(differing_pixels, total_pixels, diff_image)`. `diff_image` is an RGBA
/// buffer that highlights differing pixels in red and dims everything else
/// to greyscale, for human inspection.
fn diff_rgba(actual_bytes: &[u8], golden: &[u8], width: u32, height: u32) -> (u32, u32, Vec<u8>) {
    let w = width as usize;
    let h = height as usize;
    let total = width * height;
    let mut diff_img = vec![0u8; w * h * 4];
    let mut differing = 0u32;
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            // tiny-skia stores premultiplied RGBA in the same byte order.
            let (ar, ag, ab) = (actual_bytes[i], actual_bytes[i + 1], actual_bytes[i + 2]);
            let (gr, gg, gb) = (golden[i], golden[i + 1], golden[i + 2]);
            let dr = ar.abs_diff(gr);
            let dg = ag.abs_diff(gg);
            let db = ab.abs_diff(gb);
            if dr > MAX_CHANNEL_DELTA || dg > MAX_CHANNEL_DELTA || db > MAX_CHANNEL_DELTA {
                differing += 1;
                diff_img[i] = 255;
                diff_img[i + 1] = 0;
                diff_img[i + 2] = 0;
                diff_img[i + 3] = 255;
            } else {
                // Dim greyscale so the eye locks onto red diff pixels.
                // Sum-of-three u8s / 3 is in 0..=255, so truncation is safe.
                let grey = u8::try_from((u16::from(gr) + u16::from(gg) + u16::from(gb)) / 3)
                    .unwrap_or(255);
                let dim = grey / 2 + 128;
                diff_img[i] = dim;
                diff_img[i + 1] = dim;
                diff_img[i + 2] = dim;
                diff_img[i + 3] = 255;
            }
        }
    }
    (differing, total, diff_img)
}

fn write_rgba_png(path: &Path, rgba: &[u8], width: u32, height: u32) {
    let file = fs::File::create(path).expect("create png");
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("png header");
    writer.write_image_data(rgba).expect("write png");
}

/// Compare `pixmap` against the golden at `tests/snapshots/<name>.png`. On
/// mismatch, write the actual render and a diff image under
/// `target/test-output/snapshots-diff/`.
///
/// If `INSTA_UPDATE=1` is set, overwrite the golden and return successfully.
fn assert_snapshot(name: &str, pixmap: &tiny_skia::Pixmap) {
    let golden_path = snapshot_dir().join(format!("{name}.png"));

    if update_mode() {
        fs::create_dir_all(snapshot_dir()).expect("create snapshot dir");
        pixmap.save_png(&golden_path).expect("save golden");
        eprintln!("[snapshot] updated golden: {}", golden_path.display());
        return;
    }

    assert!(
        golden_path.exists(),
        "missing golden {}. Run `INSTA_UPDATE=1 cargo test -p closed-loop-test` to generate it.",
        golden_path.display()
    );

    // Round-trip the actual render through PNG so we compare un-premultiplied
    // RGBA bytes in the same colour space the golden was decoded in.
    let actual_png = pixmap.encode_png().expect("encode actual png");
    let (actual_rgba, aw, ah) = decode_png_bytes(&actual_png);
    let (golden, gw, gh) = decode_png(&golden_path);
    assert!(
        aw == gw && ah == gh,
        "snapshot `{name}` dimension mismatch: actual {aw}x{ah}, golden {gw}x{gh}",
    );

    let (differing, total, diff_img) = diff_rgba(&actual_rgba, &golden, gw, gh);
    let fraction = f64::from(differing) / f64::from(total);

    if fraction > MAX_DIFF_FRACTION {
        let out = diff_out_dir();
        let actual_path = out.join(format!("{name}.actual.png"));
        let diff_path = out.join(format!("{name}.diff.png"));
        pixmap.save_png(&actual_path).expect("save actual");
        write_rgba_png(&diff_path, &diff_img, gw, gh);
        panic!(
            "snapshot `{name}` differs: {differing}/{total} pixels ({:.3}%) exceed \
             tolerance ({:.3}%). actual: {} | diff: {}",
            fraction * 100.0,
            MAX_DIFF_FRACTION * 100.0,
            actual_path.display(),
            diff_path.display(),
        );
    }
}

// --- Diagrams under test -------------------------------------------------

fn sample_simple_edge() -> Diagram {
    // Two AWS icons (API Gateway + Lambda) with a single edge between them.
    let mut d = Diagram::new("simple_edge");
    let api = d.add_node(aws::api_gateway("api", "API").at(80.0, 80.0));
    let lam = d.add_node(aws::lambda("lam", "Lambda").at(320.0, 80.0));
    d.connect(&api, &lam);
    d
}

fn sample_orthogonal() -> Diagram {
    // Three icons arranged in a Y so orthogonal routing has to bend both edges.
    let mut d = Diagram::new("orthogonal");
    let api = d.add_node(aws::api_gateway("api", "API").at(80.0, 80.0));
    let lam = d.add_node(aws::lambda("lam", "Lambda").at(320.0, 260.0));
    let ddb = d.add_node(aws::dynamodb("ddb", "DynamoDB").at(560.0, 80.0));
    d.connect(&api, &lam);
    d.connect(&lam, &ddb);
    d
}

fn sample_group() -> Diagram {
    // An AWS Account container with two children inside.
    let mut d = Diagram::new("group");
    d.add_group(GroupOpts::new(
        "acct-a",
        "Account A",
        40.0,
        40.0,
        320.0,
        200.0,
        GroupKind::AwsAccount,
    ));
    d.add_node(aws::lambda("lam", "Lambda").at(100.0, 120.0));
    d.add_node(aws::s3("s3", "Bucket").at(220.0, 120.0));
    d
}

// --- Tests ---------------------------------------------------------------

#[test]
fn snapshot_simple_edge() {
    let pixmap = render_to_pixmap(&sample_simple_edge());
    assert_snapshot("sample_simple_edge", &pixmap);
}

#[test]
fn snapshot_orthogonal() {
    let pixmap = render_to_pixmap(&sample_orthogonal());
    assert_snapshot("sample_orthogonal", &pixmap);
}

#[test]
fn snapshot_group() {
    let pixmap = render_to_pixmap(&sample_group());
    assert_snapshot("sample_group", &pixmap);
}
