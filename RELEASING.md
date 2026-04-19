# Releasing `clipmem`

This project uses a tag-driven release flow.

Pushing a semver tag like `v0.1.0` triggers the release workflow in [`.github/workflows/release.yml`](/Users/tristan/Projects/clipmem/.github/workflows/release.yml), which:

- validates that the tag matches `Cargo.toml`
- builds release artifacts with `cargo-dist`
- creates or updates the GitHub Release
- publishes to crates.io through the reusable publish workflow
- updates the Homebrew formula and cask in `tristanmanchester/homebrew-tap`

The Rust CLI and SwiftUI menu bar app ship as separate Homebrew install surfaces: the `clipmem` formula installs the CLI, and the `clipmem-app` cask installs the menu bar app plus a formula dependency.

## Files involved

- [`Cargo.toml`](/Users/tristan/Projects/clipmem/Cargo.toml) – crate version and publish metadata
- [`dist-workspace.toml`](/Users/tristan/Projects/clipmem/dist-workspace.toml) – `cargo-dist` release configuration
- [`.github/workflows/ci.yml`](/Users/tristan/Projects/clipmem/.github/workflows/ci.yml) – normal CI checks
- [`.github/workflows/release.yml`](/Users/tristan/Projects/clipmem/.github/workflows/release.yml) – tag-driven release workflow
- [`.github/workflows/publish-crate.yml`](/Users/tristan/Projects/clipmem/.github/workflows/publish-crate.yml) – crates.io publish job
- [`.github/scripts/write-menubar-cask.sh`](/Users/tristan/Projects/clipmem/.github/scripts/write-menubar-cask.sh) – generated Homebrew cask writer
- [`macos/ClipmemMenuBar/`](/Users/tristan/Projects/clipmem/macos/ClipmemMenuBar) – native menu bar app source and tests

## Normal release flow

`clipmem` now publishes to crates.io through Trusted Publishing using GitHub OIDC. The reusable publish workflow authenticates immediately before `cargo publish`, so no long-lived crates.io token is exposed to earlier build or test steps.

1. Make the changes you want to ship.
2. Bump `version` in [`Cargo.toml`](/Users/tristan/Projects/clipmem/Cargo.toml).
3. Run local checks:

   ```bash
   cargo test
   cargo publish --dry-run --locked
   cargo package --list
   dist plan --allow-dirty
   xcodebuild -project macos/ClipmemMenuBar/ClipmemMenuBar.xcodeproj -scheme ClipmemMenuBar -configuration Debug -destination 'platform=macOS' -derivedDataPath /tmp/clipmem-menubar-derived test
   ```

   `--allow-dirty` is required because [`.github/workflows/release.yml`](/Users/tristan/Projects/clipmem/.github/workflows/release.yml) is intentionally hardened by hand and no longer exactly matches cargo-dist's generated template. Do not replace the workflow with generated remote installer commands just to make `dist plan` run without this flag.

4. Merge the release commit to `main`.
5. Push the release tag:

   ```bash
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```

6. Wait for the GitHub Actions release workflow to finish.
7. Verify:
   - the GitHub Release exists and has the expected assets
   - crates.io shows the new version
   - `brew install tristanmanchester/tap/clipmem` works on Apple Silicon
   - `brew install --cask tristanmanchester/tap/clipmem-app` works on Apple Silicon
   - `clipmem setup`, `clipmem service status`, and `clipmem doctor` work from the Homebrew install
   - the cask postflight starts capture without deleting or replacing the user's database
   - the menu bar app opens from `/Applications` and resolves `/opt/homebrew/bin/clipmem`
   - launch at login is enabled by default and can be disabled in Settings

## Current release artifacts

For `0.2.3`, `dist plan --allow-dirty` reports these CLI artifacts:

- `source.tar.gz`
- `source.tar.gz.sha256`
- `clipmem.rb`
- `sha256.sum`
- `clipmem-aarch64-apple-darwin.tar.xz`
- `clipmem-aarch64-apple-darwin.tar.xz.sha256`

The menu bar app release job adds these app artifacts to the same GitHub Release:

- `clipmem-app-aarch64-apple-darwin.zip`
- `clipmem-app-aarch64-apple-darwin.zip.sha256`

The generated Homebrew formula installs the `clipmem` binary for Apple Silicon. It does not include a `service do` stanza, so users should start background capture with `clipmem setup` when installing the CLI alone. The `clipmem-app` cask depends on that formula and runs `clipmem setup` in its postflight step.

## Menu bar app release

The release workflow builds `ClipmemMenuBar.app` with `Release` configuration on `macos-14`, signs it with Developer ID Application, notarizes it with `notarytool`, staples the notarization ticket, uploads the final zip to the GitHub Release, and publishes `Casks/clipmem-app.rb` to the tap.

Required GitHub secrets:

- `APPLE_DEVELOPER_ID_CERTIFICATE_BASE64` – base64-encoded `.p12` Developer ID Application certificate.
- `APPLE_DEVELOPER_ID_CERTIFICATE_PASSWORD` – password for that `.p12`.
- `APPLE_TEAM_ID` – Apple Developer team id.
- Preferred notarization path: `APP_STORE_CONNECT_API_KEY_BASE64`, `APP_STORE_CONNECT_KEY_ID`, and `APP_STORE_CONNECT_ISSUER_ID`.
- Fallback notarization path: `APPLE_ID` and `APPLE_APP_SPECIFIC_PASSWORD`.
- `HOMEBREW_TAP_TOKEN` – token that can push to `tristanmanchester/homebrew-tap`.

Clean-machine cask verification:

```bash
brew install tristanmanchester/tap/clipmem
clipmem --version
brew install --cask tristanmanchester/tap/clipmem-app
clipmem service status
open -a ClipmemMenuBar
```

The cask intentionally removes only app preferences on zap. Do not delete `~/Library/Application Support/clipmem` from the cask because that directory contains the user's clipboard archive and logs.

## Trusted Publishing configuration

The crates.io Trusted Publisher for `clipmem` should be configured as:

1. In the crate settings on crates.io:
   - `clipmem` -> Settings -> Trusted Publishing
2. Add a GitHub trusted publisher with:
   - repository owner: `tristanmanchester`
   - repository name: `clipmem`
   - workflow filename: `release.yml`
   - environment: `release`
3. Confirm the GitHub repo `release` environment exists, since the workflow runs in that environment and crates.io matches against it.
4. After the first successful OIDC-backed publish, remove the old `CARGO_REGISTRY_TOKEN` GitHub Actions secret if it still exists.

## Current CI checks

Normal CI in [`.github/workflows/ci.yml`](/Users/tristan/Projects/clipmem/.github/workflows/ci.yml) runs:

- `cargo test`
- `cargo publish --dry-run --locked`
- `cargo package --list`

The release publish workflow reruns the same validation before publishing.

## Notes

- Release tags should be `vX.Y.Z`.
- The packaged binary target is currently `aarch64-apple-darwin` only.
- The Homebrew formula and cask are published to the dedicated tap, not `homebrew/core`.
- The Homebrew formula installs the CLI only; the SwiftUI app is installed by the `clipmem-app` cask.
- GitHub Release is an output of the tag push, not the thing that triggers publishing.
- The reusable publish job is invoked from `release.yml`, and crates.io currently validates the caller workflow filename from GitHub's `workflow_ref` claim. Keep the trusted publisher registered to `release.yml`.
- [`.github/workflows/release.yml`](/Users/tristan/Projects/clipmem/.github/workflows/release.yml) is now a security-owned workflow. If you regenerate cargo-dist files with `dist init --yes`, review the diff manually and keep the pinned installer hardening in place instead of restoring generated bootstrap commands.
