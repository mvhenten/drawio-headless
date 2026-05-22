//! End-to-end integration tests for the `drawio-headless author` subcommand.
//!
//! Each test invokes the compiled CLI binary via `std::process::Command` so
//! the harness exercises argv parsing, file I/O, stdin/stdout and the JSON
//! glue together. We avoid `assert_cmd` to keep dev-deps minimal — the
//! standard library is enough for spawning a binary and inspecting stdout.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use drawio_author::{Diagram, aws};

/// Path to the compiled `drawio-headless` binary (provided by Cargo for
/// integration tests).
fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_drawio-headless")
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

fn target_tmp() -> PathBuf {
    // Cargo sets CARGO_TARGET_TMPDIR for integration tests; fall back to
    // OUT_DIR-adjacent path if it's missing (older toolchains).
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
    let stdout = String::from_utf8(out.stdout).expect("utf8 stdout");
    let stderr = String::from_utf8(out.stderr).expect("utf8 stderr");
    (out.status, stdout, stderr)
}

#[test]
fn author_file_to_file_round_trips_through_render() {
    let input = fixtures_dir().join("api-lambda.json");
    let out_dir = target_tmp();
    std::fs::create_dir_all(&out_dir).expect("create tmp");
    let output = out_dir.join("api-lambda.drawio");

    let (status, _stdout, stderr) = run(
        &["author", input.to_str().unwrap(), output.to_str().unwrap()],
        None,
    );
    assert!(status.success(), "exit failed: {stderr}");

    let xml = std::fs::read_to_string(&output).expect("read output");
    assert!(xml.contains("resIcon=mxgraph.aws4.api_gateway"));
    assert!(xml.contains("resIcon=mxgraph.aws4.lambda"));

    // Feed the authored XML through the renderer.
    let svg = drawio_render::render(&xml).expect("render");
    assert!(svg.starts_with("<svg"));
    assert!(svg.contains("<path"), "expected stencil glyph path: {svg}");
}

#[test]
fn author_stdin_to_stdout_round_trips() {
    let input = std::fs::read(fixtures_dir().join("api-lambda.json")).expect("read fixture");
    let (status, stdout, stderr) = run(&["author", "--stdin"], Some(&input));
    assert!(status.success(), "exit failed: {stderr}");
    assert!(stdout.contains("source=\"api\" target=\"lam\""), "{stdout}");
    let svg = drawio_render::render(&stdout).expect("render stdin output");
    assert!(svg.starts_with("<svg"));
}

#[test]
fn round_trip_byte_identical_to_library_authored_diagram() {
    // Build the same logical diagram two ways and assert the .drawio XML is
    // byte-identical. This is the headline guarantee: the CLI is a faithful
    // frontend, not a parallel implementation that might drift.

    // (a) Library path — what a Rust user would write today.
    let mut d = Diagram::new("ApiLambda");
    let api = d.add_node(aws::api_gateway("api", "API Gateway").at(80.0, 80.0));
    let lam = d.add_node(aws::lambda("lam", "Lambda").at(320.0, 80.0));
    d.connect(&api, &lam);
    let lib_xml = d.to_xml();

    // (b) CLI path — JSON describing the same shape.
    let input = fixtures_dir().join("api-lambda.json");
    let out = target_tmp().join("round-trip.drawio");
    let (status, _stdout, stderr) = run(
        &["author", input.to_str().unwrap(), out.to_str().unwrap()],
        None,
    );
    assert!(status.success(), "exit: {stderr}");
    let cli_xml = std::fs::read_to_string(&out).expect("read cli xml");

    assert_eq!(
        lib_xml, cli_xml,
        "library and CLI must produce byte-identical .drawio output",
    );
}

#[test]
fn rejects_unknown_node_kind() {
    let json = r#"{"nodes": [{"id": "x", "kind": "aws.lambba", "x": 0, "y": 0}]}"#;
    let (status, _stdout, stderr) = run(&["author", "--stdin"], Some(json.as_bytes()));
    assert!(!status.success(), "expected failure");
    assert!(stderr.contains("aws.lambba"), "stderr: {stderr}");
    assert!(
        stderr.contains("aws.lambda"),
        "expected suggestion: {stderr}"
    );
}

#[test]
fn rejects_missing_required_field() {
    // Missing `y` on a node.
    let json = r#"{"nodes": [{"id": "x", "kind": "aws.lambda", "x": 0}]}"#;
    let (status, _stdout, stderr) = run(&["author", "--stdin"], Some(json.as_bytes()));
    assert!(!status.success());
    assert!(stderr.contains("invalid JSON"), "stderr: {stderr}");
    assert!(
        stderr.to_lowercase().contains("missing"),
        "stderr: {stderr}"
    );
}

#[test]
fn rejects_edge_with_unknown_endpoint() {
    let json = r#"{
        "nodes": [{"id": "a", "kind": "aws.lambda", "x": 0, "y": 0}],
        "edges": [{"source": "a", "target": "ghost"}]
    }"#;
    let (status, _stdout, stderr) = run(&["author", "--stdin"], Some(json.as_bytes()));
    assert!(!status.success());
    assert!(stderr.contains("ghost"), "stderr: {stderr}");
}

#[test]
fn rejects_raw_node_without_style() {
    let json = r#"{"nodes": [{"id": "r", "kind": "raw", "x": 0, "y": 0}]}"#;
    let (status, _stdout, stderr) = run(&["author", "--stdin"], Some(json.as_bytes()));
    assert!(!status.success());
    assert!(
        stderr.contains("raw") && stderr.contains("style"),
        "stderr: {stderr}",
    );
}
