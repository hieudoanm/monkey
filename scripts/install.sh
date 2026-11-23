#!/usr/bin/env bash
#
# install.sh — Install Monkey from the latest GitHub release.
#
# Installs, by default:
#   1. the `monkey` CLI binary into ~/.local/bin
#   2. the VS Code extension (.vsix) when the `code` CLI is available
#   3. the Zed extension source tarball into ~/.local/share/monkey/
#
# Usage:
#   ./scripts/install.sh [--cli-only | --editor-only | --zed-only]
set -euo pipefail

REPO="hieudoanm/monkey"
BASE_URL="https://github.com/${REPO}/releases/latest/download"

MODE="all"
case "${1:-}" in
    --cli-only)    MODE="cli" ;;
    --editor-only) MODE="editor" ;;
    --zed-only)    MODE="zed" ;;
    "" ) ;;
    *) echo "usage: $0 [--cli-only | --editor-only | --zed-only]" >&2; exit 2 ;;
esac

want_cli()    { [ "$MODE" = "all" ] || [ "$MODE" = "cli" ]; }
want_editor() { [ "$MODE" = "all" ] || [ "$MODE" = "editor" ]; }
want_zed()    { [ "$MODE" = "all" ] || [ "$MODE" = "zed" ]; }

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

# ---------- CLI ----------
install_cli() {
    local os arch asset
    os="$(uname -s | tr '[:upper:]' '[:lower:]')"
    arch="$(uname -m)"
    case "$os/$arch" in
        darwin/arm64|darwin/aarch64) asset="cli-monkey-aarch64-apple-darwin" ;;
        darwin/x86_64)
            echo "Intel macOS: no prebuilt release — build locally: cd crates && cargo build --release" >&2
            return 1 ;;
        linux/x86_64)                asset="cli-monkey-x86_64-unknown-linux-gnu" ;;
        mingw*|msys*|cygwin*|windows*)
            echo "Windows: download cli-monkey-x86_64-pc-windows-msvc.exe manually" >&2
            return 1 ;;
        *)
            echo "unsupported platform: $os/$arch" >&2
            return 1 ;;
    esac

    echo "Downloading ${asset}..."
    if ! curl -fsSL "${BASE_URL}/${asset}" -o "$TMP_DIR/monkey"; then
        echo "error: could not download ${BASE_URL}/${asset}" >&2
        echo "hint: no prebuilt release yet — build locally: cd crates && cargo build --release" >&2
        return 1
    fi

    local bin_dir="$HOME/.local/bin"
    mkdir -p "$bin_dir"
    install -m 0755 "$TMP_DIR/monkey" "$bin_dir/monkey"
    echo "monkey CLI installed to ${bin_dir}/monkey"

    case ":$PATH:" in
        *":$bin_dir:"*) ;;
        *) echo "note: ${bin_dir} is not on your PATH — add: export PATH=\"${bin_dir}:\$PATH\"" ;;
    esac
}

# ---------- VS Code ----------
install_vscode() {
    if ! command -v code >/dev/null 2>&1; then
        echo "VS Code CLI ('code') not found — skipping extension install."
        echo "  Install manually: ${BASE_URL}/extension-vscode-monkey-c.vsix"
        return 0
    fi

    echo "Downloading extension-vscode-monkey-c.vsix..."
    if ! curl -fsSL "${BASE_URL}/extension-vscode-monkey-c.vsix" -o "$TMP_DIR/monkey-c.vsix"; then
        echo "error: could not download ${BASE_URL}/extension-vscode-monkey-c.vsix" >&2
        return 1
    fi
    code --install-extension "$TMP_DIR/monkey-c.vsix" --force
    echo "monkey-c installed into VS Code"
}

# ---------- Zed ----------
install_zed() {
    local dest="$HOME/.local/share/monkey/zed-extension"
    echo "Downloading extension-zed-monkey-c.tar.gz..."
    if ! curl -fsSL "${BASE_URL}/extension-zed-monkey-c.tar.gz" -o "$TMP_DIR/monkey-c-zed.tar.gz"; then
        echo "error: could not download ${BASE_URL}/extension-zed-monkey-c.tar.gz" >&2
        return 1
    fi
    mkdir -p "$dest"
    tar -xzf "$TMP_DIR/monkey-c-zed.tar.gz" -d "$dest"
    cat <<EOF
Zed extension source extracted to:
  ${dest}

Install it in Zed with:  zed: install ${dest}
(Or copy it into your Zed extensions directory.)
EOF
}

rc=0
want_cli    && install_cli    || true
want_editor && install_vscode || true
want_zed    && install_zed    || true
exit $rc
