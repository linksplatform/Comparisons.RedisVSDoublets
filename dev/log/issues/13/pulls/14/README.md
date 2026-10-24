# Issue #13, pull request #14: update all dependencies

Data collected while solving
[issue #13](https://github.com/linksplatform/Comparisons.RedisVSDoublets/issues/13)
in [pull request #14](https://github.com/linksplatform/Comparisons.RedisVSDoublets/pull/14).

| File | Content |
|---|---|
| `issue-13.json`, `issue-13-comments.json` | the issue and its comment (the detailed requirements) |
| `pr-14*.json` | the pull request, its comments and reviews (none when collected) |
| `dependabot/pr-*.json` | the Dependabot pull requests #2 and #6 to #12 |
| `ci-logs/` | the failed `auto-merge` jobs of #6, #8 and #12 |
| `versions.md` | every pin and the newest registry version, from `experiments/deps/versions_report.py` |
| `upstream/` | the bodies of the upstream reports |

## Requirements and how they are met

| # | Requirement | Solution | Verification |
|---|---|---|---|
| 1 | Fix the errors of the merged Dependabot pull requests #6 to #12 without reverting them | Their builds, tests and audits on `main` passed. Only `auto-merge` failed: `GraphQL: Auto merge is not allowed for this repository (enablePullRequestAutoMerge)` (`ci-logs/`). A workflow cannot change that repository setting, so `AutoMerge.yml` now fails with an explicit `::error::` saying which setting a maintainer must enable, or to merge by hand. | `ci-logs/automerge-*.log`; the AutoMerge step in `.github/workflows/AutoMerge.yml` |
| 2 | doublets 0.5.0 to 0.8.0 (#6, #7) | Kept; documented what 0.8.0 fixes. The random check of `rust/tests/same_behavior.rs` now also covers the split store, with references to existing links only, because of the defect reported upstream. | `cargo test`; `experiments/logs/rust-*.log` |
| 3 | Apply or explain the open #2 (tj-actions/changed-files 21 to 41) | Not applicable: `csharp.yml` no longer uses `tj-actions/changed-files` (the workflow uses `paths:` filters), so #2 conflicts with a step that no longer exists. Dependabot closes it when it finds no such dependency. | `grep -rn changed-files .github` finds nothing |
| 4 | Rust fully current, `edition` and `rust-version` revisited | Toolchain 1.99.1 in `rust-toolchain.toml`, installed by `rustup` in every workflow. Floors raised to the resolved versions. All four lockfiles regenerated. Edition 2024 is the newest. | `cargo update --dry-run` locks 0 packages; `versions.md` |
| 5 | C#/.NET fully current, TargetFramework checked | SDK 10.0.402. `Directory.Packages.props` holds one version per package for all 4 projects (Central Package Management with transitive pinning). net10.0 is the newest TargetFramework. | `dotnet build`/`dotnet test`; `versions.md` |
| 6 | Python fully current, honest constraints | Python 3.15 in every workflow; ruff 0.17.2 in requirements and pre-commit; `target-version = "py315"`. `pyproject.toml` has no dependencies, so no floors exist to raise. | ruff, mypy, unittest; `versions.md` |
| 7 | GitHub Actions current | Every `uses:` is pinned to the commit of the newest release; npx pins are current. | `versions.md` |
| 8 | pre-commit current | `ruff-pre-commit` v0.17.2; the local Rust hook checks every experiment. | `pre-commit run --all-files` |
| 9 | One version per dependency | Central NuGet versions; the same doublets 0.8.0 in every Cargo.toml; one ruff pin shared by pip and pre-commit; one Python version in all workflows. | `versions.md` lists one version per package except the transitive `syn` 2/3 |
| 10 | Use upstream features instead of hand-rolled code | Central Package Management replaces the copies of the versions in 4 csproj files. `scripts/benchmark_report.py` reads the versions from the lockfile. `rustup` reads `rust-toolchain.toml` instead of a version repeated in workflows. | diff |
| 11 | Build, test, lint, fix deprecations | Rust, C#, Python and Markdown suites run locally and in CI. | CI of pull request #14 |
| 12 | Security advisories | `cargo audit` (4 lockfiles), `pip-audit`, `dotnet list package --vulnerable`: no advisories. | `.github/workflows/dependencies.yml` |
| 13 | dependabot.yml covers every ecosystem | Adds `rust-toolchain`, `dotnet-sdk`, `pre-commit`, the root `Directory.Packages.props` and the new Rust experiments. pip stays at `/scripts`: the root `pyproject.toml` only configures ruff and mypy and has no dependencies. | `.github/dependabot.yml` |
| 14 | Report upstream defects | See below. Local workarounds stay. | links |

## Left behind (with reasons)

| Dependency | Pinned | Latest | Reason |
|---|---|---|---|
| Microsoft.ApplicationInsights (transitive, tests) | 2.23.0 | 3.2.0 | Microsoft.Testing.Extensions.Telemetry 2.5.1 (via xunit.v3 4.0.2) uses 2.x APIs that 3.x removed; [microsoft/testfx#7465](https://github.com/microsoft/testfx/issues/7465) |
| anes (transitive, Rust benchmarks) | 0.1.6 | 0.2.1 | criterion 0.8.2, the newest, requires `^0.1` |
| itertools (transitive, Rust benchmarks) | 0.13.0 | 0.15.0 | criterion 0.8.2 and criterion-plot 0.8.2 require `^0.13` |
| r-efi (transitive) | 6.0.0 | 7.1.0 | getrandom 0.4.3, the newest, requires `^6` |
| syn 2 (transitive, beside syn 3.0.9) | 2.0.119 | 3.0.9 | zerocopy-derive 0.8.63 requires `^2` |
| windows-link (transitive) | 0.2.1 | 0.100.0 | windows-sys 0.61.2 requires `^0.2` |
| `Whole` memory wrapper (`rust/src/memory.rs`) | kept | — | doublets 0.8.0 still panics at link 1,040,384 without it; [doublets-rs#68](https://github.com/linksplatform/doublets-rs/issues/68) |

## Upstream reports

- [linksplatform/doublets-rs#74](https://github.com/linksplatform/doublets-rs/issues/74):
  `split::Store` loses a link that refers to an id created later (new).
- [linksplatform/doublets-rs#68 (comment)](https://github.com/linksplatform/doublets-rs/issues/68#issuecomment-6381597376):
  the capacity panic persists in 0.8.0 and affects the split store too.
- [linksplatform/Data.Doublets#522](https://github.com/linksplatform/Data.Doublets/issues/522):
  `SplitMemoryLinks` cannot find a link with an internal source and an external target (new).
- [microsoft/testfx#7465](https://github.com/microsoft/testfx/issues/7465):
  ApplicationInsights 3 support of the telemetry extension (existing).
- [linksplatform/mem-rs#36](https://github.com/linksplatform/mem-rs/issues/36):
  the `grow` semantics of platform-mem that `Whole` works around (existing).
