---
phase: 5
name: Diagnostics & Doctor
gathered: 2026-05-19
status: Ready for planning
mode: smart-discuss
---

# Phase 5: Diagnostics & Doctor — Context

<domain>
## Phase Boundary

**Goal:** A user (or a confused CI job) runs `bootroom doctor` and gets a single-screen preflight report — version, embedded qemu-wasm rev, detected browser, COOP/COEP self-check, config validity — closing the documented CLI surface.

**In scope (Phase 5):**
- New `Cmd::Doctor(DoctorArgs)` variant in `crates/bootroom/src/cli.rs`. `DoctorArgs` accepts optional `--config <PATH>` and `--format <human|json>` (default human). No `--kernel`, no `--verbose`.
- New `crates/bootroom/src/doctor_cmd.rs` runner.
- Checks performed:
  1. **bootroom version** — `env!("CARGO_PKG_VERSION")` + optional git short SHA captured at build time via `build.rs` (set `BOOTROOM_GIT_SHA` env or `"unknown"`).
  2. **qemu-wasm rev** — read embedded `qemu-wasm-rev.txt` (Make `qemu-assets` target writes it alongside the artifacts). Fallback `"unknown"` if missing.
  3. **Detected browser** — reuse `run_cmd::discover_chromium` (PATH walk + `$CHROMIUM` env from Phase 4 WR-07 fix). Report binary path + `--version` output. Failure → mark check as ✗ but continue.
  4. **COOP/COEP self-check** — boot in-process axum on `127.0.0.1:0`, GET `/`, assert `Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy: require-corp` headers present and correct.
  5. **Config validity** — if `$CWD/bootroom.toml` exists (or `--config <path>` provided), run the same `LoadedConfig::load_from_str` used by `serve`/`check`; report ✓ or first-error span. If no config file is present, mark as "no config" (informational, not a failure).
  6. **CLI surface summary line** — confirm the final subcommand set: `serve, run, check, init, doctor` (helps users verify their install).
- Output formats:
  - `--format human` (default): section-headered single-screen report with status glyphs (`✓` / `✗` / `–`). Exit 0 all pass; exit 1 if any failure. Failures also emit a one-line stderr summary for CI grep.
  - `--format json`: structured JSON: `{ "version": "...", "git_sha": "...", "checks": [{"name": "...", "status": "pass|fail|info", "detail": "..."}], "overall": "pass|fail" }`. Always to stdout.
- CLI subcommand order in `--help`: `serve, run, check, init, doctor` (frequency-of-use, not alphabetical).
- Verify clap derive already exposes `--help`, `--version`, and per-subcommand long-form help text; tighten any missing `about` strings.

**Out of scope (later phases):**
- Crates.io publish + cargo-dist + GitHub Release binaries — Phase 6.
- JUnit-style report formats — v2 (REP-01).
- Multi-arch guest checks — v2 (TGT-01).
- Network probes / connectivity tests — out of scope (bootroom is loopback-only).
- Self-update / version-check against crates.io — out of scope.

**Phase 5 requirements (from ROADMAP.md):** CLI-01 (full subcommand surface), DOC-01 (`bootroom doctor` checks).

</domain>

<decisions>
## Implementation Decisions

### Doctor Output & Behavior

- **Human + JSON output:** default `--format human` produces a section-headered single-screen report. `--format json` emits a stable schema (versioned: `{"schema_version": 1, ...}`) so downstream CI can pin without surprises.
- **Exit codes:** 0 = all checks pass (or only informational checks failed), 1 = any required check failed. Mirrors `run` mode's 0/1 (no need for 2/3 differentiation since doctor never executes scenarios).
- **Browser detection:** Reuse `crate::run_cmd::discover_chromium` (PATH walk + `$CHROMIUM`). Capture `chromium --version` output. Browser-missing reports a `✗` but does NOT fail doctor — kernel CI may run doctor before installing chromium.
- **COOP/COEP self-check:** Boot in-process axum (using `build_router` + `AppState::new_for_test` — same as the `run` driver), GET `/`, assert both headers present with the canonical values. This is the load-bearing check: if it fails, every kernel test downstream will silently break.

### Checks Inventory

- **Version:** `env!("CARGO_PKG_VERSION")` plus a `build.rs` capture of `git rev-parse --short HEAD` into `BOOTROOM_GIT_SHA` (default `"unknown"` if `git` is unavailable or the build is outside a git checkout — supply-chain reproducibility).
- **qemu-wasm rev:** Read `assets/qemu/qemu-wasm-rev.txt` (committed alongside the artifacts via the `make qemu-assets` target). Fallback `"unknown"` if missing. Doctor reports the value verbatim; does not validate it against anything.
- **Config validity:** Use `LoadedConfig::load_from_str` (Phase 3 + Phase 4 regex + after-resolution validation). If no `bootroom.toml` exists in CWD and `--config` not provided, mark as informational ("no config — that's fine if you only use `bootroom run --scenario`"). If config exists and fails to load, report first error with span (line:col) — same diagnostic shape as `bootroom check`.
- **CLI surface summary line:** Render the registered subcommands from clap (`Cmd::Serve | Run | Check | Init | Doctor`) so a user can quickly confirm their installed binary matches the documented surface. This catches a misbuild where one subcommand was accidentally cfg-gated out.

### CLI Surface Finalization

- **Subcommand order in `--help`:** `Serve, Run, Check, Init, Doctor` (frequency-of-use). Currently `Cmd::Serve` is first (Phase 1 stability gate); we ADD `Run` after `Serve` and `Doctor` last, keeping all existing variant order stable to avoid Phase-1 regression tests breaking.
- **Help / version:** clap derive already provides `--help` and `--version`. Audit each subcommand for a doc-string `///` first line — those become clap's `about` text. Tighten where missing.
- **Shared flags:** `DoctorArgs` does NOT use `CommonArgs` flatten (no `--kernel`, no `--verbose`). Just `--config` and `--format`. Doctor has a deliberately tiny surface.
- **Failure output destination:** Human mode prints the full multi-line report to stdout AND a one-line summary line to stderr when overall = fail (e.g., `bootroom doctor: 2/5 checks failed (browser, config)`). JSON mode is stdout-only. CI runners can `bootroom doctor || cat stderr | grep ...`.

### Claude's Discretion

All implementation details not pinned above are at Claude's discretion — exact JSON schema fields beyond `schema_version`/`overall`/`checks[]`, internal struct shapes, error-message wording, module layout, test-fixture choice.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets

- `crate::run_cmd::discover_chromium` — Phase 4 WR-07 pure-Rust PATH walk + `$CHROMIUM` env. Returns the binary path or a diagnostic error. Doctor calls this and degrades gracefully (`✗` not exit-1) when not found.
- `crate::server::build_router` + `AppState::new_for_test` — the same router served by `serve` and `run`. Doctor binds it on `127.0.0.1:0`, fetches `/`, and asserts headers.
- `bootroom_core::config::{LoadedConfig, parse_str, LoadError}` — single config loader with span-aware errors. Doctor uses the same parser as `check` to guarantee no drift.
- `clap derive` — `Cmd::Serve | Run | Check | Init` already in place; adding `Doctor` is one variant + one args struct + one dispatch arm in `main.rs`.
- `tracing` — already configured. Doctor emits structured fields (`check.name`, `check.status`) so the output formatter can render either flavor.
- `tokio::net::TcpListener::bind("127.0.0.1:0")` — ephemeral-port pattern lifted directly from `run_cmd.rs`.

### Established Patterns

- **Span-aware errors:** `LoadError` with optional `line`/`col`. Doctor renders these consistently.
- **Subcommand contract:** `Cmd::<Variant>(<VariantArgs>)`; main.rs dispatches; one Rust file per subcommand body (`server.rs`, `check_cmd.rs`, `init_cmd.rs`, `run_cmd.rs` → add `doctor_cmd.rs`).
- **Atomic commits + grep gates:** the project's TDD habit. Doctor's tests pin the human-format header strings + JSON schema field set.

### Integration Points

- `crates/bootroom/src/cli.rs` — add `Cmd::Doctor(DoctorArgs)` + `DoctorArgs` struct.
- `crates/bootroom/src/main.rs` — dispatch arm.
- `crates/bootroom/src/lib.rs` — `pub mod doctor_cmd;`.
- `crates/bootroom/build.rs` (NEW or extend Phase-1's existing) — capture git short SHA into `BOOTROOM_GIT_SHA` env at compile time.
- `crates/bootroom/Makefile` (Phase 1 `qemu-assets` target) — extend to write `assets/qemu/qemu-wasm-rev.txt` alongside the wasm/data/worker.js outputs. Doctor reads this at runtime (or via include_str in the embedded path).
- `crates/bootroom/tests/cli_subcommands.rs` — add tests pinning Doctor subcommand presence + help text.
- `crates/bootroom/tests/doctor_*.rs` — new integration tests: subcommand exit codes, JSON schema shape, human-format gate, COOP/COEP self-check in-process.

</code_context>

<specifics>
## Specific Ideas

- Doctor's COOP/COEP self-check is the load-bearing one: it's the same gate the browser uses for `crossOriginIsolated`. A pass here is the strongest single signal that everything downstream will work.
- Make doctor cheap (~100ms target): no Chromium launch, no qemu boot. Just version reads, header self-check, config parse. Keeps it suitable for CI preflight on every job.
- Human-format banner suggestion: `bootroom doctor — preflight checks`; section headers `## Version`, `## Browser`, `## Server headers`, `## Config`. Each section uses `✓ / ✗ / – name — detail`.

</specifics>

<deferred>
## Deferred Ideas

- JUnit / GitHub Actions report formats — v2 (REP-01/02).
- `--quick` / `--exhaustive` profiles — overkill at this stage; default is already fast.
- Self-update / version probe against crates.io — out of scope.
- Multi-arch guest support — v2 (TGT-01).
- Doctor running inside the headless browser to gather `crossOriginIsolated` + SAB facts — duplicate of `run --scenario`'s pre-check; not worth a separate code path here.

</deferred>
