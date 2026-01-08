#!/bin/bash
set -e

# Function: get_wasm_opt
# Downloads the latest binaryen release for the current arch/os
# and returns the absolute path to the wasm-opt binary.
get_wasm_opt() {
    # 1. Detect Architecture
    local ARCH=$(uname -m)
    local OS=$(uname -s | tr '[:upper:]' '[:lower:]')
    local WASM_ARCH WASM_OS

    case "$ARCH" in
        x86_64)  WASM_ARCH="x86_64" ;;
        aarch64|arm64) WASM_ARCH="arm64" ;;
        *) echo "Unsupported architecture: $ARCH" >&2; return 1 ;;
    esac

    case "$OS" in
        linux)  WASM_OS="linux" ;;
        darwin) WASM_OS="macos" ;;
        *) echo "Unsupported OS: $OS" >&2; return 1 ;;
    esac

    # 2. Get latest version tag
    local LATEST_TAG=$(curl -Ls -o /dev/null -w %{url_effective} https://github.com/WebAssembly/binaryen/releases/latest | xargs basename)
    local TARGET_DIR="binaryen-$LATEST_TAG"
    local BIN_PATH="$(pwd)/$TARGET_DIR/bin/wasm-opt"

    # 3. Download and Clean if not already present
    if [ ! -d "$TARGET_DIR" ]; then
        local BINARYEN_PKG="binaryen-$LATEST_TAG-$WASM_ARCH-$WASM_OS.tar.gz"
        local URL="https://github.com/WebAssembly/binaryen/releases/download/$LATEST_TAG/$BINARYEN_PKG"

        curl -L -s -O "$URL"
        tar -xzf "$BINARYEN_PKG"
        rm "$BINARYEN_PKG"

        # Remove older versions
        for d in binaryen-*; do
            if [ "$d" != "$TARGET_DIR" ] && [ -d "$d" ]; then
                rm -rf "$d"
            fi
        done
    fi

    # Return the path to the binary
    echo "$BIN_PATH"
}

cargo build --profile wasm-release -F webgpu --target wasm32-unknown-unknown

echo "bindgening wasm"
RUST_WASM=target/wasm32-unknown-unknown/wasm-release/galaxy-cats.wasm
rm -rf dist
wasm-bindgen --no-typescript --target web --out-dir ./dist $RUST_WASM

BINDGEN_WASM=dist/galaxy-cats_bg.wasm
WASM_OPT=$(get_wasm_opt)
HASH=$(git rev-parse HEAD)

echo "wasm-opt-ing the wasm"
$WASM_OPT -Oz $BINDGEN_WASM -o dist/galaxy-cats-$HASH.wasm

rm -f $BINDGEN_WASM
sed "s/{git-hash-here}/$HASH/g" template.html > dist/index.html
echo "Built and optimized wasm & web!"
