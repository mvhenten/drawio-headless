---
name: drawio-headless
description: |
  Author and render cloud architecture diagrams (AWS, Azure, GCP,
  Kubernetes) from a natural-language description. Use this skill whenever
  the user asks to "draw an AWS architecture", "create a cloud diagram",
  "diagram with AWS / Azure / GCP", or asks for an architecture diagram
  from natural language. The skill invokes the `drawio-headless` CLI,
  which produces a real `.drawio` file (round-trips through
  app.diagrams.net) and an SVG/PNG rendering — no browser required.
---

# drawio-headless skill

## When to invoke

Trigger phrases that match this skill:

- "draw an AWS architecture for ..."
- "create a cloud diagram with ..."
- "diagram with AWS / Azure / GCP / Kubernetes ..."
- "architecture diagram from this description ..."
- explicit mentions of `drawio`, `.drawio`, or "draw.io"

The skill is appropriate when the user wants a **visual** representation of
named cloud components and their connections. It is *not* appropriate for
prose flowcharts, sequence diagrams, or freehand sketches — those are
better served by other tools.

## How it works

The user describes an architecture. You (the model) emit a small JSON
spec, then call `drawio-headless compose` to produce an SVG.

```
natural language  ─►  JSON spec  ─►  drawio-headless compose  ─►  diagram.svg
```

### Prerequisite

The `drawio-headless` binary must be on PATH. Run the bundled check:

```sh
bash scripts/ensure.sh
```

It prints install instructions if the binary is missing. There is no
automatic install — the script just guides.

## The JSON authoring schema

The spec is intentionally flat — a `name`, an array of `nodes`, an array
of `edges`, and an optional array of `groups` (containers like VPCs,
accounts). Every node has a unique `id`, a `kind` (e.g. `aws.lambda`),
optional `label`, and `x`/`y` coordinates in drawio user units. Edges
reference node ids.

```json
{
  "name": "MyArch",
  "nodes": [
    {"id": "api", "kind": "aws.api_gateway", "label": "API", "x": 80,  "y": 80},
    {"id": "lam", "kind": "aws.lambda",      "label": "Logic", "x": 320, "y": 80},
    {"id": "db",  "kind": "aws.dynamodb",    "label": "Orders", "x": 560, "y": 80}
  ],
  "edges": [
    {"source": "api", "target": "lam"},
    {"source": "lam", "target": "db"}
  ]
}
```

Full schema reference (top-level fields, group spec, edge spec, error
handling): see [`docs/authoring-schema.md`](../docs/authoring-schema.md)
in the `drawio-headless` repository.

### Layout coordinates — quick guidance

Coordinates are top-left of the node in user units. Tiles default to
78x78 (AWS) or 50x50 (Azure / GCP / k8s). A clean linear flow is around
240 user units between centres, so `x` of `80, 320, 560, 800, ...` works
well for a row of AWS tiles. For trees and meshes, give yourself ~200
units of vertical separation between layers.

## Layout rules (non-negotiable)

These are the rules that separate a diagram that merely renders from one
that reads correctly. Full field reference:
[`docs/authoring-schema.md`](../docs/authoring-schema.md).

1. **No empty boxes.** Every node needs a real catalogue `kind`. A `raw`
   node or an unrecognised `kind` draws the gray fallback box — that is
   the failure mode, not a style choice. If no glyph is an exact match,
   pick the closest metaphor (a globe for a generic client) rather than
   leaving a node bare. Some Azure glyphs currently render as thin,
   near-invisible outlines instead of a solid fill (tracked in
   [#29](https://github.com/mvhenten/drawio-headless/issues/29)) — render
   a PNG and check before trusting an Azure kind for fidelity.
2. **Arrows arrive head-on — the L-bend routing rule.** Each edge is
   either a straight line (source and target already colinear) or one
   automatic L-bend; there are no manual waypoints. The exit side always
   determines the final segment's orientation:
   - exit top/bottom → final segment is vertical → must enter the
     target's **top/bottom** to land head-on
   - exit left/right → final segment is horizontal → must enter the
     target's **left/right** to land head-on
   Pin both ends explicitly with `exit_x`/`exit_y` and
   `entry_x`/`entry_y` (each `0..1`; both members of a pair are required
   or the pair is ignored). An arrowhead that slides along a box edge
   instead of landing on it means the exit/entry sides contradict the
   nodes' actual relative position — fix the anchors or move the node,
   don't leave it.
3. **Align nodes on shared axes.** A flow meant to read as one lane
   (A → B → C) needs its nodes sharing one `x` (vertical lane) or one `y`
   (horizontal lane) so the connecting edges run straight rather than on
   a diagonal or a needless L-bend.
4. **No edge labels — use lane separation instead.** This tool has no
   edge-label rendering. Distinguish parallel paths by placing them on
   separate visual lanes and by which nodes appear on each path (e.g. an
   authorizer only on the write lane), not by labelling the arrow. When
   two edges enter the same side of a node, offset `entry_y` (e.g. 0.3
   and 0.7) so they don't overlap.
5. **Groups carry meaning.** Name group containers in domain language
   ("Payments VPC", "read replica region"), not layout jargon ("Group
   1", "container").

### Verify before calling it done

Render a PNG and **look at it** — the SVG/`.drawio` XML alone will not
show you a gray fallback box or a misrouted arrow:

```sh
drawio-headless compose spec.json /tmp/check.png --format png
```

Check: no gray fallback boxes, every label present, every arrowhead
perpendicular to (not sliding along) the edge it lands on, lanes read
straight. As a rough sanity check on the SVG output: a real glyph-bearing
SVG runs tens of KB; a suspiciously tiny SVG usually means glyphs failed
to embed.

## Discovering available shapes at runtime

To see every factory `kind` the CLI accepts, call:

```sh
drawio-headless list-shapes --format json
```

Output is a flat JSON array — one object per factory:

```json
[
  {"library": "aws",   "key": "lambda",       "category": "Compute"},
  {"library": "azure", "key": "sql_database", "category": "Database"},
  {"library": "gcp",   "key": "bigquery",     "category": "Big Data"},
  {"library": "k8s",   "key": "pod",          "category": "Workloads"}
]
```

Use this when the user names a service you don't immediately recognise.
The qualified kind is `<library>.<key>`. Filter to one library with
`--library aws|azure|gcp|k8s` if the user is sticking to a single
provider.

## Worked example

**User**: "draw an AWS lambda behind an API gateway, writing to dynamodb."

**You construct** (in a turn-internal scratchpad, not surfaced to the user):

```json
{
  "name": "ApiLambdaDynamo",
  "nodes": [
    {"id": "api", "kind": "aws.api_gateway", "label": "API",      "x": 80,  "y": 80},
    {"id": "lam", "kind": "aws.lambda",      "label": "Handler",  "x": 320, "y": 80},
    {"id": "db",  "kind": "aws.dynamodb",    "label": "Orders",   "x": 560, "y": 80}
  ],
  "edges": [
    {"source": "api", "target": "lam"},
    {"source": "lam", "target": "db"}
  ]
}
```

**You invoke** (writing the JSON to a temp file, then running):

```sh
drawio-headless compose api-lambda-dynamo.json
# emits ./api-lambda-dynamo.svg
```

For PNG instead of SVG:

```sh
drawio-headless compose api-lambda-dynamo.json --format png
# emits ./api-lambda-dynamo.png
```

To also keep the `.drawio` source for the user to edit in
app.diagrams.net:

```sh
drawio-headless compose api-lambda-dynamo.json out.svg \
    --keep-drawio out.drawio
```

You can also pipe the JSON in via stdin:

```sh
echo "$JSON" | drawio-headless compose --stdin > out.svg
```

**You return** the path to the user (and optionally inline the SVG).

## Common pitfalls

- **Unknown service names.** Always run `list-shapes --format json` once
  to confirm a `kind` exists before composing. The CLI rejects unknown
  kinds with a "did you mean ...?" hint, but discovery up front is
  cheaper than a failed compose.
- **AWS Lambda is `aws.lambda`, not `aws.aws_lambda`.** The `library`
  prefix and the function key are joined with a single dot.
- **Some logical services have unexpected stencil keys.** GCP Cloud
  Functions is `gcp.cloud_functions` (plural). Azure App Service is
  `azure.website` (legacy stencil name). Always defer to
  `list-shapes`.
- **Edges need both endpoints to exist.** A typo in `source` or
  `target` causes a compose error. Validate ids match a declared
  `node.id`.
- **Group containers are placed behind nodes.** Their bounding box just
  needs to enclose the child tiles geometrically — the renderer infers
  containment.

## CLI surface (cheat sheet)

```text
drawio-headless compose <input.json> [<output>]
    [--format svg|png] [--keep-drawio <path>] [--stdin]
drawio-headless author <input.json> [<output.drawio>] [--stdin]
drawio-headless render <input.drawio> [<output>]
    [--format svg|png] [--stdin]
drawio-headless list-shapes
    [--library aws|azure|gcp|k8s|all]
    [--format text|json]
```

Errors are single-line with a stable `error: ...` prefix — safe to
surface verbatim in your reply to the user.
