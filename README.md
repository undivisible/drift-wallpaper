# the drift macos screensaver as a wallpaper

from [sandydoo/flux](https://github.com/sandydoo/flux).

![CI](https://github.com/undivisible/drift-wallpaper-macos/actions/workflows/ci.yml/badge.svg)

---

## Requirements

| Dependency | Minimum version |
|---|---|
| macOS | 14.0 (Sonoma) |
| Xcode Command Line Tools | 15.0 |
| Rust toolchain | 1.80 (stable) |

Install the Rust toolchain with [rustup](https://rustup.rs):

```sh
curl --proto '=https' --tlsv1.2 -sSf https://rustup.rs | sh
```

---

## Building

```sh
# Clone the repository
git clone https://github.com/undivisible/drift-wallpaper-macos
cd drift-wallpaper-macos

# Debug build (faster compilation, slower rendering)
cargo build -p drift-app

# Optimised release build (recommended for daily use)
cargo build --release -p drift-app

# The binary is at:
./target/release/drift-wallpaper
```

---

## Running

```sh
# Run directly from the project root
cargo run --release -p drift-app

# Or run the compiled binary
./target/release/drift-wallpaper
```

When the app starts:

1. The Dock icon is hidden (accessory mode).  
2. A 🌊 icon appears in the menu bar.  
3. The live wallpaper covers all connected displays.  

Use the menu bar icon to configure the app.

---

## Menu bar options

| Option | Description |
|---|---|
| **Enable / Disable Wallpaper** | Toggle the live wallpaper on/off without quitting |
| **Colour Preset ▶** | Switch between six built-in colour palettes |
| **Extract Colors from Image…** | Open a file picker; the app derives a 3-stop gradient from the image |
| **Start at Login** | Install/remove the `~/Library/LaunchAgents/com.drift-wallpaper.macos.plist` agent |
| **Quit Drift Wallpaper** | Terminate the app; the original macOS desktop wallpaper reappears |

---

## Reverting to the default wallpaper

Quit the app via the menu bar (🌊 → Quit) or `pkill drift-wallpaper`.  
The Drift windows close and the standard macOS desktop image reappears immediately.

---

## Configuration

The app stores its config at:

```
~/Library/Application Support/drift-wallpaper/config.json
```

Example:

```json
{
  "enabled": true,
  "launch_at_login": false,
  "params": {
    "speed": 1.0,
    "scale": 1.0,
    "color_a": [0.01, 0.01, 0.08],
    "color_b": [0.05, 0.05, 0.30],
    "color_c": [0.35, 0.35, 0.90],
    "target_fps": 60
  }
}
```

---

## Architecture

```
drift-wallpaper-macos/
├── crates/
│   ├── drift-core/          # Platform-independent GPU simulation (wgpu + WGSL)
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── simulation.rs   # DriftParams – tunable simulation config
│   │   │   ├── renderer.rs     # DriftRenderer – wgpu render pipeline
│   │   │   ├── color.rs        # ColorPalette – presets + image extraction
│   │   │   └── shaders/
│   │   │       └── drift.wgsl  # Domain-warped fbm fragment shader
│   │   └── Cargo.toml
│   └── drift-app/           # macOS application binary
│       ├── src/
│       │   ├── main.rs         # winit event loop + wgpu surface per display
│       │   ├── config.rs       # AppConfig – persist to ~/Library/…
│       │   ├── wallpaper.rs    # WallpaperWindow via objc2 (macOS only)
│       │   ├── menubar.rs      # NSStatusItem menu via objc2 (macOS only)
│       │   └── launch_agent.rs # launchd agent install/uninstall
│       └── Cargo.toml
├── launch-agents/
│   └── com.drift-wallpaper.macos.plist  # launchd template
├── .github/workflows/ci.yml
└── README.md
```

### macOS wallpaper integration

Each display gets a borderless `NSWindow` set to level `kCGDesktopWindowLevel`
(`-2147483630`).  The window:

- ignores mouse events (`setIgnoresMouseEvents(true)`)  
- joins all Spaces (`CanJoinAllSpaces | Stationary | IgnoresCycle`)  
- is created by `objc2` + `objc2-app-kit` Rust bindings  

wgpu renders to a Metal-backed surface inside this window at the display's
native refresh rate via **Fifo** present mode (VSync).

---

## Tests

```sh
# Run platform-independent unit tests (works on Linux/macOS)
cargo test -p drift-core

# Run all tests (macOS only for full coverage)
cargo test --workspace
```

---

## Performance

- CPU usage: ~0 % during steady-state rendering (all work on GPU).  
- GPU usage: < 5 % on Apple M-series (fragment shader is ALU-bound, not memory-bound).  
- Memory: ~30 MB RSS (wgpu + Metal driver overhead).  

---

## Troubleshooting

| Symptom | Fix |
|---|---|
| Black screen instead of wallpaper | Ensure the binary has Screen Recording permission in *System Settings → Privacy* |
| App does not start at login | Toggle "Start at Login" off then on again to reinstall the Launch Agent |
| High GPU usage | Reduce `target_fps` in `config.json` (e.g. `30`) |
| Wallpaper only on one screen | All screens are detected at launch; plug in additional displays before starting |

---

## License

[MPL-2.0](LICENSE).
