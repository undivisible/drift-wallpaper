# drift-wallpaper

Fluid live wallpaper for macOS, Windows, and Linux, inspired by [sandydoo/flux](https://github.com/sandydoo/flux).

![CI](https://github.com/undivisible/drift-wallpaper/actions/workflows/ci.yml/badge.svg)

## Quick start

Install the latest development build from the Homebrew tap:

```sh
brew install --HEAD undivisible/tap/drift-wallpaper
```

Or run the installer directly:

```sh
curl -fsSL https://raw.githubusercontent.com/undivisible/drift-wallpaper/m/scripts/install.sh | bash
```

On Windows, install from source with PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://raw.githubusercontent.com/undivisible/drift-wallpaper/m/scripts/install.ps1 | iex"
```

From a checkout, use the local installer:

```sh
./scripts/install.sh
```

Build from source manually:

```sh
cargo build --release -p drift-app

# Control panel (GPUI)
./target/release/drift-wallpaper --settings

# Desktop wallpaper windows
./target/release/drift-wallpaper --background

# Large movable preview
./target/release/drift-wallpaper --preview
```

Run `drift-wallpaper --help` for presets, image sources, and other flags.

On macOS, configuration is stored at:

`~/Library/Application Support/drift-wallpaper/config.json`

## Workspace

| Crate | Role |
|-------|------|
| `drift-core` | `wgpu` fluid simulation, WGSL shaders, color / presets |
| `drift-app` | `winit` wallpaper windows, GPUI settings UI, macOS desktop integration |

## Tests & CI

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked
```

## License

[MPL-2.0](LICENSE).
