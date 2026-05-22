//! Integration tests for the `compose` and `list-shapes` subcommands.
//!
//! Spawns the compiled CLI binary so we exercise argv parsing, file I/O and
//! the feature-gated PNG path. PNG assertions only run when the binary was
//! built with the `rasterize` feature (the default).

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_drawio-headless")
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

fn target_tmp() -> PathBuf {
    PathBuf::from(option_env!("CARGO_TARGET_TMPDIR").unwrap_or(env!("CARGO_MANIFEST_DIR")))
}

fn run(args: &[&str], stdin: Option<&[u8]>) -> (std::process::ExitStatus, String, String) {
    let mut cmd = Command::new(bin());
    cmd.args(args);
    if stdin.is_some() {
        cmd.stdin(Stdio::piped());
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().expect("spawn drawio-headless");
    if let Some(buf) = stdin {
        child
            .stdin
            .as_mut()
            .expect("stdin pipe")
            .write_all(buf)
            .expect("write stdin");
    }
    let out = child.wait_with_output().expect("wait");
    (
        out.status,
        String::from_utf8(out.stdout).expect("utf8 stdout"),
        String::from_utf8(out.stderr).expect("utf8 stderr"),
    )
}

#[test]
fn compose_writes_svg_at_explicit_path() {
    let input = fixtures_dir().join("api-lambda.json");
    let out_dir = target_tmp();
    std::fs::create_dir_all(&out_dir).expect("mkdir");
    let svg_path = out_dir.join("compose-explicit.svg");
    let _ = std::fs::remove_file(&svg_path);

    let (status, _stdout, stderr) = run(
        &[
            "compose",
            input.to_str().unwrap(),
            svg_path.to_str().unwrap(),
        ],
        None,
    );
    assert!(status.success(), "exit failed: {stderr}");
    let svg = std::fs::read_to_string(&svg_path).expect("read svg");
    assert!(svg.starts_with("<svg"), "expected SVG, got: {svg:.80}");
    assert!(
        svg.contains("<path"),
        "expected at least one stencil path in SVG"
    );
}

#[test]
fn compose_keep_drawio_writes_both_artifacts() {
    let input = fixtures_dir().join("api-lambda.json");
    let out_dir = target_tmp();
    std::fs::create_dir_all(&out_dir).expect("mkdir");
    let svg_path = out_dir.join("compose-keep.svg");
    let drawio_path = out_dir.join("compose-keep.drawio");
    let _ = std::fs::remove_file(&svg_path);
    let _ = std::fs::remove_file(&drawio_path);

    let (status, _, stderr) = run(
        &[
            "compose",
            input.to_str().unwrap(),
            svg_path.to_str().unwrap(),
            "--keep-drawio",
            drawio_path.to_str().unwrap(),
        ],
        None,
    );
    assert!(status.success(), "exit failed: {stderr}");

    let drawio = std::fs::read_to_string(&drawio_path).expect("read drawio");
    assert!(drawio.starts_with("<mxfile"));
    assert!(drawio.contains("resIcon=mxgraph.aws4.api_gateway"));

    let svg = std::fs::read_to_string(&svg_path).expect("read svg");
    assert!(svg.starts_with("<svg"));
}

#[test]
fn compose_stdin_to_stdout_svg() {
    let json = std::fs::read(fixtures_dir().join("api-lambda.json")).expect("fixture");
    let (status, stdout, stderr) = run(&["compose", "--stdin"], Some(&json));
    assert!(status.success(), "exit failed: {stderr}");
    assert!(stdout.starts_with("<svg"), "expected SVG on stdout");
}

#[cfg(feature = "rasterize")]
#[test]
fn compose_png_writes_valid_png() {
    let input = fixtures_dir().join("api-lambda.json");
    let out_dir = target_tmp();
    std::fs::create_dir_all(&out_dir).expect("mkdir");
    let png_path = out_dir.join("compose-png.png");
    let _ = std::fs::remove_file(&png_path);

    let (status, _stdout, stderr) = run(
        &[
            "compose",
            input.to_str().unwrap(),
            png_path.to_str().unwrap(),
            "--format",
            "png",
        ],
        None,
    );
    assert!(status.success(), "exit failed: {stderr}");
    let bytes = std::fs::read(&png_path).expect("read png");
    // PNG magic: 89 50 4E 47 0D 0A 1A 0A
    assert_eq!(
        &bytes[..8],
        &[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a],
        "not a PNG"
    );
    assert!(bytes.len() > 200, "PNG too small: {} bytes", bytes.len());
}

#[test]
fn list_shapes_json_is_valid_and_covers_well_known_keys() {
    let (status, stdout, stderr) = run(&["list-shapes", "--format", "json"], None);
    assert!(status.success(), "exit failed: {stderr}");
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("parse json");
    let arr = v.as_array().expect("json array");
    assert!(arr.len() >= 30, "catalogue too small: {}", arr.len());

    let kinds: Vec<String> = arr
        .iter()
        .map(|e| {
            format!(
                "{}.{}",
                e["library"].as_str().unwrap(),
                e["key"].as_str().unwrap()
            )
        })
        .collect();
    for needle in [
        "aws.lambda",
        "aws.api_gateway",
        "azure.sql_database",
        "gcp.cloud_functions",
        "k8s.pod",
    ] {
        assert!(
            kinds.iter().any(|k| k == needle),
            "missing {needle}: {kinds:?}",
        );
    }

    // Every entry has the documented shape.
    for e in arr {
        assert!(e["library"].is_string());
        assert!(e["key"].is_string());
        assert!(e["category"].is_string());
    }
}

#[test]
fn list_shapes_text_is_grouped_by_library() {
    let (status, stdout, stderr) = run(&["list-shapes", "--format", "text"], None);
    assert!(status.success(), "exit failed: {stderr}");
    // Library headers
    assert!(stdout.contains("aws:"), "missing aws header: {stdout}");
    assert!(stdout.contains("azure:"));
    assert!(stdout.contains("gcp:"));
    assert!(stdout.contains("k8s:"));
    // Fully-qualified kinds
    assert!(stdout.contains("aws.lambda"));
    assert!(stdout.contains("k8s.pod"));
}

#[test]
fn list_shapes_library_filter() {
    let (status, stdout, stderr) = run(
        &["list-shapes", "--library", "aws", "--format", "json"],
        None,
    );
    assert!(status.success(), "exit failed: {stderr}");
    let arr: serde_json::Value = serde_json::from_str(&stdout).expect("parse json");
    let arr = arr.as_array().expect("array");
    assert!(arr.iter().all(|e| e["library"].as_str() == Some("aws")));
    assert!(arr.iter().any(|e| e["key"].as_str() == Some("lambda")));
}

#[test]
fn compose_with_multi_library_input() {
    // Smoke test: a JSON spec mixing azure + gcp + k8s factories should
    // pass through compose. Validates the kind resolver was extended past
    // aws.* in this PR.
    let json = r#"{
        "name": "MultiCloud",
        "nodes": [
            {"id": "db",  "kind": "azure.sql_database",  "x": 0,   "y": 0},
            {"id": "bq",  "kind": "gcp.bigquery",        "x": 200, "y": 0},
            {"id": "pod", "kind": "k8s.pod",             "x": 400, "y": 0}
        ]
    }"#;
    // --stdin writes to stdout by default; capture and assert.
    let (status, stdout, stderr) = run(&["compose", "--stdin"], Some(json.as_bytes()));
    assert!(status.success(), "exit failed: {stderr}");
    assert!(stdout.starts_with("<svg"), "expected SVG: {stdout:.80}");
    assert!(stdout.contains("</svg>"), "SVG should close cleanly");
}

#[test]
fn error_messages_are_single_line() {
    let json = r#"{"nodes": [{"id": "x", "kind": "aws.lambba", "x": 0, "y": 0}]}"#;
    let (status, _stdout, stderr) = run(&["compose", "--stdin"], Some(json.as_bytes()));
    assert!(!status.success());
    let trimmed = stderr.trim_end_matches('\n');
    assert!(
        !trimmed.contains('\n'),
        "stderr should be single line: {stderr:?}",
    );
    assert!(stderr.starts_with("error: "), "stderr: {stderr}");
}
