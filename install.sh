#!/usr/bin/env bash
# install.sh - Install drift-wallpaper
# From a checkout: builds from source with cargo.
# From anywhere else: downloads the latest release binary.
#
# Usage:
#   ./install.sh                         # in a clone → cargo build --release
#   DRIFT_USE_RELEASE=1 ./install.sh     # in a clone → download binary instead
#   curl -fsSL …/install.sh | bash       # download latest release
#
# Environment:
#   DRIFT_INSTALL_DIR    Installation directory (default: $HOME/.local/bin)
#   DRIFT_VERSION         Specific version to install (default: latest)
#   DRIFT_USE_RELEASE     Force release download even in a clone (set to 1)
#   DRIFT_NO_VERIFY       Skip SHA256 checksum verification (not recommended)

set -euo pipefail

REPO="undivisible/drift-wallpaper"
INSTALL_DIR="${DRIFT_INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${DRIFT_VERSION:-}"

# Colors for output
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[0;33m'; CYAN='\033[0;36m'; BOLD='\033[1m'; NC='\033[0m'

info()  { printf "${CYAN}%s${NC}\n" "$*"; }
ok()    { printf "${GREEN}✓ %s${NC}\n" "$*"; }
warn()  { printf "${YELLOW}! %s${NC}\n" "$*" >&2; }
die()   { printf "${RED}error: %s${NC}\n" "$*" >&2; exit 1; }

path_hint() {
  if command -v drift-wallpaper &>/dev/null 2>&1; then
    return
  fi
  printf "\n${BOLD}Add drift-wallpaper to your PATH:${NC}\n"
  case "${SHELL:-}" in
    */fish) printf '  fish_add_path %s\n' "$INSTALL_DIR" ;;
    *)      printf '  echo '\''export PATH="%s:$PATH"'\'' >> ~/.bashrc  # or ~/.zshrc\n' "$INSTALL_DIR" ;;
  esac
}

install_from_repo() {
  local root="$1"
  if ! command -v cargo &>/dev/null; then
    die "cargo not in PATH — install Rust from https://rustup.rs/ or set DRIFT_USE_RELEASE=1 to download a binary."
  fi
  info "Building drift-wallpaper from local checkout (${root})…"
  ( cd "$root" && cargo build --release -p drift-app --locked )
  mkdir -p "$INSTALL_DIR"
  cp "${root}/target/release/drift-wallpaper" "${INSTALL_DIR}/drift-wallpaper"
  chmod +x "${INSTALL_DIR}/drift-wallpaper"
  ok "drift-wallpaper built and installed to ${INSTALL_DIR}/drift-wallpaper"
  path_hint
}

install_from_release() {
  local OS ARCH os arch ASSET BASE_URL TMP TMP_SHA HAVE_SHA EXPECTED ACTUAL

  OS="$(uname -s)"
  ARCH="$(uname -m)"

  case "$OS" in
    Darwin)  os="macos" ;;
    Linux)   os="linux" ;;
    *)       die "Unsupported OS: $OS" ;;
  esac

  case "$ARCH" in
    x86_64|amd64)          arch="x86_64" ;;
    aarch64|arm64)         arch="aarch64" ;;
    *)                     die "Unsupported architecture: $ARCH" ;;
  esac

  ASSET="drift-wallpaper-${os}-${arch}"

  if [ -z "$VERSION" ]; then
    info "Fetching latest release version…"
    VERSION="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
      | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"\(.*\)".*/\1/')"
    [ -n "$VERSION" ] || die "Could not determine latest version from GitHub API"
  fi

  info "Installing drift-wallpaper ${VERSION} (${os}/${arch}) from GitHub Releases…"

  BASE_URL="https://github.com/${REPO}/releases/download/${VERSION}"
  TMP="$(mktemp)"
  TMP_SHA="$(mktemp)"
  trap 'rm -f "${TMP:-}" "${TMP_SHA:-}"' EXIT

  if command -v curl &>/dev/null; then
    curl -fsSL --progress-bar "${BASE_URL}/${ASSET}" -o "$TMP"
    HAVE_SHA=0
    if curl -fsSL "${BASE_URL}/${ASSET}.sha256" -o "$TMP_SHA" 2>/dev/null; then
      HAVE_SHA=1
    fi
  elif command -v wget &>/dev/null; then
    wget -q --show-progress "${BASE_URL}/${ASSET}" -O "$TMP"
    HAVE_SHA=0
    if wget -q "${BASE_URL}/${ASSET}.sha256" -O "$TMP_SHA" 2>/dev/null; then
      HAVE_SHA=1
    fi
  else
    die "curl or wget is required"
  fi

  if [ "$HAVE_SHA" -eq 1 ] && [ -s "$TMP_SHA" ]; then
    EXPECTED="$(tr -d '[:space:]' < "$TMP_SHA")"
    if command -v sha256sum &>/dev/null; then
      ACTUAL="$(sha256sum "$TMP" | awk '{print $1}')"
    elif command -v shasum &>/dev/null; then
      ACTUAL="$(shasum -a 256 "$TMP" | awk '{print $1}')"
    else
      die "sha256sum or shasum is required to verify release integrity"
    fi

    [ "$ACTUAL" = "$EXPECTED" ] || die "SHA256 mismatch — download may be corrupted or tampered with
  expected: $EXPECTED
  actual:   $ACTUAL"
    ok "checksum verified"
  elif [ "${DRIFT_NO_VERIFY:-}" = "1" ]; then
    warn "DRIFT_NO_VERIFY=1 set — skipping integrity verification"
  else
    die "No checksum file for ${VERSION}; set DRIFT_NO_VERIFY=1 to install without integrity verification"
  fi

  chmod +x "$TMP"
  mkdir -p "$INSTALL_DIR"
  mv "$TMP" "${INSTALL_DIR}/drift-wallpaper"

  ok "drift-wallpaper ${VERSION} installed to ${INSTALL_DIR}/drift-wallpaper"
  path_hint
}

# ---- repo vs release -------------------------------------------------------

_SRC="${BASH_SOURCE[0]:-}"
if [[ -n "$_src" ]] && [[ "$(basename -- "$_src")" == "install.sh" ]] && [[ "${DRIFT_USE_RELEASE:-}" != "1" ]]; then
  _root="$(cd "$(dirname -- "$_src")" && pwd)"
  if [ -f "${_root}/Cargo.toml" ] && grep -q 'name = "drift-app"' "${_root}/Cargo.toml" 2>/dev/null; then
    install_from_repo "$_root"
    exit 0
  fi
fi

install_from_release
