#!/bin/sh
# Builds target/machine/<platform>/{asterismd,asterism} for the platforms the daemon installs on machines.
set -eu
cd "$(dirname "$0")/.."

RUST_IMAGE=rust:1.99.1-alpine
BINS="asterismd asterism"

# ponytail: amd64 builds run emulated on Apple silicon and are slow; switch to cargo-zigbuild if this gets in the way.
for pair in amd64:x86_64 arm64:aarch64; do
	docker_arch=${pair%%:*}
	arch=${pair#*:}
	docker run --rm --platform "linux/$docker_arch" \
		-v "$PWD":/src -v "asterism-machine-cargo-$arch":/usr/local/cargo/registry -w /src \
		"$RUST_IMAGE" sh -c "apk add --no-cache build-base && cargo build -p asterism --bin asterismd --bin asterism --release --target-dir target/linux-$arch"
	mkdir -p "target/machine/linux-$arch"
	for bin in $BINS; do cp "target/linux-$arch/release/$bin" "target/machine/linux-$arch/"; done
done

cargo build -p asterism --bin asterismd --bin asterism
# A cargo config or CARGO_TARGET_DIR may move the host build out of ./target.
host_target=$(cargo metadata --format-version 1 --no-deps | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')
case "$(uname -s)-$(uname -m)" in
	Darwin-arm64) host=darwin-aarch64 ;;
	Darwin-x86_64) host=darwin-x86_64 ;;
	*) exit 0 ;;
esac
mkdir -p "target/machine/$host"
for bin in $BINS; do cp "$host_target/debug/$bin" "target/machine/$host/"; done
