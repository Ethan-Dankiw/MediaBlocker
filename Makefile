all: build-release run

clean:
	cargo clean

build-release:
	cargo build --release --package MediaBlocker --bin MediaBlocker

run:
	./target/release/MediaBlocker -C 2

.PHONY: profile build-debug flamegraph

profile: flamegraph

build-debug:
	cargo build --release --package MediaBlocker --bin MediaBlocker

flamegraph: build-debug
	cargo flamegraph --release --bin MediaBlocker --freq 4000 -- -C 2