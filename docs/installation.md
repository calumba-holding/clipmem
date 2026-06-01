# Installation

clipmem runs on macOS and requires Apple Silicon for the prebuilt Homebrew
package. Intel Macs and source builds are supported through Cargo.

## Requirements

- **macOS** — clipboard capture uses `NSPasteboard` via `objc2` /
  `objc2-app-kit` and only runs on macOS. The database and search layers
  compile on other platforms for development, but capture won't work.
- **Apple Silicon** — required for the prebuilt Homebrew package (target
  `aarch64-apple-darwin`). Intel Macs can use `cargo install` instead.
- **Rust 1.88+** — required only if you're building from source.

## Homebrew (CLI only)

Install the CLI binary on Apple Silicon:

```bash
brew install tristanmanchester/tap/clipmem
clipmem setup
```

Verify the installation:

```bash
clipmem --version
clipmem doctor
```

If Homebrew reports that `tristanmanchester/tap` is not trusted, trust the
formula and rerun the install command:

```bash
brew trust --formula tristanmanchester/tap/clipmem
brew install tristanmanchester/tap/clipmem
```

## Homebrew (CLI + menu bar app)

Install the CLI and the native SwiftUI menu bar app together:

```bash
brew install --cask tristanmanchester/tap/clipmem-app
open -a ClipmemMenuBar
```

The cask depends on the CLI formula, runs `clipmem setup` after
installation, and enables launch at login by default.

If Homebrew reports that `tristanmanchester/tap` is not trusted, trust the
formula and cask before installing:

```bash
brew trust --formula tristanmanchester/tap/clipmem
brew trust --cask tristanmanchester/tap/clipmem-app
brew install --cask tristanmanchester/tap/clipmem-app
```

## Cargo

Install on Apple Silicon or Intel:

```bash
cargo install clipmem
clipmem setup
```

Verify the installation:

```bash
clipmem --version
clipmem doctor
```

## Build from source

Clone the repository and build:

```bash
cargo build --release
```

Or install the current checkout into `~/.local/bin`:

```bash
cargo install --path . --root ~/.local --force --locked
```

All install methods (Homebrew CLI, Cargo, source) produce the same
`clipmem` binary.

## Upgrading

Upgrade using the same method you installed with:

```bash
# Homebrew
brew upgrade clipmem

# Cargo (overwrites the existing binary)
cargo install clipmem

# Source
git pull && cargo build --release
```

Re-running `clipmem setup` after an upgrade is safe and updates the
managed service definition as needed.

## Next steps

- [Getting started](getting-started.md) — set up background capture
  and run your first queries
- [Menu bar app](menu-bar-app.md) — install and configure the native
  macOS menu bar frontend
