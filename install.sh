#!/bin/bash
# install.sh - Install drift-wallpaper
set -e

REPO="undivisible/drift-wallpaper"
INSTALL_DIR="${HOME}/.local/bin"
RELEASE_URL="https://github.com/${REPO}/releases/latest/download"

echo "Installing drift-wallpaper..."

# Detect platform
PLATFORM="$(uname -s)"
ARCH="$(uname -m)"

case "${PLATFORM}" in
    Darwin*)
        PLATFORM="macos"
        EXT="zip"
        ;;
    Linux*)
        PLATFORM="linux"
        EXT="tar.gz"
        ;;
    *)
        echo "Unsupported platform: ${PLATFORM}"
        exit 1
        ;;
esac

case "${ARCH}" in
    x86_64)
        ARCH="x86_64"
        ;;
    aarch64|arm64)
        ARCH="aarch64"
        ;;
    *)
        echo "Unsupported architecture: ${ARCH}"
        exit 1
        ;;
esac

ASSET_NAME="drift-wallpaper-${PLATFORM}-${ARCH}.${EXT}"
TMP_DIR=$(mktemp -d)
trap "rm -rf ${TMP_DIR}" EXIT

echo "Downloading ${ASSET_NAME}..."
curl -L -o "${TMP_DIR}/release.${EXT}" \
    "${RELEASE_URL}/${ASSET_NAME}"

mkdir -p "${INSTALL_DIR}"
cd "${TMP_DIR}"

if [ "${EXT}" = "zip" ]; then
    unzip -o "release.${EXT}"
else
    tar -xzf "release.${EXT}"
fi

cp -f drift-wallpaper "${INSTALL_DIR}/"
chmod +x "${INSTALL_DIR}/drift-wallpaper"

echo "Installed drift-wallpaper to ${INSTALL_DIR}/drift-wallpaper"
echo "Add ${INSTALL_DIR} to your PATH if needed."
