#!/usr/bin/env bash
# Build an optimized release binary and produce a compressed archive for
# distribution. The binary already embeds gzip-compressed tokenizers, so it is
# far smaller than a naive build; this script additionally packages it.
#
# Usage:
#   scripts/build-release.sh            # build for the host target
#   scripts/build-release.sh <target>   # e.g. x86_64-unknown-linux-musl
#
# Requires: cargo. Optionally `upx` for extra compression (see --upx).

set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

TARGET="${1:-}"
BIN="ctos"

if [[ -n "$TARGET" ]]; then
  cargo build --release --target "$TARGET"
  BIN_PATH="target/$TARGET/release/$BIN"
  ARCHIVE="ctos-$TARGET.tar.gz"
else
  cargo build --release
  BIN_PATH="target/release/$BIN"
  ARCHIVE="ctos-$(uname -m)-$(uname -s | tr 'A-Z' 'a-z').tar.gz"
fi

RAW_SIZE=$(du -h "$BIN_PATH" | cut -f1)
tar -czf "$ARCHIVE" -C "$(dirname "$BIN_PATH")" "$BIN"
GZ_SIZE=$(du -h "$ARCHIVE" | cut -f1)

echo "binary : $BIN_PATH  ($RAW_SIZE)"
echo "archive: $ARCHIVE  ($GZ_SIZE)"
