#!/bin/sh
set -eu

REPO="fschutt/git2pdf"
VERSION="0.1.0"
BASE_URL="https://github.com/${REPO}/releases/download/${VERSION}"
INSTALL_DIR="/usr/local/bin"

main() {
    asset_name="$(detect_asset)"
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT

    echo "Downloading git2pdf ${VERSION} for $(uname -s) $(uname -m)..."
    download "${BASE_URL}/${asset_name}" "${tmp}/git2pdf"
    chmod +x "${tmp}/git2pdf"

    if [ -w "${INSTALL_DIR}" ]; then
        mv "${tmp}/git2pdf" "${INSTALL_DIR}/git2pdf"
    else
        echo "Installing to ${INSTALL_DIR} (requires sudo)..."
        sudo mv "${tmp}/git2pdf" "${INSTALL_DIR}/git2pdf"
    fi

    echo "git2pdf ${VERSION} installed to ${INSTALL_DIR}/git2pdf"
}

detect_asset() {
    os="$(uname -s)"
    case "${os}" in
        Linux)  echo "git2pdf-linux-x64" ;;
        Darwin) echo "git2pdf-macos-x64" ;;
        *)      echo "Unsupported OS: ${os}" >&2; exit 1 ;;
    esac
}

download() {
    if command -v curl > /dev/null 2>&1; then
        curl -fSL -o "$2" "$1"
    elif command -v wget > /dev/null 2>&1; then
        wget -qO "$2" "$1"
    else
        echo "Error: curl or wget required" >&2; exit 1
    fi
}

main
