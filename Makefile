.PHONY: test
test:
	cargo fmt -- --check
	cargo-sort --check --workspace
	cargo clippy --all-features --workspace -- -D warnings
	cargo test --workspace

.PHONY: format
format:
	cargo fmt
	cargo-sort --workspace
