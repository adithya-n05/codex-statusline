.PHONY: check test coverage build

check:
	cargo fmt --all --check
	cargo clippy --workspace --all-targets --locked -- -D warnings
	$(MAKE) test
	$(MAKE) coverage

test:
	cargo test --workspace --locked --no-fail-fast

coverage:
	cargo coverage

build:
	cargo build --release --locked
