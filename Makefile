.PHONY: help fmt fmt-check check clippy test doc ci pre-commit run

help:
	@printf '%s\n' \
		'Targets:' \
		'  make fmt         Format all Rust code' \
		'  make fmt-check   Check Rust formatting' \
		'  make check       Type-check all features' \
		'  make clippy      Run strict clippy gates' \
		'  make test        Run all tests' \
		'  make doc         Build crate docs' \
		'  make ci          Run CI validation gates' \
		'  make pre-commit  Run local pre-commit gates' \
		'  make run         Run the binary locally'

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

check:
	cargo check --all-features

clippy:
	cargo clippy --all-targets --all-features -- -D warnings

test:
	cargo test --all-features

doc:
	cargo doc --no-deps --all-features

ci: fmt-check clippy test

pre-commit: fmt-check clippy test

run:
	cargo run
