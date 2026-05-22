# drawio-headless — Design

## Goal

Author and render [drawio](https://drawio.com/) diagrams from any language, headlessly, without Chromium or Electron. Optimised for LLM-driven authoring of cloud architecture diagrams (AWS first).

The deliverable is a Rust library + CLI that:

1. Emits valid `.drawio` files interoperable with `app.diagrams.net`.
2. Renders those files to SVG.

## Motivation

The drawio editor is a JavaScript application. Every "official" export pipeline wraps Chromium — drawio-desktop is an Electron app, `draw-image-export2` is Node + Puppeteer, the docker image is the desktop binary under xvfb. For programmatic authoring (LLMs, CI, Lambda) this is the wrong shape: heavy dependency tree, slow cold-start, awkward in serverless.

Two observations make a cleaner solution possible:

1. The `.drawio` on-disk format is **plain XML** when `compressed="false"`. Authoring is string-templating with a service catalogue — no DOM, no JS engine.
2. The drawio rendering pipeline is **declarative**: shapes resolve to mxStencil XML, a small vector-drawing DSL (`move/line/curve/path/fillstroke`). A few hundred lines of Rust can interpret it into SVG.

drawio interop is preserved by sharing the **on-disk format**, not by sharing any code.

## Non-goals (durable)

These are not "later"; they are "no":

- **No Electron / Chromium wrapper**, even as an optional higher-fidelity adapter.
- **No port of mxGraph or maxGraph**. The drawio app ships a 14k-LOC editor layer on top of its vendored mxGraph fork; replaying it under jsdom inherits a heavy dependency tree for only ~80% fidelity.
- **No pixel-perfect drawio parity**. "Renders" is the bar. Recognisable AWS icons, correct colours, sensible edges.
- **No browser DOM in the core library**. Plain SVG is emitted; rasterisation is the consumer's problem.
- **No foreignObject / HTML labels** (`html=1`). Authored documents use `html=0`.
- **No editor UI**. Authoring is offered as a library API and a thin JSON
  CLI; no graphical editor.

## Architecture

Three crates, hard-decoupled:

```
drawio-headless/
  crates/
    author/            library: zero-IO, builds .drawio XML in memory
    render/            library: parses .drawio XML, emits SVG
    cli/               binary: thin wrapper exposing `render`
    closed-loop-test/  integration: author -> render -> rasterise -> assert
  stencils/
    aws4.xml           vendored from jgraph/drawio (Apache-2.0)
```

The split lets an LLM author a diagram, persist it as a `.drawio` document interchangeable with app.diagrams.net, and feed that document — possibly hours later, on a different machine, through a different language — into the renderer to produce SVG. **The authored document is the durable interface.**

### `author`

- `Diagram::new(name)` creates a graph.
- `Diagram::add_node(node)` appends a vertex; returns an opaque handle.
- `Diagram::connect(&from, &to)` appends an edge.
- `Diagram::to_xml()` serialises to a `.drawio` XML string.
- AWS factories on `aws::*` emit the canonical resourceIcon style (`shape=mxgraph.aws4.resourceIcon;resIcon=mxgraph.aws4.<key>;`) with the AWS category fill colour.
- `Node::raw(id, x, y, w, h, label, style)` is the low-level escape hatch — accepts any style string verbatim.
- Zero I/O, no DOM, no async, no global state.

### `render`

- `render(xml: &str) -> Result<String, RenderError>` is the single public entry.
- Parses `mxfile -> diagram -> mxGraphModel -> root -> mxCell` via `quick-xml`.
- For each vertex: parses the style attribute. If `shape=mxgraph.aws4.resourceIcon`, draws a coloured tile + looks up the named stencil and renders its `<foreground>` path inside the tile in white. Otherwise: plain coloured rectangle with a label.
- For each edge: straight line between source/target bounding-box midpoints with an optional arrowhead marker.
- The stencil library is parsed once via `OnceLock` — `aws4.xml` is 6 MB; per-render parsing would be wasteful.
- Compressed `<diagram>` payloads (the default in interactively-saved drawio files) are inflated transparently before parsing: trim, base64-decode, raw DEFLATE, URL-decode, hand the resulting `<mxGraphModel>` XML to the same parser. Detection is body-based (first non-whitespace char): if it is `<` we treat it as plain XML, otherwise as compressed — more robust than trusting the optional `compressed="..."` attribute on `<mxfile>`.

### `cli`

```sh
drawio-headless render input.drawio output.svg
drawio-headless render --stdin > out.svg

drawio-headless author input.json output.drawio
drawio-headless author --stdin > out.drawio
```

Thin wrapper. No business logic in the renderer path; the `author` path is
pure glue — JSON → `serde` structs → `drawio-author` library calls. The
author library itself stays `serde`-free (zero JSON deps), and the JSON glue
lives only in the CLI crate. Round-trip byte-equivalence between
library-authored and CLI-authored diagrams is asserted in
`crates/cli/tests/author.rs`. Schema reference:
[`docs/authoring-schema.md`](authoring-schema.md).

### `closed-loop-test`

The closed feedback loop is encoded as an integration test:

1. `author` builds a 2-node + 1-edge diagram (with an XML-special label to exercise escaping).
2. Serialise to `.drawio`; write to `target/test-output/api-lambda.drawio`.
3. Run through `render`; write to `target/test-output/api-lambda.svg`.
4. Rasterise via `resvg` (dev-dep only); write to `target/test-output/api-lambda.png`.
5. Assertions:
   - PNG decodes; dimensions ≥ 200×100.
   - Non-background pixel count above threshold (we drew something).
   - AWS-orange pixel count inside the Lambda tile region above threshold (proves the stencil glyph drew in the right place with the right colour).

The PNG is **not a product feature**. It is the validator. Rasterising the SVG lets us assert *did anything actually appear, and is it in roughly the right place* without committing golden snapshots.

A snapshot from the current test run is checked in at `docs/sample-output.png`.

## The `.drawio` format

Verified against the drawio source (`jgraph/drawio@1d9b73f`; see `stencils/SOURCE`).

```xml
<mxfile compressed="false">
  <diagram name="Page-1">
    <mxGraphModel ...>
      <root>
        <mxCell id="0"/>                    <!-- implicit root -->
        <mxCell id="1" parent="0"/>          <!-- default layer -->
        <mxCell id="x" vertex="1" parent="1" style="..." value="...">
          <mxGeometry x= y= width= height= as="geometry"/>
        </mxCell>
        <mxCell id="e" edge="1" parent="1" source="x" target="y" style="...">
          <mxGeometry relative="1" as="geometry"/>
        </mxCell>
      </root>
    </mxGraphModel>
  </diagram>
</mxfile>
```

The `style` attribute is a semicolon-separated `key=value` mini-DSL. Unknown keys are silently ignored — over-specifying is safe.

For AWS resource icons the canonical pattern is:

```
shape=mxgraph.aws4.resourceIcon;resIcon=mxgraph.aws4.<service>;fillColor=<aws-category-colour>;...
```

The stencil name in `resIcon` matches the `name` attribute on the `<shape>` element inside `stencils/aws4.xml`, with spaces replaced by underscores (e.g. `name="api gateway"` → `resIcon=mxgraph.aws4.api_gateway`).

## Stencil engine

`stencils/aws4.xml` is vendored as **data**, untouched from upstream. It contains 1037 `<shape>` definitions in mxStencil format. The renderer parses the subset of the DSL used by the shapes we currently support:

| Element              | Meaning                                  |
| -------------------- | ---------------------------------------- |
| `<path>`             | container for path commands              |
| `<move x= y=/>`      | moveTo                                   |
| `<line x= y=/>`      | lineTo                                   |
| `<curve x1= y1= x2= y2= x3= y3=/>` | cubic bezier                |
| `<quad x1= y1= x2= y2=/>` | quadratic bezier                    |
| `<close/>`           | close subpath                            |
| `<ellipse x= y= w= h=/>` | ellipse primitive                    |
| `<rect x= y= w= h=/>` | rectangle primitive                     |
| `<roundrect ... arcsize=/>` | rounded rectangle                 |
| `<fill/>` `<stroke/>` `<fillstroke/>` | paint operations          |

Stencil coordinates live in their own `(w, h)` system, declared on `<shape w= h=>`. The renderer linearly maps these to the destination cell's geometry with a small inset (currently 18%) so the glyph does not touch the tile edges.

Unrecognised commands are silently skipped. A `RenderError::UnsupportedStencilCmd` variant is wired up but not yet produced — this makes the engine forward-compatible at the cost of silent fidelity loss for stencils we have not tested.

## v0 scope

- **Authoring catalogue**: ~30 curated AWS resource-icon factories grouped by category, plus `Node::raw` for everything else:
  - Application Integration: `api_gateway`, `sqs`, `sns`, `eventbridge`, `step_functions`, `appsync`
  - Compute: `lambda`, `ec2`, `ecs`, `eks`, `fargate`, `app_runner`, `batch`
  - Database: `dynamodb`, `rds`, `elasticache`
  - Storage: `s3`, `efs`
  - Networking & Content Delivery: `cloudfront`, `route_53`, `vpc`, `elastic_load_balancing`
  - Security, Identity & Compliance: `iam`, `cognito`, `secrets_manager`, `kms`
  - Analytics: `kinesis`, `athena`, `msk`
  - Management & Governance: `cloudwatch`
- **Output format**: plain XML, `compressed="false"`, `html=0`.
- **Rendered shapes**: `mxgraph.aws4.resourceIcon` (proper coloured tile + stencil glyph). Anything else falls back to a plain coloured rectangle with the label.
- **Edges**: straight line between cell midpoints with a simple open arrowhead.
- **Stencil DSL**: the subset listed above.
- **CLI authoring**: small flat JSON schema with named factories
  (`aws.lambda`, `aws.api_gateway`, ...) and a `raw` escape hatch. See
  [`docs/authoring-schema.md`](authoring-schema.md).
- **Skill bundle**: a `skill/` directory at the repo root packages the
  CLI as a [Claude Code
  skill](https://docs.claude.com/en/docs/claude-code/skills). The bundle
  ships `SKILL.md` (trigger phrases + worked example), an `ensure.sh`
  install-check, and a round-trip smoke test. The skill drives
  `drawio-headless compose` (author + render in one shot) and
  `drawio-headless list-shapes` (factory discovery at runtime).

## Closed-loop measurements (v0 baseline)

From `cargo test --workspace`:

- PNG: 732 × 300, 31.5 KB
- Non-background pixels: 48,586
- AWS-orange pixels inside the 156×156 Lambda tile region: 22,423 / ~24,336 (~92%, with the white glyph cut out — exactly the expected ratio)

See `docs/sample-output.png`.

## Roadmap

No commitments on order or timeline.

- **More AWS services** in the curated catalogue as needs surface (the v0 ~25-service target is met).
- **Other stencil libraries**: Azure, GCP, Cisco, Kubernetes — vendor as additional XML files alongside `aws4.xml`.
- **Orthogonal edge routing** (`edgeStyle=orthogonalEdgeStyle`).
- **Connection-point snapping** (`entryX/exitX`).
- **More mxStencil commands** as we encounter stencils that need them.
- **Snapshot tests** with committed golden PNGs for visual regression.

## Licensing

Apache-2.0. The vendored `stencils/aws4.xml` is also Apache-2.0 (from `jgraph/drawio`); see `stencils/SOURCE` for the exact commit.
