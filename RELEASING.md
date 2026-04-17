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

Use this once `clipmem` already exists on crates.io and the repo has been migrated to Trusted Publishing.

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

## Bootstrap release for `0.1.0`

The first crates.io publish cannot use Trusted Publishing because crates.io requires the crate to already exist before Trusted Publishing can be configured.

For the first release only:

1. Ensure the Homebrew tap repo exists:
   - [tristanmanchester/homebrew-tap](https://github.com/tristanmanchester/homebrew-tap)
2. In `tristanmanchester/clipmem`, add GitHub Actions secrets:
   - `HOMEBREW_TAP_TOKEN`
     - must have write access to `tristanmanchester/homebrew-tap`
   - `CARGO_REGISTRY_TOKEN`
     - temporary bootstrap token for crates.io
3. In crates.io account settings, ensure the account behind `CARGO_REGISTRY_TOKEN` has a verified email address.
4. In GitHub repo settings, confirm the `release` environment exists.
5. Run local checks:

   ```bash
   cargo test
   cargo publish --dry-run --locked
   cargo package --list
   dist plan
   ```

6. Push the bootstrap tag:

   ```bash
   git tag v0.1.0
   git push origin v0.1.0
   ```

7. Verify:
   - GitHub Release contains:
     - `clipmem-aarch64-apple-darwin.tar.xz`
     - checksums
     - `clipmem.rb`
   - crates.io contains `clipmem 0.1.0`
   - `brew install tristanmanchester/tap/clipmem` succeeds

## Migrate to crates.io Trusted Publishing

After `0.1.0` is published on crates.io:

1. Go to the crate settings on crates.io:
   - `clipmem` -> Settings -> Trusted Publishing
2. Add a GitHub trusted publisher with:
   - repository owner: `tristanmanchester`
   - repository name: `clipmem`
   - workflow filename: `release.yml`
   - environment: `release`
3. Update [`.github/workflows/publish-crate.yml`](/Users/tristan/Projects/clipmem/.github/workflows/publish-crate.yml):
   - replace the bootstrap `CARGO_REGISTRY_TOKEN` publish step with `rust-lang/crates-io-auth-action@v1`
   - pass the action output token to `cargo publish`
4. Test on the next non-production release tag if you want a dry operational check.
5. Remove the `CARGO_REGISTRY_TOKEN` secret from GitHub after Trusted Publishing is confirmed working.

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
- [`.github/workflows/release.yml`](/Users/tristan/Projects/clipmem/.github/workflows/release.yml) is now a security-owned workflow. If you regenerate cargo-dist files with `dist init --yes`, review the diff manually and keep the pinned installer hardening in place instead of restoring generated bootstrap commands.
