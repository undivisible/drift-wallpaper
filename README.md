# drift-wallpaper

Fluid live wallpaper for macOS, inspired by [sandydoo/flux](https://github.com/sandydoo/flux).

![CI](https://github.com/undivisible/drift-wallpaper/actions/workflows/ci.yml/badge.svg)

## Quick start

**Recommended:** use **[wax](https://github.com/semitechnological/wax)**:

```sh
wax install undivisible/tap/drift-wallpaper
```

**Manual install**

If you don't use wax, download and run the installer directly:

**macOS**

```sh
curl -fsSL https://raw.githubusercontent.com/undivisible/drift-wallpaper/m/scripts/install.sh | bash
```

From a local checkout (builds from source):

```sh
./scripts/install.sh
```

---

**Development builds**

For the latest unreleased changes, use `--head` or the `--version` flag:

```sh
wax install undivisible/tap/drift-wallpaper --head
```

```sh
DRIFT_USE_RELEASE=1 ./scripts/install.sh --version latest
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
