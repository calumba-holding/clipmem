# Releasing `clipmem`

This project uses a tag-driven release flow.

Pushing a semver tag like `v0.1.0` triggers the release workflow in [`.github/workflows/release.yml`](/Users/tristan/Projects/clipmem/.github/workflows/release.yml), which:

- validates that the tag matches `Cargo.toml`
- builds release artifacts with `cargo-dist`
- creates or updates the GitHub Release
- publishes to crates.io through the reusable publish workflow
- updates the Homebrew tap at `tristanmanchester/homebrew-tap`

## Files involved

- [`Cargo.toml`](/Users/tristan/Projects/clipmem/Cargo.toml) – crate version and publish metadata
- [`dist-workspace.toml`](/Users/tristan/Projects/clipmem/dist-workspace.toml) – `cargo-dist` release configuration
- [`.github/workflows/ci.yml`](/Users/tristan/Projects/clipmem/.github/workflows/ci.yml) – normal CI checks
- [`.github/workflows/release.yml`](/Users/tristan/Projects/clipmem/.github/workflows/release.yml) – tag-driven release workflow
- [`.github/workflows/publish-crate.yml`](/Users/tristan/Projects/clipmem/.github/workflows/publish-crate.yml) – crates.io publish job

## Normal release flow

`clipmem` now publishes to crates.io through Trusted Publishing using GitHub OIDC. The reusable publish workflow authenticates immediately before `cargo publish`, so no long-lived crates.io token is exposed to earlier build or test steps.

1. Make the changes you want to ship.
2. Bump `version` in [`Cargo.toml`](/Users/tristan/Projects/clipmem/Cargo.toml).
3. Run local checks:

   ```bash
   cargo test
   cargo publish --dry-run --locked
   cargo package --list
   dist plan
   ```

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
- The Homebrew formula is published to the dedicated tap, not `homebrew/core`.
- GitHub Release is an output of the tag push, not the thing that triggers publishing.
- The reusable publish job is invoked from `release.yml`, and crates.io currently validates the caller workflow filename from GitHub's `workflow_ref` claim. Keep the trusted publisher registered to `release.yml`.
- [`.github/workflows/release.yml`](/Users/tristan/Projects/clipmem/.github/workflows/release.yml) is now a security-owned workflow. If you regenerate cargo-dist files with `dist init --yes`, review the diff manually and keep the pinned installer hardening in place instead of restoring generated bootstrap commands.
