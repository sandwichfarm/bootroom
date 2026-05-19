---
phase: 6
name: Distribution
gathered: 2026-05-19
status: Ready for planning
mode: smart-discuss
---

# Phase 6: Distribution — Context

<domain>
## Phase Boundary

**Goal:** A kernel project on any supported platform installs `bootroom` in one step — `cargo install --locked bootroom`, `cargo binstall bootroom`, or a `curl | sh` from a GitHub Release — and runs it from any working directory with no in-repo assumptions.

**In scope (Phase 6):**
- **License files:** add `LICENSE-MIT` and `LICENSE-APACHE` at the repo root; add SPDX `MIT OR Apache-2.0` to both `crates/bootroom/Cargo.toml` and `crates/bootroom-core/Cargo.toml` `[package].license`; add badges/links to README.
- **Crates.io packaging:** add explicit `[package].include` allow-lists to both crate manifests covering `src/`, `build.rs`, `Makefile` (bootroom only), `web/`, `assets/qemu/*.wasm`, `assets/qemu/*.data`, `assets/qemu/*.worker.js`, `assets/qemu/qemu-wasm-rev.txt`, `LICENSE-MIT`, `LICENSE-APACHE`, `README.md`. Explicit allow-list prevents `.gitignore`-shadowed assets from silently dropping (Pitfall: `cargo publish` ignores files that aren't tracked OR not in `include`).
- **`make install` + `make release` targets** in the existing Makefile:
  - `make install` → `cargo install --path crates/bootroom` (DIST-02)
  - `make release` → `cargo dist build --artifacts=all` (local cross-platform smoke)
- **cargo-dist setup** — initialize `cargo-dist` config (v0.31+) targeting `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`, `x86_64-apple-darwin`, `aarch64-apple-darwin`. Uses `cargo-zigbuild` under the hood for Linux musl cross-compile. Generates `.github/workflows/release.yml`. Auto-emits shell installer (`bootroom-installer.sh`) and Homebrew tap formula.
- **`cargo install bootroom` smoke test** — CI step that, inside a clean Docker container, runs `cargo install --locked bootroom` from the just-built crate (path-based or local registry) then `bootroom doctor` to verify the installed binary works (DIST-03 + DIST-05).
- **`cargo binstall` discovery** — drops out of `cargo-dist` + `Cargo.toml` `[package.repository]` automatically (DIST-06). No extra work — verify with a smoke test.
- **External-callable verification (DIST-05)** — release-smoke runs the installed binary from `/tmp` (not the source dir) to confirm `include_dir!` embedding works with no path assumptions.
- **README updates** — top-of-file install matrix:
  ```
  cargo install --locked bootroom            # primary, Rust-equipped devs
  cargo binstall bootroom                    # secondary, prebuilt binaries
  curl ...bootroom-installer.sh | sh         # third, no Rust toolchain needed
  ```
  Plus a quickstart section: `bootroom doctor` then `bootroom serve --kernel <path>`.
- **bootroom-core publish ordering** — publish `bootroom-core` to crates.io first so `bootroom`'s manifest can pin a registry version (not a path dep). Keep workspace path dep for local dev; cargo-dist + the release-CI handle the publish swap.
- **`cargo deny` configuration** — `deny.toml` enforcing `MIT OR Apache-2.0` and equivalents across the dep tree (RESEARCH note: the project constraint says cargo-deny gates CI).

**Out of scope:**
- Windows / `*-pc-windows-msvc` targets — defer to v2 (project constraint = Linux + macOS only).
- Linux GNU (non-musl) targets — defer; musl covers static linking + portability.
- Docker image / containerized distribution — defer.
- `homebrew-bootroom` tap maintenance beyond cargo-dist's auto-emitted formula — defer.
- Auto-update mechanism — out of scope.
- crates.io publish from a non-release-CI environment — explicitly NOT done; only the release workflow publishes.

**Phase 6 requirements:** DIST-02, DIST-03, DIST-04, DIST-05, DIST-06, DIST-07 (6 items).

</domain>

<decisions>
## Implementation Decisions

### Release Tooling

- **cargo-dist** is the release pipeline tool. Init via `cargo dist init`, target the 4 platforms above, accept the generated `release.yml`.
- **Targets:** `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`, `x86_64-apple-darwin`, `aarch64-apple-darwin`. No Windows, no Linux GNU.
- **Cross-compile mechanism:** cargo-zigbuild (cargo-dist's default for Linux musl on the macOS/Linux GH Actions runner pool).
- **License:** Dual MIT OR Apache-2.0. `LICENSE-MIT` + `LICENSE-APACHE` at repo root; SPDX in both crate manifests; README badge.

### Asset Embedding & Publishing

- **Explicit `[package].include` allow-list** on each crate manifest (see Phase Boundary list).
- **Both crates published:** `bootroom-core` first (no path-dep pin in registry version), then `bootroom`. Release workflow handles the ordering.
- **Release-smoke gate:** Docker-isolated `cargo install --locked bootroom` + `bootroom doctor` BEFORE the release tag triggers crates.io publish. CI gates the publish on green smoke.
- **Path-independence verification:** smoke test invokes the installed binary from `/tmp` (or any non-source dir) to confirm `include_dir!` embedding works without source-tree assumptions.

### Install Surface & Documentation

- **Primary install:** `cargo install --locked bootroom`.
- **Secondary:** `cargo binstall bootroom` (auto-discovered via cargo-dist artifacts + `[package.repository]`).
- **Tertiary:** one-line `curl https://github.com/<org>/bootroom/releases/latest/download/bootroom-installer.sh | sh` (cargo-dist auto-emits this).
- **Make targets:** `make install` for the Rust path, `make release` for local cross-platform smoke (calls `cargo dist build --artifacts=all`).
- **README:** top-of-file install matrix + quickstart (`bootroom doctor` then `bootroom serve --kernel <path>`). Keep terse — point to repo docs for deeper reads.

### Claude's Discretion

- Exact CI yaml structure beyond what `cargo dist init` generates is at Claude's discretion. Don't hand-edit `release.yml` if cargo-dist's defaults work.
- `deny.toml` rule set beyond the MIT/Apache enforcement is at Claude's discretion.
- README section layout below the install matrix is at Claude's discretion.
- bootroom-core dep version pinning strategy (caret vs exact) within the manifest swap step is at Claude's discretion.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets

- `crates/bootroom/build.rs` — already validates required qemu-wasm assets at compile time; with the `include` allow-list, `cargo publish` will respect the same surface.
- `crates/bootroom/Makefile` — Phase 1/5 targets (`qemu-assets`, etc.). Append `install` and `release` targets.
- `crates/bootroom/assets/qemu/qemu-wasm-rev.txt` — exists (Phase 5); `include` allow-list must include it.
- `crates/bootroom/web/` — vanilla JS UI; `include` allow-list covers it.
- `bootroom doctor` (Phase 5) — the release-smoke gate uses `bootroom doctor --format json` to verify the installed binary's surface.
- The workspace already pins all transitive deps. `cargo install --locked` is the recommended invocation everywhere.

### Established Patterns

- **Workspace publish:** Cargo-published projects with workspace deps either inline `version = "x.y.z"` alongside `path = "../foo"` (both keys) or use `cargo workspaces`. cargo-dist handles this with the right flags.
- **License files at repo root:** both at the workspace root so SPDX resolvers pick them up; per-crate symlinks are NOT needed if Cargo.toml lists `license-file` or `license` and `include` lists them.
- **Release branching:** none beyond `master`; cargo-dist works off git tags.
- **Smoke testing:** the project's existing test patterns (CARGO_BIN_EXE + ChildGuard subprocess shape) extend naturally to a Docker-isolated `cargo install` test.

### Integration Points

- New files: `LICENSE-MIT`, `LICENSE-APACHE` at repo root; `deny.toml` at repo root; `dist-workspace.toml` (or `[workspace.metadata.dist]` in root Cargo.toml — cargo-dist's preference).
- Modified: root `Cargo.toml` (workspace metadata for cargo-dist), `crates/bootroom/Cargo.toml` (`include`, `license`, `repository`, `homepage`, `description`, `keywords`, `categories`), `crates/bootroom-core/Cargo.toml` (same), `Makefile` (install / release targets), `README.md` (install matrix + quickstart), `.github/workflows/release.yml` (cargo-dist generated).
- New CI workflow: `.github/workflows/release.yml` (cargo-dist generated, gated on `git tag v*`).
- New smoke workflow: `.github/workflows/release-smoke.yml` (Docker `cargo install --locked` + `bootroom doctor`); runs on every release tag BEFORE publish.

</code_context>

<specifics>
## Specific Ideas

- The release-smoke uses the freshly-built local artifacts (a `--registry local-test` path or a `cargo publish --dry-run` + verification) — not a hot crates.io round trip.
- `cargo dist init` is interactive. Plan a non-interactive scripted invocation with `--yes --installers shell,homebrew --targets x86_64-unknown-linux-musl,aarch64-unknown-linux-musl,x86_64-apple-darwin,aarch64-apple-darwin` (and pin the cargo-dist version in `[workspace.metadata.dist]`).
- Ensure `bootroom-core`'s `Cargo.toml` `version` matches `bootroom`'s; bump the workspace-level pin once.

</specifics>

<deferred>
## Deferred Ideas

- Windows targets (`*-pc-windows-msvc`) — out of scope per project constraints; revisit if downstream demands.
- Linux GNU (non-musl) — defer; musl provides static binary that runs everywhere.
- Docker / OCI image — defer.
- Homebrew tap external maintenance beyond cargo-dist's auto-emitted formula — defer.
- Auto-update / version probe — out of scope.
- `cargo publish` from a non-CI environment — explicitly not supported.
- crates.io rate-limit handling (publish twice on the same minute) — operational concern, out of scope for Phase 6 PLAN.md.

</deferred>
