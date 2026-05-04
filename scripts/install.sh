#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/install.sh [--homebrew|--source]

Options:
  --homebrew  Install latest development build from undivisible/tap.
  --source    Build this checkout and install the binary to ~/.local/bin.

Default: --homebrew when brew is available, otherwise --source.
USAGE
}

repo_url="${DRIFT_REPO_URL:-https://github.com/undivisible/drift-wallpaper.git}"
branch="${DRIFT_BRANCH:-m}"
mode="${1:-auto}"
case "$mode" in
  auto | --homebrew | --source) ;;
  -h | --help)
    usage
    exit 0
    ;;
  *)
    usage >&2
    exit 2
    ;;
esac

if [[ "$mode" == "auto" ]]; then
  if command -v brew >/dev/null 2>&1; then
    mode="--homebrew"
  else
    mode="--source"
  fi
fi

if [[ "$mode" == "--homebrew" ]]; then
  if ! command -v brew >/dev/null 2>&1; then
    echo "Homebrew not found. Re-run with --source or install Homebrew first." >&2
    exit 1
  fi

  brew tap undivisible/tap
  brew install --HEAD undivisible/tap/drift-wallpaper
  exit 0
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo not found. Install Rust or re-run with --homebrew." >&2
  exit 1
fi

install_dir="${DRIFT_INSTALL_DIR:-$HOME/.local/bin}"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]:-.}")/.." 2>/dev/null && pwd || true)"

if [[ -n "$repo_root" && -f "$repo_root/crates/drift-app/Cargo.toml" ]]; then
  cd "$repo_root"
  cargo build --release -p drift-app --locked
  mkdir -p "$install_dir"
  install -m 0755 target/release/drift-wallpaper "$install_dir/drift-wallpaper"

  echo "Installed drift-wallpaper to $install_dir/drift-wallpaper"
  exit 0
fi

cargo install --git "$repo_url" --branch "$branch" drift-app --locked --force
