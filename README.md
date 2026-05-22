# drawio-headless

Headless authoring and rendering of [drawio](https://drawio.com/) diagrams in
Rust. No browser, no DOM, no Node.

## Workspace layout

```
crates/
  author/   library: build .drawio XML programmatically
  render/   library: .drawio XML -> SVG (parses mxStencil + the style DSL)
  cli/      binary `drawio-headless`: file/stdin -> SVG file/stdout
  closed-loop-test/  integration test crate
stencils/
  aws4.xml  vendored from jgraph/drawio (see stencils/SOURCE)
```

## Usage

### Library: author a diagram, serialise to XML

```rust
use drawio_author::{Diagram, aws};

let mut d = Diagram::new("MyArch");
let api = d.add_node(aws::api_gateway("api", "API Gateway").at(80.0, 80.0));
let lam = d.add_node(aws::lambda("lam", "Lambda").at(320.0, 80.0));
d.connect(&api, &lam);
let xml: String = d.to_xml();
```

`Node::raw(id, x, y, w, h, label, style)` is the low-level escape hatch for
shapes outside the curated catalog.

### Library: render to SVG

```rust
let svg: String = drawio_render::render(&xml)?;
```

### CLI

```sh
drawio-headless render input.drawio output.svg
drawio-headless render --stdin > out.svg
cat input.drawio | drawio-headless render --stdin
```

Authoring via the CLI is not in scope for v0.

## Scope (v0)

- Authoring: 5 AWS resource-icon shapes
  (`api_gateway`, `lambda`, `s3`, `dynamodb`, `ec2`), plus the generic
  `Node::raw` escape hatch.
- Emits plain XML (`compressed="false"`); labels are plain text
  (`html=0`).
- Rendering: parses `mxCell` vertices and edges and the
  `mxgraph.aws4.resourceIcon` shape. Falls back to a plain coloured rect
  for unrecognised shapes.
- Stencil engine: supports the `<path>`, `<move>`, `<line>`, `<curve>`,
  `<quad>`, `<close/>`, `<fill/>`, `<stroke/>`, `<fillstroke/>`,
  `<ellipse>`, `<rect>` and `<roundrect>` commands.
- Edges: straight line between bounding-box midpoints with a simple
  arrowhead. No orthogonal routing yet.
- Compressed `<diagram>` payloads (the drawio editor's default on save)
  are inflated transparently — `render()` accepts both compressed and
  uncompressed `.drawio` files.

## Development

```sh
cargo test --workspace        # runs the closed-loop test too
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
```

The closed-loop test (`crates/closed-loop-test/tests/closed_loop.rs`) writes
artifacts to `target/test-output/`:
`api-lambda.drawio`, `api-lambda.svg`, `api-lambda.png`.

Visual regression is covered by
`crates/closed-loop-test/tests/snapshots.rs`, which pixel-diffs rendered
PNGs against committed goldens in `crates/closed-loop-test/tests/snapshots/`.
To regenerate goldens after an intentional visual change:

```sh
INSTA_UPDATE=1 cargo test -p closed-loop-test
git add crates/closed-loop-test/tests/snapshots/
git commit -m "test: refresh golden snapshots"
```

See `CONTRIBUTING.md` for tolerances, determinism notes, and the diff
artifact location.

## License

Apache-2.0. The vendored stencil (`stencils/aws4.xml`) is from
[jgraph/drawio](https://github.com/jgraph/drawio), also Apache-2.0;
see `stencils/SOURCE` for the exact commit.
