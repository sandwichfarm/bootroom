---
phase: 06-distribution
type: overview
mode: mvp
plan_count: 8
waves: 4
---

# Phase 6: Distribution — Plan Set Overview

## Phase Goal

**As a** kernel project on any supported platform (Linux x86_64/aarch64-musl, macOS x86_64/aarch64), **I want to** install `bootroom` in one step (`cargo install --locked bootroom`, `cargo binstall bootroom`, or `curl ... bootroom-installer.sh | sh`), **so that** I can run the binary from any working directory with no in-repo assumptions and gate my CI on a published, dual-licensed release.

## Requirements Coverage

- **DIST-02** — Single command installs the binary locally (`make install` / `cargo install --path .`). Covered by **06-04**.
- **DIST-03** — Published to crates.io; `cargo install bootroom` works. Covered by **06-03** (cargo-dist release.yml drives the publish) + **06-06** (release-smoke gates the publish).
- **DIST-04** — Prebuilt release binaries for four targets via cargo-dist + GitHub Releases. Covered by **06-03**.
- **DIST-05** — Binary runs from any working directory; assets embedded via `include_dir!`. Covered by **06-02** (`[package].include` allow-list) + **06-08** (path-independence smoke).
- **DIST-06** — `cargo binstall bootroom` works automatically from release artifacts. Covered by **06-03** (drops out of cargo-dist + `[package.repository]`).
- **DIST-07** — License is MIT OR Apache-2.0 (dual SPDX). Covered by **06-01** (SPDX + license files + manifest fields) + **06-05** (`cargo deny` enforcement in CI).

## Multi-Source Coverage Audit

| Source | Item | Covered by |
|---|---|---|
| GOAL | One-step install on any of the four supported platforms | 06-03, 06-04 |
| GOAL | Binary runs from any working directory (no in-repo assumptions) | 06-02, 06-08 |
| GOAL | Three install paths (cargo / binstall / curl-installer) all discoverable | 06-03, 06-07 |
| REQ | DIST-02 (`make install` / `cargo install --path .`) | 06-04 |
| REQ | DIST-03 (crates.io publish + smoke) | 06-03, 06-06 |
| REQ | DIST-04 (prebuilt 4-target binaries) | 06-03 |
| REQ | DIST-05 (path-independent embedded assets) | 06-02, 06-08 |
| REQ | DIST-06 (`cargo binstall` discovery) | 06-03 |
| REQ | DIST-07 (MIT OR Apache-2.0 dual license) | 06-01, 06-05 |
| RESEARCH | `cargo-dist` v0.31+ as the release pipeline tool | 06-03 |
| RESEARCH | Four targets: x86_64/aarch64-unknown-linux-musl + x86_64/aarch64-apple-darwin | 06-03 |
| RESEARCH | `cargo-zigbuild` cross-compile path (cargo-dist default for musl) | 06-03 |
| RESEARCH | `cargo-binstall` auto-discovery via `[package.repository]` | 06-03 |
| RESEARCH | Explicit `[package].include` allow-list (avoids silent `.gitignore` drops) | 06-02 |
| RESEARCH | `cargo-deny` enforces `MIT OR Apache-2.0` in CI | 06-05 |
| RESEARCH | Docker-isolated `cargo install --locked` smoke gates publish | 06-06 |
| RESEARCH | `bootroom doctor --format json` is the install-validation surface | 06-06 |
| CONTEXT | D-01 (cargo-dist as the pipeline tool; `cargo dist init --yes`) | 06-03 |
| CONTEXT | D-02 (four targets; no Windows, no Linux GNU) | 06-03 |
| CONTEXT | D-03 (dual MIT OR Apache-2.0; LICENSE files at repo root + SPDX + badge) | 06-01 |
| CONTEXT | D-04 (explicit `[package].include` allow-list) | 06-02 |
| CONTEXT | D-05 (release-smoke = Docker `cargo install --locked` + `bootroom doctor`; gates publish) | 06-06 |
| CONTEXT | D-06 (path-independence verification from `/tmp`) | 06-08 |
| CONTEXT | D-07 (primary/secondary/tertiary install matrix in README) | 06-07 |
| CONTEXT | D-08 (`make install` + `make release` targets) | 06-04 |
| CONTEXT | D-09 (bootroom-core publishes first; registry version pinning) | 06-02 (manifest metadata), 06-03 (cargo-dist publish ordering) |
| CONTEXT | D-10 (`deny.toml` enforces MIT/Apache + equivalents) | 06-05 |

**No unplanned source items.** All locked decisions land in at least one plan. No Deferred Ideas (Windows targets, Linux GNU, Docker image, external Homebrew tap maintenance, auto-update, non-CI publish, rate-limit handling) appear in any plan.

## Repo-Rename Caveat

Per PROJECT.md Key Decisions, the workspace currently lives at `~/Develop/norn-web` and the GitHub repo at `https://github.com/sandwich-farm/bootroom` (already named `bootroom` in `Cargo.toml` `[workspace.package].repository`). Plan **06-07** README install matrix and **06-03** cargo-dist `[workspace.metadata.dist]` use the **future canonical `bootroom` repo name** (already the value in `Cargo.toml`). If the GitHub remote is still under the old org/name at release time, that is an operational rename, **not a planning change** — no plan body assumes a particular GitHub URL beyond what `[workspace.package].repository` already pins.

## Dependency Graph & Waves

Four waves total. Same-wave plans have zero `files_modified` overlap and can run in parallel.

```
Wave 1 (foundational — license + license enforcement; needed before publish-shaped plans):
  06-01  License files + SPDX manifests
         files_modified: LICENSE-MIT, LICENSE-APACHE, crates/bootroom/Cargo.toml,
                         crates/bootroom-core/Cargo.toml, README.md
         depends_on: []

  06-05  cargo-deny configuration
         files_modified: deny.toml, .github/workflows/ci-deny.yml
         depends_on: []
         (Independent of 06-01: deny.toml's license allow-list is the
          authoritative SPDX list — it does NOT read the crate manifests.
          Same-wave with 06-01 only if files_modified do not overlap. They
          don't. Both can run in Wave 1.)

Wave 2 (packaging metadata — needs license in place for SPDX field):
  06-02  Cargo.toml [package].include allow-lists + publish metadata
         files_modified: crates/bootroom/Cargo.toml, crates/bootroom-core/Cargo.toml
         depends_on: [06-01]
         (Hard dep: 06-01 writes the `license` workspace key + SPDX values
          that 06-02's `cargo package --list` smoke checks assume. Also,
          06-02 touches the SAME manifests as 06-01 → forced to a later
          wave by the files_modified overlap rule even without the
          semantic dep.)

Wave 3 (release pipeline; needs licensed + properly-packaged crates):
  06-03  cargo-dist init + release.yml
         files_modified: Cargo.toml (workspace metadata.dist),
                         dist-workspace.toml (cargo-dist generated, optional),
                         .github/workflows/release.yml
         depends_on: [06-01, 06-02]
         (Hard dep: cargo-dist refuses to init if license/manifest metadata
          is incomplete or missing. Also reads `[package].include` to know
          what to ship.)

Wave 4 (consumers of the release pipeline — parallel after 06-03):
  06-04  make install / make release targets
         files_modified: Makefile, README.md
         depends_on: [06-03]
         (Hard dep on 06-03: `make release` calls `cargo dist build
          --artifacts=all`, which requires the dist config 06-03 lands.
          README dep is soft — quickstart pointer only, distinct from
          06-07's install matrix.)

  06-06  Release-smoke workflow (Docker cargo install + bootroom doctor)
         files_modified: .github/workflows/release-smoke.yml,
                         crates/bootroom/tests/install_smoke.rs
         depends_on: [06-03]
         (Hard dep on 06-03: smoke runs BEFORE release.yml publishes, but
          must coordinate with the cargo-dist-generated workflow's trigger
          and gating mechanism. release.yml needs to know how to wait on
          release-smoke.)

  06-07  README install matrix + quickstart
         files_modified: README.md
         depends_on: [06-04, 06-06]
         (Soft dep on 06-04/06: README documents `make install`, `make
          release`, and the three install paths cargo-dist enables. Must
          land after both so the documented commands actually work.)

  06-08  Path-independence verification test
         files_modified: crates/bootroom/tests/path_independence.rs,
                         .github/workflows/release-smoke.yml
         depends_on: [06-06]
         (Hard dep on 06-06: 06-08 extends release-smoke.yml with the
          /tmp-CWD invocation step. Re-touches the same workflow file →
          forced sequential by the files_modified overlap rule.)
```

### Wave Summary

| Wave | Plans | Rationale |
|---|---|---|
| 1 | 06-01, 06-05 | No file overlap; independent SPDX-related groundwork |
| 2 | 06-02 | Mutates same manifests as 06-01 (forced sequential) |
| 3 | 06-03 | cargo-dist init needs license + include metadata in place |
| 4 | 06-04, 06-06 (parallel), then 06-07, 06-08 (sequential after 06-06) | Consumers of the release pipeline |

(06-07 and 06-08 are notionally Wave 4 but sequence after 06-06 via depends_on; an executor can run them sequentially or fan-out one-at-a-time after 06-04 + 06-06 land.)

## Source Audit Conclusion

All 6 DIST requirements, all 10 D-XX context decisions, all 8 RESEARCH-derived patterns, and the phase GOAL are covered. No item is MISSING; no Deferred Idea has leaked into any plan. The plan set is publishable as-is.

## Next Steps

Execute: `/gsd-execute-phase 06`

<sub>`/clear` first — fresh context window</sub>
