# Installation

[Wiki home](README.md) · [Français](../fr/installation.md)

## What you need

| | Required for | Notes |
|---|---|---|
| **Vintage Story 1.22** | Converting to Vintage Story | OdasiO starts the game's dedicated server, in the background, to create the new world with your settings, then writes your blocks into it. |
| **Minecraft Java Edition** | Textures of the 3D preview | Launch the game at least once so a version is downloaded. Not needed to convert. |
| A graphics card with Vulkan, Metal or OpenGL | 3D preview | Any card from the last ten years. The rest of the app works without it. |

OdasiO looks for the games in their default locations:

| | Linux | Windows |
|---|---|---|
| Vintage Story | `~/.local/share/vintagestory` | `%APPDATA%\Vintagestory` |
| Vintage Story saves | `~/.config/VintagestoryData/Saves` | `%APPDATA%\VintagestoryData\Saves` |
| Minecraft | `~/.minecraft` (and the Flatpak launcher's folder for saves) | `%APPDATA%\.minecraft` |

Vintage Story's server needs the **.NET runtime** that the game itself uses. If the game runs, the server runs too. On Linux, a runtime installed in `~/.dotnet` is picked up automatically.

## Linux

### Debian, Ubuntu and derivatives

Download the `.deb` package from the [Releases](https://github.com/JibayMcs/OdasiO/releases) page, then:

```sh
sudo apt install ./odasio_*.deb
```

This installs two programs: **OdasiO** (in your applications menu, `odasio-gui` from a terminal) and the `odasio` command line.

To uninstall: `sudo apt remove odasio`.

### Other distributions

Build from source (below). The app is a single binary with no runtime dependencies apart from your graphics drivers and, for file dialogs, `xdg-desktop-portal` (installed with every desktop environment).

## Windows

Download the installer (`.exe`) from the [Releases](https://github.com/JibayMcs/OdasiO/releases) page and run it. OdasiO appears in the Start menu. The `odasio` command line is installed next to it.

## Updates

OdasiO checks for a new version at startup (at most once a day) and shows a banner when one is available: **Update** downloads it, checks its SHA-256 fingerprint against the list published with the release, and installs it the way OdasiO was installed:

- **Windows installer**: the new installer runs silently, then restarts OdasiO;
- **`.deb` package**: installed with your password (`pkexec`); without it, the package opens in your software manager;
- **portable archive**: the executable is replaced in place.

Click the version number (top right) to check right away. Set the `ODASIO_NO_UPDATE` environment variable to disable the automatic check. Versions built from source don't check in debug mode.

## Build from source

1. Install Rust 1.85 or newer with [rustup](https://rustup.rs).
2. On Linux, install a C compiler (used to build SQLite and zstd):

   ```sh
   # Debian, Ubuntu
   sudo apt install build-essential pkg-config
   # Fedora
   sudo dnf install gcc pkgconf-pkg-config
   # Arch
   sudo pacman -S base-devel
   ```

3. Build:

   ```sh
   git clone https://github.com/JibayMcs/OdasiO.git
   cd OdasiO
   cargo build --release
   ```

The programs are in `target/release/`: `odasio-gui` (desktop app) and `odasio` (command line). You can copy them anywhere, they embed everything they need (block mapping, translations, icon).

To check everything works:

```sh
cargo test --workspace
./target/release/odasio info
```
