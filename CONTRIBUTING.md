# Contributing

Thanks for considering a contribution. This file covers the bits that aren't
obvious from `README.md`.

## Quality gates

Every PR must keep these green:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

GitHub Actions runs the full suite on every push.

## Releasing

Merging to `main` releases automatically -- `.github/workflows/release.yml`
computes the next version from Conventional Commit prefixes (`fix:` ->
patch, `feat:` -> minor, `!`/`BREAKING CHANGE:` -> major) since the last tag,
then builds, tags, and publishes. A push with no `feat`/`fix`/breaking commits
skips the release. Push a `v*.*.*` tag by hand to force a specific version.

## Snapshot tests (visual regression)

`crates/closed-loop-test/tests/snapshots.rs` renders a handful of fixed
diagrams to PNG and pixel-diffs them against committed goldens in
`crates/closed-loop-test/tests/snapshots/`. This catches "did the *right*
thing render" rather than just "did anything render".

### Running

The snapshot tests run as part of the normal test suite — no extra flag
needed:

```sh
cargo test --workspace
```

On failure, the actual render and a per-pixel diff image are written to
`target/test-output/snapshots-diff/<name>.{actual.png,diff.png}` so you can
eyeball what changed.

### Regenerating goldens after an intentional visual change

If you've made a deliberate change that legitimately alters the rendered
output (e.g. tweaking edge routing, fixing a stencil), regenerate the
goldens and commit the new PNGs:

```sh
# Overwrites tests/snapshots/*.png with the current render output.
INSTA_UPDATE=1 cargo test -p closed-loop-test

# Inspect the new PNGs, then stage and commit them.
git add crates/closed-loop-test/tests/snapshots/
git commit -m "test: refresh golden snapshots"
```

The env var is called `INSTA_UPDATE` for familiarity with the `insta`
crate's convention — but this project rolls its own ~30-line pixel-diff
harness rather than pulling in `insta`.

### Tolerances

Tunable as constants at the top of `crates/closed-loop-test/tests/snapshots.rs`:

- `MAX_CHANNEL_DELTA` (default `5`): per-channel `(r,g,b)` delta above
  which a pixel is counted as differing. Absorbs harmless rounding noise
  from resvg's vector rasteriser.
- `MAX_DIFF_FRACTION` (default `0.005`, i.e. 0.5%): maximum fraction of
  pixels allowed to differ before a snapshot fails.

### Determinism note

The snapshot rasteriser deliberately does **not** load system fonts, which
causes `resvg` to drop all `<text>` elements. Snapshots therefore only
contain geometric stencils, edges and group boundaries — the parts of the
render that are pixel-deterministic across machines. Font rasterisation
under `fontdb` is the main source of cross-machine jitter and is
intentionally excluded.

If you add a new snapshot that *does* need to assert label rendering, you
will need to bump `MAX_DIFF_FRACTION` substantially (or load a vendored
font) — open an issue first to discuss.
