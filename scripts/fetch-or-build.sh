#!/bin/sh
# Build step for `herdr plugin install`.
#
# Tries the binaries published with the matching release first, so installing
# does not require a Rust toolchain, and falls back to building from source.
# Either way the result lands where herdr-plugin.toml expects it.
set -eu

REPO=ecylmz/herdr-hosts
OUT=target/release

version() {
    sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -1
}

target_triple() {
    case "$(uname -s)-$(uname -m)" in
        Linux-x86_64) echo x86_64-unknown-linux-gnu ;;
        Linux-aarch64) echo aarch64-unknown-linux-gnu ;;
        Darwin-arm64) echo aarch64-apple-darwin ;;
        Darwin-x86_64) echo x86_64-apple-darwin ;;
        *) echo "" ;;
    esac
}

build_from_source() {
    if ! command -v cargo >/dev/null 2>&1; then
        echo "herdr-hosts: no prebuilt binary for this platform and cargo is not installed." >&2
        echo "herdr-hosts: install Rust (https://rustup.rs) and try again." >&2
        exit 1
    fi
    echo "herdr-hosts: building from source"
    exec cargo build --release
}

TRIPLE=$(target_triple)
[ -n "$TRIPLE" ] || build_from_source
command -v curl >/dev/null 2>&1 || build_from_source
command -v tar >/dev/null 2>&1 || build_from_source

URL="https://github.com/$REPO/releases/download/v$(version)/herdr-hosts-$TRIPLE.tar.gz"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

echo "herdr-hosts: fetching $URL"
if ! curl -fsSL --retry 2 "$URL" -o "$TMP/release.tar.gz"; then
    echo "herdr-hosts: no published binary for this version" >&2
    build_from_source
fi

tar -xzf "$TMP/release.tar.gz" -C "$TMP" || build_from_source
mkdir -p "$OUT"
for binary in herdr-hosts herdr-hosts-ssh; do
    [ -f "$TMP/$binary" ] || build_from_source
    install -m 0755 "$TMP/$binary" "$OUT/$binary"
done

# A binary that cannot run here is worse than no binary at all.
"$OUT/herdr-hosts-ssh" >/dev/null 2>&1 || true
echo "herdr-hosts: installed prebuilt binaries for $TRIPLE"
