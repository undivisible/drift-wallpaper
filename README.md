# drift-wallpaper

Fluid live wallpaper inspired by [sandydoo/flux](https://github.com/sandydoo/flux). **macOS** is the primary, fully integrated target; **Linux** (X11) and **Windows** ship release binaries with wallpaper support (tray, media art, and some macOS-only integrations differ by platform).

![CI](https://github.com/undivisible/drift-wallpaper/actions/workflows/ci.yml/badge.svg)

## Quick start

**Recommended (macOS):** use **[wax](https://github.com/semitechnological/wax)**:

```sh
wax install undivisible/tap/drift-wallpaper
```

### Manual install

**macOS**

```sh
curl -fsSL https://raw.githubusercontent.com/undivisible/drift-wallpaper/m/scripts/install.sh | bash
```

From a local checkout (builds from source):

```sh
./scripts/install.sh
```

**Linux**

From a checkout (needs Rust and the same dev packages as CI — see [Tests & CI](#tests--ci)):

```sh
./scripts/install.sh --source
```

Or download `drift-wallpaper-linux-x86_64.tar.gz` from [GitHub Releases](https://github.com/undivisible/drift-wallpaper/releases) (published on version tags).

**Windows**

In PowerShell, from a checkout:

```powershell
.\scripts\install.ps1
```

Or download `drift-wallpaper-windows-x86_64.zip` from [GitHub Releases](https://github.com/undivisible/drift-wallpaper/releases).

---

### Development builds

For the latest unreleased changes on macOS, use `--head` or the `--version` flag:

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

**Config paths**

| OS | Location |
|----|----------|
| macOS | `~/Library/Application Support/drift-wallpaper/config.json` |
| Linux / Windows | `drift-config.json` in the process working directory (today) |

## Releases

Pushing a tag `v*` (for example `v0.1.44`) runs CI on macOS, Linux, and Windows, then attaches:

- `drift-wallpaper-macos-aarch64.tar.gz`
- `drift-wallpaper-linux-x86_64.tar.gz`
- `drift-wallpaper-windows-x86_64.zip`

Each archive has a matching `.sha256` checksum on the release page.

## Workspace

| Crate | Role |
|-------|------|
| `drift-core` | `wgpu` fluid simulation, WGSL shaders, color / presets |
| `drift-app` | `winit` wallpaper windows, GPUI settings UI, platform desktop integration |

## Tests & CI

Local (same as GitHub Actions):

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked
```

**Linux build in Docker** (no GPU/display; validates compile + tests like CI):

```sh
docker build -t drift-wallpaper-linux .
```

On macOS with Apple Silicon, Docker runs `linux/amd64` by default for this image; add `--platform linux/amd64` if your builder requires it.

## License

[MPL-2.0](LICENSE).
