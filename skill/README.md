# drawio-headless skill bundle

This directory packages `drawio-headless` as a [Claude Code
skill](https://docs.claude.com/en/docs/claude-code/skills) so an LLM can
author and render cloud architecture diagrams from natural-language
prompts.

## Install

Copy the directory into your Claude Code skills folder:

```sh
cp -r skill ~/.claude/skills/drawio-headless
```

Then verify the binary is on PATH:

```sh
bash ~/.claude/skills/drawio-headless/scripts/ensure.sh
```

If `drawio-headless` is missing, the script prints copy-pasteable
install instructions (it does *not* install for you — by design).

## What this skill does

When the user asks for a cloud architecture diagram, Claude emits a
small JSON spec and runs:

```sh
drawio-headless compose spec.json
# -> ./spec.svg
```

The output is a real SVG that round-trips through `app.diagrams.net` if
the user wants to edit it further (use `--keep-drawio <path>` to also
save the editable `.drawio` source).

See [`SKILL.md`](./SKILL.md) for the full skill manifest: trigger
phrases, the JSON authoring schema, a worked example, and common
pitfalls.

## Requirements

- The `drawio-headless` binary on PATH. Install with
  `cargo install --git https://github.com/mvhenten/drawio-headless --path crates/cli`.
- POSIX `sh` to run `scripts/ensure.sh` (no bash-isms).

## Test

A round-trip smoke test that exercises the same code paths an LLM would
hit:

```sh
bash test/round-trip.sh
```

The test composes a small AWS architecture from a JSON fixture and
asserts the output is a non-empty SVG starting with `<svg`.
