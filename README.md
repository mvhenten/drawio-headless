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
  aws4.xml         vendored from jgraph/drawio (see stencils/SOURCE)
  azure.xml        vendored from jgraph/drawio (see stencils/SOURCE-azure)
  gcp.xml          concatenated upstream category files
                   (see stencils/SOURCE-gcp)
  kubernetes.xml   vendored from jgraph/drawio
                   (see stencils/SOURCE-kubernetes)
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

### Library: render to SVG

```rust
let svg: String = drawio_render::render(&xml)?;
```

### CLI

```sh
drawio-headless render input.drawio output.svg
drawio-headless render --stdin > out.svg
cat input.drawio | drawio-headless render --stdin

drawio-headless author input.json output.drawio
drawio-headless author --stdin > out.drawio
cat input.json | drawio-headless author --stdin

# Author + render in one shot. Writes ./<input-stem>.svg by default.
drawio-headless compose input.json
drawio-headless compose input.json out.svg
drawio-headless compose input.json out.png --format png
drawio-headless compose input.json out.svg --keep-drawio out.drawio

# Enumerate the curated factory catalogue (LLM-friendly).
drawio-headless list-shapes --format json
drawio-headless list-shapes --library aws --format text
```

PNG output is gated behind the `rasterize` feature, which is enabled by
default. Build a slim binary without `resvg` via
`cargo build --no-default-features`.

The `author` subcommand reads a small declarative JSON schema and emits a
`.drawio` XML file (full reference: [`docs/authoring-schema.md`](docs/authoring-schema.md)):

```json
{
  "name": "ApiLambda",
  "nodes": [
    {"id": "api", "kind": "aws.api_gateway", "label": "API", "x": 80, "y": 80},
    {"id": "lam", "kind": "aws.lambda",      "label": "Lambda", "x": 320, "y": 80}
  ],
  "edges": [{"source": "api", "target": "lam"}]
}
```

The JSON path is a thin frontend over the library: feeding either path the
same logical diagram produces byte-identical `.drawio` output.

### Using as a Claude Code skill

The `skill/` directory packages `drawio-headless` as a [Claude Code
skill](https://docs.claude.com/en/docs/claude-code/skills) so an LLM can
author and render cloud architecture diagrams from natural-language
prompts. Install with:

```sh
cp -r skill ~/.claude/skills/drawio-headless
bash ~/.claude/skills/drawio-headless/scripts/ensure.sh
```

`ensure.sh` checks that the `drawio-headless` binary is on PATH and
prints copy-pasteable install instructions if it isn't (no automatic
install — by design). The skill itself triggers on phrases like "draw an
AWS architecture", "create a cloud diagram", or "diagram with AWS /
Azure / GCP", and uses `drawio-headless compose` under the hood. See
[`skill/SKILL.md`](skill/SKILL.md) for trigger phrases, the JSON
schema, worked example, and common pitfalls.

## Scope

- **Authoring catalogues**, all built on a shared `Node` / `Diagram` model:
  - `aws` — ~30 AWS resource-icon factories plus group containers
    (`AwsAccount`, `AwsVpc`, `AwsCloud`):
    - **Application Integration**: `api_gateway`, `sqs`, `sns`,
      `eventbridge`, `step_functions`, `appsync`
    - **Compute**: `lambda`, `ec2`, `ecs`, `eks`, `fargate`,
      `app_runner`, `batch`
    - **Database**: `dynamodb`, `rds`, `elasticache`
    - **Storage**: `s3`, `efs`
    - **Networking & Content Delivery**: `cloudfront`, `route_53`,
      `vpc`, `elastic_load_balancing`
    - **Security, Identity & Compliance**: `iam`, `cognito`,
      `secrets_manager`, `kms`
    - **Analytics**: `kinesis`, `athena`, `msk`
    - **Management & Governance**: `cloudwatch`
  - `azure` — 15 legacy Azure shapes (Active Directory, SQL Database,
    Service Bus, Storage Blob, Virtual Machine, Traffic Manager, …).
  - `gcp` — 15 GCP shapes across compute, storage, big_data,
    networking, identity_and_security, and management_tools.
  - `k8s` — 10 core Kubernetes primitives (Pod, Deployment, Service,
    Ingress, ConfigMap, Secret, Namespace, Node, PV, ReplicaSet).
  - Generic `Node::raw` escape hatch for shapes outside any catalogue.
- Emits plain XML (`compressed="false"`); labels are plain text
  (`html=0`).
- **Rendering**: parses `mxCell` vertices and edges. Resolves the
  vertex's stencil glyph via four bundled stencil libraries — AWS,
  Azure, GCP, Kubernetes — selected from the shape's library prefix.
  Falls back to a plain coloured rect for unrecognised shapes.
- **Stencil engine**: supports `<path>`, `<move>`, `<line>`, `<curve>`,
  `<quad>`, `<close/>`, `<fill/>`, `<stroke/>`, `<fillstroke/>`,
  `<ellipse>`, `<rect>` and `<roundrect>`. Other commands (`<arc>`,
  `<save>`/`<restore>`, `<alpha>`, `<strokecolor>`, `<fillcolor>`, …)
  are silently skipped — see issue #7.
- Edges: orthogonal two-segment routing for
  `edgeStyle=orthogonalEdgeStyle`; straight line otherwise. Endpoints
  snap to declared `points=[…]` connection-point constraints.
- Compressed `<diagram>` payloads (the drawio editor's default on save)
  are inflated transparently — `render()` accepts both compressed and
  uncompressed `.drawio` files.

### Render fidelity per library

| Library | Glyph fidelity | Notes |
| ------- | -------------- | ----- |
| AWS     | High            | Stencils use only `<path>`-family commands the engine fully supports. |
| Kubernetes | Good         | Same path-only stencil set; tiles render with their canonical blue fill and white glyph. |
| Azure   | Low             | Heavy use of `<arc>` for every silhouette; rendered shapes are skeletal until issue #7 is closed. |
| GCP     | Medium          | Outer hexagon silhouette renders correctly; interior detail relies on `<save>`/`<alpha>`/`<arc>` which are skipped today. |

Style strings emitted by the authoring layer are correct in all four
cases and round-trip through the upstream drawio editor with full
fidelity. The fidelity caveats only apply to the headless rasteriser.

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

Apache-2.0. The vendored stencils under `stencils/` are from
[jgraph/drawio](https://github.com/jgraph/drawio), also Apache-2.0.
See `stencils/SOURCE`, `stencils/SOURCE-azure`, `stencils/SOURCE-gcp`,
and `stencils/SOURCE-kubernetes` for the exact upstream paths and
commit hash.
