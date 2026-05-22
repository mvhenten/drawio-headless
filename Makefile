.PHONY: build test fix lint examples

build:
	cargo build --workspace

test:
	cargo test --workspace

lint:
	cargo clippy --workspace --all-targets -- -D warnings

fix:
	cargo fmt --all
	cargo clippy --workspace --all-targets --fix --allow-dirty --allow-staged

examples:
	./scripts/build-examples.sh
