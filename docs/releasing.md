# Releasing (maintainers)

Releases are built by [cargo-dist](https://opensource.axo.dev/cargo-dist/) from `dist-workspace.toml`
(plan D42). The workflow is `.github/workflows/release.yml`, which dist generates; don't edit it by hand.

## One-time setup

1. Create the public repo `kondfox/homebrew-tap` (empty, default branch `main`).
2. Create a fine-grained token with **Contents: read and write** on `kondfox/homebrew-tap` only, and store
   it as the Actions secret `HOMEBREW_TAP_TOKEN` in this repo.

## Cutting a release

1. Bump `version` in the root `Cargo.toml` (`[workspace.package]`) and commit.
2. Tag and push: `git tag v0.2.0 && git push origin v0.2.0`.
3. The workflow builds macOS (arm64, x64), Linux (x64, arm64) and Windows (x64) binaries. Each build
   compiles the dashboard and the WASM engine and fails instead of shipping the placeholder page
   (`.github/build-setup.yml`). It then creates the GitHub Release with the shell and PowerShell
   installers and pushes `Formula/pmx.rb` to the tap.
4. Smoke-test one artifact: `pmx --version`, then `pmx demo /tmp/x && pmx -C /tmp/x serve --open`.

## Changing the release setup

Edit `dist-workspace.toml` (or `.github/build-setup.yml`), then run `dist generate` and commit the
regenerated workflow. `dist plan` lists what a release would build. `dist build --artifacts=local`
builds this machine's archive into `target/distrib/`.
