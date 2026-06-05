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

Merging to `main` releases — there is no manual tagging. On every push to
`main`, `.github/workflows/auto-release.yml`:

1. Computes the next version from the Conventional Commits since the last tag
   (`scripts/next-version.sh`: `fix:` → patch, `feat:` → minor, `!`/
   `BREAKING CHANGE:` → major; nothing releasable → it no-ops).
2. Bumps the version everywhere it lives — `Cargo.toml`, `Cargo.lock`,
   `npm/package.json` — via `scripts/bump-version.sh`.
3. Commits `chore(release): vX.Y.Z [skip-release]`, tags `vX.Y.Z`, pushes both.
4. Invokes `release.yml` (via `workflow_call`) to build the binary matrix,
   attach it to a GitHub Release, and publish the npm wrapper.

The `[skip-release]` marker on the bump commit stops the resulting push from
triggering another release (the workflow's top-level `if` guard skips it).

So commit messages drive the version. Use Conventional Commit prefixes; the PR
squash-merge subject is what `next-version.sh` reads.

### Trigger chain & required settings

- `auto-release.yml` calls `release.yml` directly via `workflow_call` **on
  purpose**: a tag pushed with the default `GITHUB_TOKEN` does *not* retrigger
  a workflow listening on `push: tags`, so a plain "push tag and hope
  release.yml fires" would silently never build. The direct call sidesteps that
  with no extra secret.
- **`NPM_TOKEN`** repo secret is required for the npm publish step. Without it,
  the GitHub Release (binaries) still publishes and the npm job logs a skip —
  the binary distribution works, npm just won't update.
- The bot push needs `contents: write` (set on the workflow) and the repo's
  Actions setting **"Allow GitHub Actions to create and approve pull requests"**
  is *not* needed (we push commits/tags, not PRs), but **"Read and write
  permissions"** must be allowed for `GITHUB_TOKEN`, or the per-workflow
  `permissions: contents: write` must be honoured (it is, by default).

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
