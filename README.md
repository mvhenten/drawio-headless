# drawio-headless

Render [drawio](https://drawio.com/) diagrams headlessly: describe a diagram
as JSON, get back SVG, PNG, or a `.drawio` file. No browser, no DOM, no Node
required to run it.

## Demo

<https://mvhenten.github.io/drawio-headless/> — rendered examples with the
JSON and `.drawio` source next to each.

## Install

Three paths, in order of convenience.

### npm (recommended)

Works on Linux (x86\_64 / aarch64), macOS (x86\_64 / arm64), and
Windows (x86\_64). `npm install` downloads the matching pre-built
binary from GitHub Releases as a postinstall step.

```sh
npm install -g drawio-headless
drawio-headless --version
```

### Curl install script (no Node)

```sh
curl -fsSL https://raw.githubusercontent.com/mvhenten/drawio-headless/main/scripts/install.sh | sh
```

Drops the binary in `~/.local/bin/drawio-headless` (override with
`INSTALL_DIR=…`). Pin a specific release with `VERSION=v0.1.0 ...`.
The script prints a `PATH` hint when `~/.local/bin` isn't already on
your shell's path. macOS and Linux only — Windows users should use the
npm package.

### Cargo (developer / Rust path)

```sh
cargo install --git https://github.com/mvhenten/drawio-headless --path crates/cli
```

Or clone and `cargo install --path crates/cli` from a local checkout.

## CLI usage

Write a diagram as JSON:

```json
{
  "name": "ApiLambda",
  "nodes": [
    {"id": "api", "kind": "aws.api_gateway", "label": "API Gateway", "x": 80, "y": 80},
    {"id": "lam", "kind": "aws.lambda",      "label": "Lambda",      "x": 320, "y": 80}
  ],
  "edges": [
    {"source": "api", "target": "lam"}
  ]
}
```

Render it:

```sh
drawio-headless compose diagram.json out.svg
drawio-headless compose diagram.json out.png --format png
drawio-headless compose diagram.json out.svg --keep-drawio out.drawio
cat diagram.json | drawio-headless compose --stdin > out.svg
```

Other commands:

```sh
# .drawio XML -> SVG
drawio-headless render input.drawio output.svg
cat input.drawio | drawio-headless render --stdin

# JSON -> .drawio XML, without rendering
drawio-headless author input.json output.drawio

# List every shape the JSON schema understands
drawio-headless list-shapes --format json
drawio-headless list-shapes --library aws --format text
```

PNG output is gated behind the `rasterize` feature, enabled by default.
Build a slim binary without `resvg` via `cargo build --no-default-features`.

## The JSON format

You don't need to memorize this — the schema is small and flat by design so
an LLM or coding agent can write it correctly on the first try. Point Claude
Code (or any agent) at `docs/authoring-schema.md` and let it generate the
JSON for you.

A node is `{"id", "kind", "label", "x", "y"}`; an edge is
`{"source", "target"}`. `kind` is `<library>.<name>` — `aws.lambda`,
`azure.sql_database`, `gcp.bigquery`, `k8s.pod`, or `raw` with your own
style string. Run `drawio-headless list-shapes` to see every available
shape instead of memorizing the catalogue.

Full reference: [`docs/authoring-schema.md`](docs/authoring-schema.md).

### Using as a Claude Code skill

`skill/` packages `drawio-headless` as a [Claude Code
skill](https://docs.claude.com/en/docs/claude-code/skills), so an agent can
author and render diagrams straight from a prompt like "draw an AWS
architecture with API Gateway, Lambda, and DynamoDB":

```sh
cp -r skill ~/.claude/skills/drawio-headless
bash ~/.claude/skills/drawio-headless/scripts/ensure.sh
```

`ensure.sh` checks that `drawio-headless` is on `PATH` and prints install
instructions if it isn't — it won't install anything for you. See
[`skill/SKILL.md`](skill/SKILL.md) for trigger phrases, the schema
reference, and the layout rules (anchor semantics, arrow routing, axis
alignment) that make a composed diagram read correctly.

## Scope

- **Catalogues**: `aws` (~30 resources: compute, database, storage,
  networking, security, integration, analytics), `azure` (15 legacy
  shapes), `gcp` (15 shapes), `k8s` (10 core primitives). `raw` is the
  escape hatch for anything outside these. Run `list-shapes` for the exact,
  current set — it's generated from the same catalogue the renderer uses.
- **Rendering**: parses `mxCell` vertices and edges, resolves each shape's
  stencil glyph from four bundled libraries (AWS, Azure, GCP, Kubernetes),
  and falls back to a plain coloured rect for unrecognised shapes.
- **Stencil engine** supports `<path>`, `<move>`, `<line>`, `<curve>`,
  `<quad>`, `<close/>`, `<fill/>`, `<stroke/>`, `<fillstroke/>`,
  `<ellipse>`, `<rect>`, `<roundrect>`. `<arc>`, `<save>`/`<restore>`,
  `<alpha>`, `<strokecolor>`, `<fillcolor>` are silently skipped — see
  issue #7.
- Edges use orthogonal two-segment routing for
  `edgeStyle=orthogonalEdgeStyle`, straight lines otherwise, and snap to
  declared `points=[…]` connection constraints.
- Compressed `<diagram>` payloads (the drawio editor's default on save)
  are inflated transparently.

### Render fidelity per library

| Library | Glyph fidelity | Notes |
| ------- | -------------- | ----- |
| AWS     | High            | Stencils use only `<path>`-family commands the engine fully supports. |
| Kubernetes | Good         | Same path-only stencil set; tiles render with their canonical blue fill and white glyph. |
| Azure   | Low             | Heavy use of `<arc>` for every silhouette; rendered shapes are skeletal until issue #7 is closed. |
| GCP     | Medium          | Outer hexagon silhouette renders correctly; interior detail relies on `<save>`/`<alpha>`/`<arc>`, which are skipped today. |

The `.drawio` XML itself is always correct and round-trips through the
upstream drawio editor with full fidelity — these caveats only apply to
the headless rasteriser.

## Development

```sh
cargo test --workspace        # runs the closed-loop test too
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
```

Or `make test` / `make fix` / `make lint`. These also run as parallel jobs
on every PR via `.github/workflows/ci.yml`; PRs cannot merge with a red
build. See [`CONTRIBUTING.md`](CONTRIBUTING.md) for snapshot-test details
(tolerances, regenerating goldens, determinism notes).

The demo page is deployed by `.github/workflows/pages.yml` on every push to
`main`. It regenerates `docs/examples/*.svg` from
`crates/examples/examples/` and publishes `docs/` — the SVGs aren't
committed to the repo. Preview locally:

```sh
./scripts/build-examples.sh   # regenerates docs/examples/*.svg
open docs/index.html          # any static server / browser works
```

## Advanced: using as a Rust crate

```
crates/
  author/   library: build .drawio XML programmatically
  render/   library: .drawio XML -> SVG (parses mxStencil + the style DSL)
  cli/      binary `drawio-headless`: file/stdin -> SVG file/stdout
  closed-loop-test/  integration test crate
stencils/
  aws4.xml         vendored from jgraph/drawio (see stencils/SOURCE)
  azure.xml        vendored from jgraph/drawio (see stencils/SOURCE-azure)
  gcp.xml          concatenated upstream category files
                   (see stencils/SOURCE-gcp)
  kubernetes.xml   vendored from jgraph/drawio
                   (see stencils/SOURCE-kubernetes)
```

Author a diagram and serialise it to XML:

```rust
use drawio_author::{Diagram, aws};

let mut d = Diagram::new("MyArch");
let api = d.add_node(aws::api_gateway("api", "API Gateway").at(80.0, 80.0));
let lam = d.add_node(aws::lambda("lam", "Lambda").at(320.0, 80.0));
d.connect(&api, &lam);
let xml: String = d.to_xml();
```

Four curated catalogues are exposed: `aws`, `azure`, `gcp`, `k8s`. Each
emits the canonical drawio style strings for its library, so the
resulting `.drawio` files round-trip through the upstream editor:

```rust
use drawio_author::{Diagram, azure, gcp, k8s};

let mut d = Diagram::new("PolyCloud");
d.add_node(azure::sql_database("db", "Orders").at(80.0, 80.0));
d.add_node(gcp::bigquery("bq", "Warehouse").at(240.0, 80.0));
d.add_node(k8s::pod("p", "frontend").at(400.0, 80.0));
```

`Node::raw(id, x, y, w, h, label, style)` is the low-level escape hatch for
shapes outside the curated catalogues.

Render to SVG:

```rust
let svg: String = drawio_render::render(&xml)?;
```

The JSON path (`author`/`compose` in the CLI) is a thin frontend over this
same library — feeding either path the same logical diagram produces
byte-identical `.drawio` output.

## License

Apache-2.0. The vendored stencils under `stencils/` are from
[jgraph/drawio](https://github.com/jgraph/drawio), also Apache-2.0.
See `stencils/SOURCE`, `stencils/SOURCE-azure`, `stencils/SOURCE-gcp`,
and `stencils/SOURCE-kubernetes` for the exact upstream paths and
commit hash.
