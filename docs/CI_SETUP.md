# GitHub Actions build verification for lg-wled-sync

The fork's Rust and ARMv7 checks are defined in `.github/workflows/ci.yml`
on this development branch. It triggers on pushes and pull requests targeting
`feature/vendor-neutral-wled-drm-first` or `main`, and on manual dispatch.

## Required one-time GitHub fork step

GitHub disables workflows on newly created forks by default. Go to:

https://github.com/angelcode1/lg-wled-sync/actions

and click **I understand my workflows, go ahead and enable them** if shown.
No credentials or TV access are required for CI.

## Normalize sources, then validate

1. In Actions, select **Format Rust (manual)** (defined on main).
2. Choose **Run workflow**. This uses stable Rustfmt to format and commit
   changed Rust source on the development branch only.
3. That commit triggers the **CI** workflow on the development branch.
4. Open CI and require both jobs to finish with green checks:
   - Rust fmt / tests / Clippy / dashboard
   - ARMv7 / webOS glibc 2.28 ABI

Direct CI dispatch is available because ci.yml also exists on main.
If no jobs appear, verify the fork's Actions enablement in Settings ->
Actions -> General and the Actions tab. GitHub API status may be empty until
the first job actually runs.

## Required tools and gates

- Ubuntu 24.04, native C toolchain, perl, libssl-dev, pkg-config, CMake,
  Python 3 and Node.
- Stable Rust with rustfmt and Clippy.
- cargo-nextest 0.9.146, installed from its official prebuilt package.
- scripts/check_release.py, cargo fmt --check, locked nextest, locked
  Cargo doc tests, Clippy with warnings denied, dashboard JS syntax.
- Docker-based Debian Buster cross compiler with ARMv7 gnueabi Rust target,
  GNU cross linker and glibc <=2.28 ELF ABI validation.

## Important

Changing workflow files or adding these tools does not by itself constitute
a passing build. Inspect individual job logs. Do not install the binary on
the TV until tests, ARMv7 build, symbol/version checks and model-specific
native histogram validation are complete.

The CI host job also checks C5 collector bash syntax and runs the offline profile importer unittest suite.
