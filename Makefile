.PHONY: build debug fmt clippy test check install uninstall clean

build:
	cargo build --release

debug:
	cargo build

fmt:
	cargo fmt --all

clippy:
	cargo clippy --workspace --all-targets -- -D warnings

test:
	cargo test --workspace

check: fmt clippy test

install:
	sudo packaging/install.sh

uninstall:
	sudo packaging/uninstall.sh

clean:
	cargo clean
