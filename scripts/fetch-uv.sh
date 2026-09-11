#!/usr/bin/env bash
set -euo pipefail

UV_VERSION=0.12.5
TARGET_TRIPLE=${1:-x86_64-unknown-linux-gnu}
ROOT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
BIN_DIR="$ROOT_DIR/crates/smabar/binaries"
STAMP="$BIN_DIR/.uv-version"

case "$TARGET_TRIPLE" in
  x86_64-unknown-linux-gnu)
    ASSET="uv-$TARGET_TRIPLE.tar.gz"
    BINARY=uv
    # Pinned in-repo: the release's own .sha256 would match a tampered
    # release asset. When bumping UV_VERSION, take the archive digest from
    # GitHub's release API, then extract that verified archive for the binary pin.
    ARCHIVE_SHA256="68a509da24b06b4223a1c0175fb5eb5bc79342b76cbeff0cfe51ac3f5b17b6b2"
    BINARY_SHA256="b65f23a420c4acc96427efb30e5ed9bc0f7e25d2d712000f6ede77c1a0de5f46"
    ;;
  x86_64-pc-windows-msvc)
    ASSET="uv-$TARGET_TRIPLE.zip"
    BINARY=uv.exe
    ARCHIVE_SHA256="4c4d49d8738847d9b71ba319e49a5688c93eac0fe6204b1df24e98528dddf39a"
    BINARY_SHA256="8da6cedef60c27ac997ebf400fbfc6d373c5b0a7ae6a299b9d52be7fe63723fb"
    ;;
  aarch64-apple-darwin)
    ASSET="uv-$TARGET_TRIPLE.tar.gz"
    BINARY=uv
    ARCHIVE_SHA256="5bb0e5fe008a773c3dbcb97ff79cd89e1241464fe9d2f986d52ad8f1b037bd62"
    BINARY_SHA256="ad3564874e19defa0debefcf48e8381ac1d087c584190c1323c247bd351dd25f"
    ;;
  x86_64-apple-darwin)
    ASSET="uv-$TARGET_TRIPLE.tar.gz"
    BINARY=uv
    ARCHIVE_SHA256="b3b2137477cf96c9686ebfb71524614cec780c673fd73e59bce099aef02e70e8"
    BINARY_SHA256="9f810c3f3ea6b29f5e02b03acb0dcff74f516d4c24b91cd5491f172829e5c9e4"
    ;;
  *)
    printf 'error: unsupported uv target triple: %s\n' "$TARGET_TRIPLE" >&2
    exit 2
    ;;
esac

DESTINATION="$BIN_DIR/uv-$TARGET_TRIPLE"
if [[ "$BINARY" == *.exe ]]; then
  DESTINATION+=.exe
fi
STAMP_ENTRY="$TARGET_TRIPLE $UV_VERSION"

sha256_file() {
  local output
  if command -v sha256sum >/dev/null 2>&1; then
    output=$(sha256sum -- "$1")
  elif command -v shasum >/dev/null 2>&1; then
    output=$(shasum -a 256 "$1")
  else
    printf 'error: sha256sum or shasum is required to verify uv\n' >&2
    exit 1
  fi
  printf '%s\n' "${output%% *}"
}

if [[ -f "$DESTINATION" && -f "$STAMP" ]] && grep -Fqx "$STAMP_ENTRY" "$STAMP"; then
  if [[ "$(sha256_file "$DESTINATION")" == "$BINARY_SHA256" ]]; then
    printf 'uv %s for %s is already present and verified\n' "$UV_VERSION" "$TARGET_TRIPLE"
    exit 0
  fi
  printf 'cached uv %s for %s failed checksum verification; downloading a clean copy\n' \
    "$UV_VERSION" "$TARGET_TRIPLE" >&2
fi

mkdir -p "$BIN_DIR"
TEMP_DIR=$(mktemp -d)
trap 'rm -rf -- "$TEMP_DIR"' EXIT

BASE_URL="https://github.com/astral-sh/uv/releases/download/$UV_VERSION"
curl -fsSL --retry 3 "$BASE_URL/$ASSET" -o "$TEMP_DIR/$ASSET"

# Verify against the pin recorded in this file — NOT the release's own
# .sha256 asset, which an attacker who replaced the release would have
# replaced as well.
if [[ "$(sha256_file "$TEMP_DIR/$ASSET")" != "$ARCHIVE_SHA256" ]]; then
  printf 'error: checksum verification failed for %s\n' "$ASSET" >&2
  exit 1
fi

mkdir "$TEMP_DIR/extracted"
case "$ASSET" in
  *.tar.gz)
    tar -xzf "$TEMP_DIR/$ASSET" -C "$TEMP_DIR/extracted"
    ;;
  *.zip)
    if command -v unzip >/dev/null 2>&1; then
      unzip -q "$TEMP_DIR/$ASSET" -d "$TEMP_DIR/extracted"
    else
      python -m zipfile -e "$TEMP_DIR/$ASSET" "$TEMP_DIR/extracted"
    fi
    ;;
esac

SOURCE=$(find "$TEMP_DIR/extracted" -type f -name "$BINARY" -print -quit)
if [[ -z "$SOURCE" ]]; then
  printf 'error: %s was not found in %s\n' "$BINARY" "$ASSET" >&2
  exit 1
fi
if [[ "$(sha256_file "$SOURCE")" != "$BINARY_SHA256" ]]; then
  printf 'error: extracted %s failed checksum verification\n' "$BINARY" >&2
  exit 1
fi
cp "$SOURCE" "$DESTINATION"
chmod 755 "$DESTINATION"

STAMP_TEMP="$TEMP_DIR/.uv-version"
if [[ -f "$STAMP" ]]; then
  while read -r recorded_target recorded_version; do
    if [[ "$recorded_target" != "$TARGET_TRIPLE" ]]; then
      printf '%s %s\n' "$recorded_target" "$recorded_version" >>"$STAMP_TEMP"
    fi
  done <"$STAMP"
fi
printf '%s\n' "$STAMP_ENTRY" >>"$STAMP_TEMP"
mv "$STAMP_TEMP" "$STAMP"
printf 'installed uv %s for %s at %s\n' "$UV_VERSION" "$TARGET_TRIPLE" "$DESTINATION"
