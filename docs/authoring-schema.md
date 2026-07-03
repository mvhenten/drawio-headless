# Authoring schema

The `drawio-headless author` subcommand reads a small JSON schema and emits a
`.drawio` XML file by driving the `drawio-author` library. The schema is
intentionally flat, named-factory based, and free of nested DSLs — designed
to be trivial for an LLM (or a human) to emit correctly on the first try.

## Worked example

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

Run:

```sh
drawio-headless author input.json output.drawio
# or
cat input.json | drawio-headless author --stdin > output.drawio
```

For the common "JSON in, SVG out" flow, `compose` skips the intermediate
file entirely:

```sh
drawio-headless compose input.json            # writes ./input.svg
drawio-headless compose input.json out.svg
drawio-headless compose input.json out.png --format png
drawio-headless compose input.json out.svg --keep-drawio out.drawio
cat input.json | drawio-headless compose --stdin > out.svg
```

`compose` consumes the exact same JSON schema documented below.

The output is byte-identical to what the equivalent Rust code would produce:

```rust
let mut d = Diagram::new("ApiLambda");
let api = d.add_node(aws::api_gateway("api", "API Gateway").at(80.0, 80.0));
let lam = d.add_node(aws::lambda("lam", "Lambda").at(320.0, 80.0));
d.connect(&api, &lam);
let xml = d.to_xml();
```

## Top-level fields

| Field    | Type                | Required | Default       | Notes |
| -------- | ------------------- | -------- | ------------- | ----- |
| `name`   | string              | no       | `"Diagram"`   | Diagram page name. |
| `groups` | array of group spec | no       | `[]`          | Rendered behind nodes. |
| `nodes`  | array of node spec  | no       | `[]`          | The shapes. |
| `edges`  | array of edge spec  | no       | `[]`          | Connections between nodes. |

Unknown top-level keys are rejected.

## Node spec

| Field    | Type    | Required | Default | Notes |
| -------- | ------- | -------- | ------- | ----- |
| `id`     | string  | yes      | —       | Unique within the document. |
| `kind`   | string  | yes      | —       | See *Node kinds* below. |
| `label`  | string  | no       | `""`    | Visible label. |
| `x`      | number  | yes      | —       | Top-left x in user units. |
| `y`      | number  | yes      | —       | Top-left y in user units. |
| `width`  | number  | no       | `78`    | Node width. |
| `height` | number  | no       | `78`    | Node height. |
| `style`  | string  | required for `kind=raw` | — | Verbatim drawio style string. Ignored otherwise. |

### Node kinds

Factories are namespaced as `<library>.<key>`, where `<library>` is one
of `aws`, `azure`, `gcp`, `k8s`, `client`, or `generic` and `<key>` matches
the function name in the corresponding `drawio-author` module. `client`
and `generic` are vendor-neutral: browsers/mobile apps/people/external
systems, and cloud/database/queue/document shapes respectively — for the
parts of a diagram that aren't any particular vendor's service. Use `raw`
to bypass the catalogue and supply your own `style` string.

The full catalogue is discoverable at runtime — no need to memorise it:

```sh
drawio-headless list-shapes --format json
# -> [{"library":"aws","key":"lambda","category":"Compute"}, ...]

drawio-headless list-shapes --library aws --format text
# human-friendly listing grouped by category
```

Indicative members (verify against `list-shapes` for the current set):

- **AWS** (~30): `aws.api_gateway`, `aws.lambda`, `aws.s3`,
  `aws.dynamodb`, `aws.ec2`, `aws.sqs`, `aws.sns`, `aws.cloudfront`,
  `aws.msk`, `aws.iam`, `aws.vpc`, `aws.eventbridge`,
  `aws.step_functions`, `aws.appsync`, `aws.ecs`, `aws.eks`,
  `aws.fargate`, `aws.app_runner`, `aws.batch`, `aws.rds`,
  `aws.elasticache`, `aws.efs`, `aws.route_53`,
  `aws.elastic_load_balancing`, `aws.cognito`, `aws.secrets_manager`,
  `aws.kms`, `aws.kinesis`, `aws.athena`, `aws.cloudwatch`.
- **Azure** (19): `azure.active_directory`, `azure.entra_id`,
  `azure.multi_factor_authentication`, `azure.sql_database`,
  `azure.service_bus`, `azure.storage_blob`, `azure.virtual_machine`,
  `azure.virtual_network`, `azure.website`, `azure.cloud_service`,
  `azure.cdn`, `azure.express_route`, `azure.notification_hub`,
  `azure.traffic_manager`, `azure.cache`, `azure.load_balancer`,
  `azure.storage_queue`, `azure.server`, `azure.storage`.
- **GCP** (15): `gcp.app_engine`, `gcp.cloud_functions`,
  `gcp.compute_engine`, `gcp.gke`, `gcp.cloud_storage`, `gcp.bigquery`,
  `gcp.pubsub`, `gcp.cloud_sql`, `gcp.cloud_datastore`, `gcp.bigtable`,
  `gcp.cloud_cdn`, `gcp.cloud_load_balancing`, `gcp.cloud_dns`,
  `gcp.iam`, `gcp.logging`.
- **Kubernetes** (10): `k8s.pod`, `k8s.deployment`, `k8s.replica_set`,
  `k8s.service`, `k8s.ingress`, `k8s.config_map`, `k8s.secret`,
  `k8s.namespace`, `k8s.node`, `k8s.persistent_volume`.
- **Client** (4, vendor-neutral): `client.browser`, `client.mobile`,
  `client.person`, `client.external_system`.
- **Generic** (4, vendor-neutral): `generic.cloud`, `generic.database`,
  `generic.queue`, `generic.document`.

Unknown kinds are rejected with the closest catalogue matches surfaced as a
"did you mean ...?" hint.

## Group spec

Groups are container rectangles drawn behind the nodes. Children are inferred
by geometric containment at render time — placing a group's bounding box
around the nodes that belong to it is enough.

| Field    | Type    | Required | Default | Notes |
| -------- | ------- | -------- | ------- | ----- |
| `id`     | string  | yes      | —       | Unique within the document. |
| `kind`   | string  | yes      | —       | One of `aws.account`, `aws.vpc`, `aws.cloud`, `generic`. |
| `label`  | string  | no       | `""`    | Visible label (top-left). |
| `x`      | number  | yes      | —       | Top-left x. |
| `y`      | number  | yes      | —       | Top-left y. |
| `width`  | number  | yes      | —       | Container width. |
| `height` | number  | yes      | —       | Container height. |

## Edge spec

| Field     | Type    | Required | Notes |
| --------- | ------- | -------- | ----- |
| `source`  | string  | yes      | Must reference a node `id`. |
| `target`  | string  | yes      | Must reference a node `id`. |
| `exit_x`  | number  | no       | `0..=1`; pin source-side attachment x. |
| `exit_y`  | number  | no       | `0..=1`; pin source-side attachment y. |
| `entry_x` | number  | no       | `0..=1`; pin target-side attachment x. |
| `entry_y` | number  | no       | `0..=1`; pin target-side attachment y. |

`exit_x`/`exit_y` (and `entry_x`/`entry_y`) are honoured only when both
members of the pair are set; otherwise the renderer's points-based picker
chooses. Values outside `[0, 1]` are clamped, matching drawio's behaviour.

## Errors

The subcommand exits non-zero with a message on stderr for:

- Invalid JSON (unknown fields, missing required fields, type mismatches).
- Unknown `kind` (with close-match suggestions when available).
- A `raw` node missing `style`.
- An edge referencing an `id` that no node declared.
- Two nodes / groups sharing an `id`.

## Why JSON?

JSON was picked over TOML and YAML for three reasons:

1. **LLM-friendliness.** Every modern LLM emits JSON fluently and many have a
   strict JSON output mode. TOML and YAML are tolerated but error-prone at
   the margins (significant whitespace, multi-line strings, comment quoting).
2. **Dependency weight.** `serde` + `serde_json` are the de-facto Rust parsing
   stack — already on every machine that builds the project. TOML and YAML
   bring incremental crates (`toml`, `serde_yaml` or `yaml-rust`) that pull
   their own transitive trees.
3. **Schema clarity.** The drawio data model is shape-heavy and nesting-light;
   JSON's arrays-of-objects shape matches it directly. TOML's table-array
   syntax (`[[nodes]]`) reads worse for this; YAML's flexibility creates
   ambiguity (strings vs. numbers vs. booleans for `id` values like `"on"`).
