# Phase 5: Diagnostics & Doctor — Research

**Researched:** 2026-05-19
**Domain:** CLI subcommand (preflight diagnostics) + build-time metadata capture
**Confidence:** HIGH

## Summary

Phase 5 adds one new subcommand — `bootroom doctor` — and finalizes the top-level CLI surface (`serve`, `run`, `check`, `init`, `doctor`). The phase is small, almost entirely additive, and reuses primitives that landed in earlier phases:

- `crate::run_cmd::discover_chromium` (Phase 4 WR-07) for browser detection
- `bootroom_core::config::LoadedConfig::load_from_str` (Phase 3) for config validity
- `crate::server::build_router` + `AppState::new_for_test` for in-process COOP/COEP self-check
- `crate::headers::{coop_layer, coep_layer}` middleware (Phase 1) is what the self-check verifies
- The existing `check_cmd.rs` / `init_cmd.rs` `ExitCode`-returning pattern

Two thin build-system extensions are needed:
1. `build.rs` capture of `git rev-parse --short HEAD` into `BOOTROOM_GIT_SHA` (with `"unknown"` fallback for non-git builds and `cargo install` from crates.io).
2. Makefile `qemu-assets` target writes `assets/qemu/qemu-wasm-rev.txt` from `git -C qemu-wasm rev-parse --short HEAD`, embedded into the binary via `include_str!` (or `include_dir`-derived lookup) at compile time.

**Primary recommendation:** Build `doctor_cmd.rs` around a small `Check { name, status, detail }` enum-shaped struct, run all five checks unconditionally (no early exit on first failure — the operator wants to see them all), then dispatch to one of two formatters (`human` / `json`) at the end. Reuse Phase 4's ASCII-only glyph convention from `verbose.rs` rather than the unicode `✓ / ✗ / –` glyphs CONTEXT.md mentions — this aligns with Phase 4 Open Question 4 (cross-platform CI; Windows console rendering).

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

#### Doctor Output & Behavior

- **Human + JSON output:** default `--format human` produces a section-headered single-screen report. `--format json` emits a stable schema (versioned: `{"schema_version": 1, ...}`) so downstream CI can pin without surprises.
- **Exit codes:** 0 = all checks pass (or only informational checks failed), 1 = any required check failed. Mirrors `run` mode's 0/1 (no need for 2/3 differentiation since doctor never executes scenarios).
- **Browser detection:** Reuse `crate::run_cmd::discover_chromium` (PATH walk + `$CHROMIUM`). Capture `chromium --version` output. Browser-missing reports a `✗` but does NOT fail doctor — kernel CI may run doctor before installing chromium.
- **COOP/COEP self-check:** Boot in-process axum (using `build_router` + `AppState::new_for_test` — same as the `run` driver), GET `/`, assert both headers present with the canonical values. This is the load-bearing check: if it fails, every kernel test downstream will silently break.

#### Checks Inventory

- **Version:** `env!("CARGO_PKG_VERSION")` plus a `build.rs` capture of `git rev-parse --short HEAD` into `BOOTROOM_GIT_SHA` (default `"unknown"` if `git` is unavailable or the build is outside a git checkout — supply-chain reproducibility).
- **qemu-wasm rev:** Read `assets/qemu/qemu-wasm-rev.txt` (committed alongside the artifacts via the `make qemu-assets` target). Fallback `"unknown"` if missing. Doctor reports the value verbatim; does not validate it against anything.
- **Config validity:** Use `LoadedConfig::load_from_str` (Phase 3 + Phase 4 regex + after-resolution validation). If no `bootroom.toml` exists in CWD and `--config` not provided, mark as informational ("no config — that's fine if you only use `bootroom run --scenario`"). If config exists and fails to load, report first error with span (line:col) — same diagnostic shape as `bootroom check`.
- **CLI surface summary line:** Render the registered subcommands from clap (`Cmd::Serve | Run | Check | Init | Doctor`) so a user can quickly confirm their installed binary matches the documented surface. This catches a misbuild where one subcommand was accidentally cfg-gated out.

#### CLI Surface Finalization

- **Subcommand order in `--help`:** `Serve, Run, Check, Init, Doctor` (frequency-of-use). Currently `Cmd::Serve` is first (Phase 1 stability gate); we ADD `Doctor` last, keeping all existing variant order stable to avoid Phase-1 regression tests breaking.
- **Help / version:** clap derive already provides `--help` and `--version`. Audit each subcommand for a doc-string `///` first line — those become clap's `about` text. Tighten where missing.
- **Shared flags:** `DoctorArgs` does NOT use `CommonArgs` flatten (no `--kernel`, no `--verbose`). Just `--config` and `--format`. Doctor has a deliberately tiny surface.
- **Failure output destination:** Human mode prints the full multi-line report to stdout AND a one-line summary line to stderr when overall = fail (e.g., `bootroom doctor: 2/5 checks failed (browser, config)`). JSON mode is stdout-only. CI runners can `bootroom doctor || cat stderr | grep ...`.

### Claude's Discretion

All implementation details not pinned above are at Claude's discretion — exact JSON schema fields beyond `schema_version`/`overall`/`checks[]`, internal struct shapes, error-message wording, module layout, test-fixture choice.

### Deferred Ideas (OUT OF SCOPE)

- JUnit / GitHub Actions report formats — v2 (REP-01/02).
- `--quick` / `--exhaustive` profiles — overkill at this stage; default is already fast.
- Self-update / version probe against crates.io — out of scope.
- Multi-arch guest support — v2 (TGT-01).
- Doctor running inside the headless browser to gather `crossOriginIsolated` + SAB facts — duplicate of `run --scenario`'s pre-check; not worth a separate code path here.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| CLI-01 | Top-level subcommands are short verbs: `serve`, `run`, `init`, `check`, `doctor`, plus `--version` and `--help`. | Add `Cmd::Doctor(DoctorArgs)` variant in `cli.rs`; audit `about` doc-strings; pin subcommand presence in `tests/cli_subcommands.rs`. |
| DOC-01 | `bootroom doctor` reports bootroom version, embedded qemu-wasm rev, detected browser, COOP/COEP self-check on `/`, and current config validity. | New `doctor_cmd.rs` runs five `Check { name, status, detail }` checks and prints them via a `human` / `json` formatter; build.rs captures git SHA; Makefile writes `qemu-wasm-rev.txt`. |
</phase_requirements>

## Project Constraints (from CLAUDE.md)

- **Single static binary:** doctor must add zero runtime dependencies. Reuse `reqwest` (already a dev-dep — moving to a regular dep for `coop_coep_headers.rs` is acceptable) **OR** prefer `axum::oneshot` via `tower::ServiceExt` (already a regular dep) to skip the HTTP loopback entirely. See "Architecture Patterns → COOP/COEP Self-Check" below.
- **No external `which` invocation:** Phase 4 WR-07 already enforces pure-Rust `$PATH` walking via `which_via_path_env`. Doctor's browser-detection check must continue to use this — do not introduce a `Command::new("which")` shell-out.
- **MIT OR Apache-2.0 license:** any new crate must satisfy this. None needed for Phase 5 (`serde_json` and `clap` are already in the tree).
- **GSD workflow:** all file changes flow through phase plans. Phase 5 plans go in `.planning/phases/05-diagnostics-doctor/05-XX-PLAN.md`.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| `doctor` subcommand entrypoint | CLI dispatch (main.rs) | — | Exactly mirrors how `check` / `init` / `run` dispatch today. |
| Version + git SHA capture | Build-time (build.rs) | Runtime (env!) | Git is unavailable on crates.io builds; build.rs handles fallback to `"unknown"`. Runtime just reads the compile-time constant. |
| qemu-wasm rev capture | Build-time (Makefile `qemu-assets`) | Runtime (include_str / Dir::get_file) | Rev is determined at the moment `make qemu-assets` ran; embedded as a file the binary reads at runtime. |
| Browser detection | Runtime (doctor_cmd → run_cmd::discover_chromium) | — | Same primitive used by `bootroom run`; doctor degrades to ✗ instead of exit-3 on failure. |
| COOP/COEP self-check | Runtime (doctor_cmd → build_router + oneshot/loopback) | — | Verifies the same layer (`headers.rs`) the browser would check. In-process via `tower::ServiceExt::oneshot` is the minimal substrate. |
| Config validity | Runtime (doctor_cmd → LoadedConfig) | — | Identical parser to `bootroom check`; no drift possible. |
| Output formatting | Runtime (doctor_cmd → human / json formatter) | — | Pure functions over `Vec<Check>`; trivially testable without I/O. |

## Standard Stack

### Core (already in workspace — no additions)
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `clap` (derive) | 4.6.1 | `Cmd::Doctor(DoctorArgs)` variant and `--format` parser | Already used for every other subcommand; ValueEnum derive renders `--format <human\|json>` from a Rust enum cleanly. [VERIFIED: workspace deps] |
| `serde_json` | 1.x | JSON output formatter | Already a workspace dep; `serde_json::to_writer_pretty` to stdout for the JSON branch. [VERIFIED: workspace deps] |
| `serde` | 1.0.228 | Derive `Serialize` on the report types | Already used everywhere. [VERIFIED: workspace deps] |
| `tower` | 0.5.3 (with `util` feature in dev-deps) | `ServiceExt::oneshot` for in-process router self-check | Phase 1 tests already use this pattern (`tests/server.rs::test_router_returns_coop_coep_on_404`). Promote `tower::util` from dev to regular dep, OR keep doctor's COOP/COEP check in an integration test only and let production code bind a real loopback listener. See "Architecture Patterns → COOP/COEP Self-Check". [VERIFIED: workspace deps] |
| `tokio` | 1.52.3 | Async runtime + `TcpListener` for the loopback variant | Already in use across `server.rs` / `run_cmd.rs`. [VERIFIED: workspace deps] |
| `reqwest` | 0.12 (dev-dep) | HTTP client if loopback variant is chosen | Already pulled in as a dev-dep for `tests/coop_coep_headers.rs`; doctor MAY choose to keep COOP/COEP check as a tower-oneshot to avoid promoting reqwest to a regular dep. [VERIFIED: Cargo.toml] |

### Build-time
| Tool | Purpose | Why Standard |
|------|---------|--------------|
| `std::process::Command` (in `build.rs`) | Invoke `git rev-parse --short HEAD` | Build scripts conventionally shell out to git for SHA capture. The pattern is well-trodden (see `rustc -V` capture in many crates). No `git2` / `gix` dependency needed for one shell-out. |
| `std::fs::write` (in Makefile recipe) | Write `qemu-wasm-rev.txt` | One line in the `qemu-assets` recipe: `git -C $(QEMU_WASM_DIR) rev-parse --short HEAD > $(QEMU_OUT_DIR)/qemu-wasm-rev.txt`. |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `tower::ServiceExt::oneshot` for self-check | Bind `127.0.0.1:0` + real HTTP via reqwest | Loopback variant is closer to "what the browser sees" but adds reqwest as a regular dep (+ a port-bind + async overhead). Oneshot variant is purer Rust, no socket, faster. Both are valid; oneshot is the recommended default. |
| `serde_json::to_writer_pretty` | Hand-rolled JSON | `serde_json` is already a dep; no reason to hand-roll. |
| `git2` / `gix` crate in build.rs | `Command::new("git")` | Crate-based git adds 30+ transitive deps for one SHA read. `Command::new("git").arg("rev-parse")...` is 8 lines and fails gracefully. |
| `include_str!` for `qemu-wasm-rev.txt` | Look it up via `embed::QEMU.get_file(...)` | `include_str!` is one line and produces a `&'static str`; `Dir::get_file` returns `Option<&File>` and forces an `unwrap_or("unknown")` at runtime. Either works; `include_str!` is simpler. Caveat: `include_str!` resolves the path at compile time, so a missing file produces a compile error — must guard with `cfg` or use the `Option` shape. **Recommend** `option_env!`-style via a `build.rs` re-export, OR `include_dir`'s `QEMU.get_file("qemu-wasm-rev.txt").and_then(\|f\| f.contents_utf8()).unwrap_or("unknown")` — the latter integrates with the existing embed pattern and degrades gracefully when the rev file is absent. [VERIFIED: existing `embed.rs` uses `include_dir!`] |

## Architecture Patterns

### System Architecture Diagram

```
                        bootroom doctor [--format human|json] [--config PATH]
                                                │
                                                ▼
                                ┌────────────────────────────────┐
                                │  doctor_cmd::run(args) -> ExitCode │
                                └────────────────────────────────┘
                                                │
                            ┌───────────────────┼───────────────────┐
                            │                   │                   │
                            ▼                   ▼                   ▼
              ┌─────────────────┐    ┌──────────────────┐    ┌──────────────────┐
              │ check_version() │    │ check_qemu_rev() │    │ check_browser()  │
              │ env!() + GIT_SHA│    │ embed::QEMU.get  │    │ run_cmd::         │
              │                 │    │ ("qemu-wasm-rev")│    │ discover_chromium│
              │ -> Check::info  │    │ -> Check::info   │    │ -> Check::pass/✗ │
              └─────────────────┘    └──────────────────┘    └──────────────────┘
                            │                   │                   │
                            └───────────────────┼───────────────────┘
                                                │
                            ┌───────────────────┼───────────────────┐
                            ▼                                       ▼
              ┌──────────────────────┐                ┌──────────────────────┐
              │ check_headers()       │                │ check_config()       │
              │ build_router(state)   │                │ LoadedConfig::       │
              │ + oneshot("/")        │                │ load_from_str        │
              │ assert COOP+COEP      │                │ -> pass/fail/info    │
              │ -> pass/fail          │                │  (no file = info)   │
              └──────────────────────┘                └──────────────────────┘
                            │                                       │
                            └───────────────────┬───────────────────┘
                                                │
                                                ▼
                              ┌────────────────────────────────┐
                              │ Vec<Check> = [v, q, b, h, c]   │
                              │ + cli_surface summary line     │
                              └────────────────────────────────┘
                                                │
                            ┌───────────────────┴───────────────────┐
                            ▼                                       ▼
              ┌──────────────────────┐                ┌──────────────────────┐
              │ format_human(&checks)│                │ format_json(&checks) │
              │ -> stdout            │                │ serde_json -> stdout │
              │ + stderr summary     │                │                      │
              │   when overall=fail  │                │                      │
              └──────────────────────┘                └──────────────────────┘
                            │                                       │
                            └───────────────────┬───────────────────┘
                                                ▼
                                  ExitCode (0 = all pass, 1 = any fail)
```

### Recommended Project Structure (additions only)
```
crates/bootroom/src/
├── doctor_cmd.rs            # NEW — runs the five checks + dispatches to formatter
├── cli.rs                   # MOD — add Cmd::Doctor(DoctorArgs) variant + DoctorArgs struct
├── main.rs                  # MOD — add dispatch arm for Cmd::Doctor
├── lib.rs                   # MOD — `pub mod doctor_cmd;` + re-export DoctorArgs
└── build.rs                 # MOD — capture BOOTROOM_GIT_SHA via `git rev-parse --short HEAD`

crates/bootroom/assets/qemu/
└── qemu-wasm-rev.txt        # NEW (build artifact) — written by `make qemu-assets`

crates/bootroom/tests/
├── doctor_subcommand.rs     # NEW — pin Doctor CLI shape (help text, --format flag)
├── doctor_human_format.rs   # NEW — gate section headers + glyphs + summary line shape
├── doctor_json_schema.rs    # NEW — pin schema_version=1 + required keys + overall field
├── doctor_exit_codes.rs     # NEW — pin 0-on-pass, 1-on-fail
└── doctor_headers_check.rs  # NEW — in-process self-check passes against build_router

Makefile  (repo root)        # MOD — `qemu-assets` recipe writes qemu-wasm-rev.txt
```

### Pattern 1: ASCII-only glyphs (not Unicode)

**What:** Use ASCII prefixes `[ok]`, `[FAIL]`, `[--]` (or single-char `+` `-` `~`) instead of `✓ / ✗ / –` despite CONTEXT.md mentioning unicode glyphs.
**When to use:** All terminal output across both formatters.
**Why:** Phase 4 `verbose.rs` already established ASCII-only output per Open Question 4 (Windows console rendering, CI log grep stability). Doctor is a sibling command and should match.
**Example:**
```rust
// Source: crates/bootroom/src/verbose.rs (Phase 4)
pub const GLYPH_PASS: &str = "+ ";
pub const GLYPH_FAIL: &str = "- ";
pub const GLYPH_INFO: &str = "~ ";   // new for doctor
```

If CONTEXT.md's unicode glyphs are non-negotiable, surface this as a discuss-phase question — but the planning default should be ASCII to stay consistent with Phase 4.

### Pattern 2: `Check { name, status, detail }` record

**What:** A small enum-shaped struct that every check returns.
**When to use:** As the unit currency between check functions and formatters.
**Example:**
```rust
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus { Pass, Fail, Info }

#[derive(Debug, Clone, serde::Serialize)]
pub struct Check {
    pub name: String,         // "version" | "qemu_wasm_rev" | "browser" | "headers" | "config"
    pub status: CheckStatus,
    pub detail: String,       // free-form: version string, error message, path, etc.
}
```

Then:
```rust
async fn run_doctor(args: DoctorArgs) -> ExitCode {
    let checks = vec![
        check_version(),
        check_qemu_rev(),
        check_browser(),
        check_headers().await,
        check_config(&args.config),
    ];
    let overall_failed = checks.iter().any(|c| matches!(c.status, CheckStatus::Fail));
    match args.format {
        OutputFormat::Human => format_human(&checks, overall_failed),
        OutputFormat::Json  => format_json(&checks, overall_failed),
    }
    if overall_failed { ExitCode::from(1) } else { ExitCode::SUCCESS }
}
```

### Pattern 3: Build-time env var via `build.rs` + `option_env!`

**What:** Capture git SHA in build.rs, emit `cargo:rustc-env=BOOTROOM_GIT_SHA=<sha>`, read at runtime via `env!()` (mandatory — emitted in every case) or `option_env!()` (optional).
**When to use:** Any compile-time-known metadata (git SHA, build profile, build host).
**Example:**
```rust
// build.rs — at the end of main()
let sha = std::process::Command::new("git")
    .args(["rev-parse", "--short", "HEAD"])
    .output()
    .ok()
    .filter(|o| o.status.success())
    .and_then(|o| String::from_utf8(o.stdout).ok())
    .map(|s| s.trim().to_string())
    .unwrap_or_else(|| "unknown".to_string());
println!("cargo:rustc-env=BOOTROOM_GIT_SHA={sha}");
println!("cargo:rerun-if-changed=.git/HEAD");
println!("cargo:rerun-if-changed=.git/refs");
```

```rust
// doctor_cmd.rs
fn check_version() -> Check {
    let v = env!("CARGO_PKG_VERSION");
    let sha = env!("BOOTROOM_GIT_SHA");
    Check {
        name: "version".into(),
        status: CheckStatus::Info,
        detail: format!("bootroom {v} ({sha})"),
    }
}
```

### Pattern 4: COOP/COEP Self-Check — two equally viable variants

**Variant A (recommended): `tower::ServiceExt::oneshot`** — purely in-process, no socket, no async server task. Mirrors `crates/bootroom/src/server.rs::tests::test_router_returns_coop_coep_on_404`:

```rust
async fn check_headers() -> Check {
    use tower::ServiceExt;
    use axum::{body::Body, http::Request};
    let kernel = std::env::temp_dir().join("bootroom-doctor-noop");
    // safe no-op kernel path — new_for_test does NOT require the file to exist
    let state = std::sync::Arc::new(
        crate::AppState::new_for_test(kernel, None)
    );
    let app = crate::build_router(state);
    let resp = match app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
    {
        Ok(r) => r,
        Err(e) => return Check {
            name: "headers".into(),
            status: CheckStatus::Fail,
            detail: format!("router oneshot failed: {e}"),
        },
    };
    let coop = resp.headers().get("cross-origin-opener-policy").and_then(|v| v.to_str().ok());
    let coep = resp.headers().get("cross-origin-embedder-policy").and_then(|v| v.to_str().ok());
    if coop == Some("same-origin") && coep == Some("require-corp") {
        Check { name: "headers".into(), status: CheckStatus::Pass,
            detail: "COOP=same-origin, COEP=require-corp on /".into() }
    } else {
        Check { name: "headers".into(), status: CheckStatus::Fail,
            detail: format!("expected COOP=same-origin, COEP=require-corp; got COOP={coop:?}, COEP={coep:?}") }
    }
}
```

Promote `tower = { workspace = true, features = ["util"] }` from `[dev-dependencies]` to `[dependencies]` in `crates/bootroom/Cargo.toml`. `tower::util` is tiny.

**Variant B: real loopback bind + reqwest** — closer to "what a browser would see" but heavier (binds a port, spawns a task, adds reqwest as a regular dep, tears down via abort). Mirrors `run_cmd.rs::run_inner` steps 5-8. Not recommended unless we discover that some response transformation (e.g. compression middleware) only applies on a real connection.

### Pattern 5: Embedded text file via include_dir lookup

**What:** Read `qemu-wasm-rev.txt` via the existing `embed::QEMU` `Dir`.
**When to use:** Any small text file co-located with the qemu artifacts.
**Example:**
```rust
fn check_qemu_rev() -> Check {
    let rev = crate::embed::QEMU
        .get_file("qemu-wasm-rev.txt")
        .and_then(|f| f.contents_utf8())
        .map(str::trim)
        .unwrap_or("unknown");
    Check {
        name: "qemu_wasm_rev".into(),
        status: CheckStatus::Info,
        detail: format!("qemu-wasm rev {rev}"),
    }
}
```

Caveat: `include_dir!` captures the directory contents at compile time. If `qemu-wasm-rev.txt` is absent at compile time, the `get_file` returns `None` and `unwrap_or("unknown")` fires. This is the intended degradation path for dev builds where `make qemu-assets` has not yet written the file.

### Anti-Patterns to Avoid

- **`std::process::exit()` inside a check function:** doctor's whole point is to run ALL checks and present a summary; exit-on-first-fail kills that. Each check returns a `Check`; only `run_doctor` translates to `ExitCode` at the end.
- **Unicode glyphs in stderr:** breaks Windows console rendering and CI log grep. Use ASCII (per Phase 4 precedent).
- **Re-implementing config loading:** any drift from `LoadedConfig::load_from_str` will eventually surface as "check passed but serve failed." Reuse the exact same call.
- **Hand-rolled `which`:** Phase 4 already provides `which_via_path_env` (pure Rust, no shell-out). Reuse.
- **JSON shape changes without bumping `schema_version`:** downstream CI may parse this; treat `schema_version: 1` as a contract.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| TOML parse + validate | Custom validator | `LoadedConfig::load_from_str` | Single source of truth; Phase 3 + 4 already capture every error variant including spans. |
| Chromium binary discovery | Custom `$PATH` walk | `crate::run_cmd::discover_chromium` | Already pure-Rust, already tested, already handles `$CHROMIUM` override. Note: `discover_chromium` is private to `run_cmd.rs` — expose via a `pub(crate) fn discover_chromium()` or move to a shared module. |
| Header checks | Custom HTTP client | `build_router` + `tower::ServiceExt::oneshot` | Phase 1's `tests/server.rs` proves the pattern works in-process. |
| JSON output | Manual string building | `serde_json::to_writer_pretty` | A single typo in hand-rolled JSON breaks every CI consumer. |
| ExitCode dispatch | Direct `std::process::exit` calls | Return `ExitCode` from `run()` | Mirrors `check_cmd::run` / `init_cmd::run` exactly; integration tests can call doctor as a library function. |
| Git SHA capture in main code | Runtime `git` invocation | `build.rs` env emission | Runtime git invocation breaks in installed binaries (no .git tree). |

**Key insight:** Phase 5's job is composition, not invention. Every primitive doctor needs already exists in the tree from Phases 1-4.

## Runtime State Inventory

> N/A — Phase 5 is greenfield additive work. No renames, no migrations.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `git` (CLI) | build.rs git SHA capture | ✓ | 2.51.2 | `"unknown"` literal — implemented inline; required for `cargo install` from crates.io which has no .git tree |
| `chromium` (CLI) | Doctor's browser-detection check at runtime | ✓ (probe-time) | per-system | Already handled: `discover_chromium` returns Err → Check::Fail (does NOT exit 1) |
| Rust toolchain | Build | ✓ | 1.85 MSRV | none — required |
| `make` | qemu-assets Makefile recipe | ✓ | GNU Make 4.x | none — only invoked when rebuilding qemu-wasm artifacts (rare; output committed) |
| `docker` | qemu-assets Makefile recipe | ✓ | 24+ | none — same caveat as `make`; not needed for `bootroom doctor` itself |

**Missing dependencies with no fallback:** None.
**Missing dependencies with fallback:** `git` at build time falls back to `"unknown"` SHA via build.rs guard.

## Common Pitfalls

### Pitfall 1: `build.rs` git invocation breaks `cargo install` from crates.io

**What goes wrong:** crates.io packages don't ship `.git/`. A `Command::new("git").args(["rev-parse", "--short", "HEAD"])` returns `ErrorKind::NotFound` or exits non-zero, and a naive `unwrap()` aborts the build.
**Why it happens:** The package tarball is what gets uploaded, not the git checkout.
**How to avoid:** The build.rs git capture MUST tolerate (a) `git` not on `$PATH`, (b) `git` running but failing because there's no repo, (c) `git` running but `HEAD` is detached / missing. Use the `.ok().filter(...).and_then(...)` chain shown in Pattern 3; never `.unwrap()`. Fallback string MUST be `"unknown"` (lowercase, no whitespace) so doctor can emit it verbatim.
**Warning signs:** A `cargo install bootroom` from crates.io fails with a build.rs panic mentioning `Command::new("git")` or `rev-parse`.

### Pitfall 2: `qemu-wasm-rev.txt` missing in dev builds

**What goes wrong:** Operator runs `cargo build` before `make qemu-assets` has been run on this branch. `include_dir!` captures the directory but `get_file("qemu-wasm-rev.txt")` returns `None`.
**Why it happens:** The qemu artifacts in `assets/qemu/` are committed to git but the rev file is new in Phase 5 — pre-Phase-5 branches don't have it.
**How to avoid:** The runtime lookup must use `unwrap_or("unknown")` (Pattern 5). The Makefile must write the file every time `qemu-assets` is run, so once Phase 5 + a fresh `make qemu-assets` lands, the file is committed. Test the missing-file path explicitly in `tests/doctor_subcommand.rs`.
**Warning signs:** Doctor reports `qemu-wasm rev unknown` even on a fresh-from-Makefile checkout — verify the Makefile recipe ran `git -C qemu-wasm rev-parse --short HEAD > .../qemu-wasm-rev.txt` and the file is committed.

### Pitfall 3: `include_str!` vs `include_dir!` path resolution

**What goes wrong:** `include_str!("../../assets/qemu/qemu-wasm-rev.txt")` fails to compile if the file doesn't exist; there's no graceful degradation at compile time.
**Why it happens:** `include_str!` is a macro that requires the path to resolve at compile time.
**How to avoid:** Use the existing `include_dir!`-derived `embed::QEMU.get_file(...)` lookup which returns `Option<&File>` — degrades to `None` gracefully (Pattern 5). The Phase 1 build.rs presence-check list (`REQUIRED` in `build.rs`) deliberately omits `qemu-wasm-rev.txt` so missing-rev does NOT fail the build.
**Warning signs:** `cargo build` fails on a pre-Phase-5 branch with "file not found" pointing at `qemu-wasm-rev.txt`.

### Pitfall 4: Browser detection failure cascading to exit 1

**What goes wrong:** A CI runner that hasn't installed chromium yet calls `bootroom doctor` early in its pipeline (e.g., as a preflight). Doctor returns exit 1 because browser is missing, blocking the rest of the pipeline.
**Why it happens:** Operator instinctively maps "missing browser" to "fail."
**How to avoid:** CONTEXT.md decision: browser-missing → `✗` glyph in the report but does NOT contribute to exit-1. Treat browser as `CheckStatus::Info` (informational) when missing, or introduce a third `CheckStatus::Warn` that the overall-fail logic ignores. **Recommendation:** keep three states (`Pass`, `Fail`, `Info`); browser-missing emits `Info` with detail "not found — install for `bootroom run`". This aligns with the JSON schema decision (`status: "pass|fail|info"`).
**Warning signs:** CI pipelines fail at the doctor step when they would have succeeded; users complain about preflight false positives.

### Pitfall 5: JSON schema drift without `schema_version` bump

**What goes wrong:** A future change adds/renames a field in the JSON output; existing CI consumers parse `null` or panic.
**Why it happens:** No versioning discipline.
**How to avoid:** Top-level `schema_version: 1` is REQUIRED (CONTEXT decision). Pin the field set in `tests/doctor_json_schema.rs` so any addition forces a test update + a schema_version bump conversation. Any breaking change → `schema_version: 2`.
**Warning signs:** Downstream CI scripts break on a bootroom version bump.

### Pitfall 6: `tower` not in regular deps

**What goes wrong:** `doctor_cmd.rs` uses `tower::ServiceExt::oneshot`, which today is only in `[dev-dependencies]`. Production build fails.
**Why it happens:** Phase 1 only needed tower in tests.
**How to avoid:** The plan that introduces doctor's headers check MUST also bump `tower = { workspace = true, features = ["util"] }` from `[dev-dependencies]` to `[dependencies]` in `crates/bootroom/Cargo.toml`. The workspace already pins `tower = "0.5.3"`.
**Warning signs:** Production build errors `unresolved import tower::ServiceExt`.

### Pitfall 7: `discover_chromium` private to `run_cmd.rs`

**What goes wrong:** `doctor_cmd.rs` cannot call `run_cmd::discover_chromium` because it's a private `fn`.
**Why it happens:** Phase 4 scoped the visibility tightly.
**How to avoid:** Phase 5's plan that adds doctor must change `fn discover_chromium()` to `pub(crate) fn discover_chromium()` in `run_cmd.rs`. Alternatively, move the discovery code (plus its `which_via_path_env`, `discover_chromium_with_candidates`, `ShellQuote` helpers) into a new `chromium_discovery` module — only do this if Phase 6 will need it from a third site. **Recommendation:** the minimal `pub(crate)` exposure now; defer module extraction.
**Warning signs:** Compile error "function `discover_chromium` is private."

### Pitfall 8: Stderr summary line gets captured by stdout-only CI

**What goes wrong:** Operator writes `bootroom doctor > report.txt` and never sees the failure summary because it went to stderr.
**Why it happens:** CONTEXT.md decision: failure summary is stderr-only (so `bootroom doctor > report.txt` doesn't pollute the report with the summary).
**How to avoid:** Document this behavior in the doc-string. Operators wanting a unified log redirect with `bootroom doctor &> report.txt`. The decision is sound; the doc is the mitigation.
**Warning signs:** User asks "why did my CI think doctor passed when the JSON said fail" — usually they're parsing stdout only.

### Pitfall 9: clap doc-strings drift between `///` first line and `--help`

**What goes wrong:** Operator writes a multi-line `///` doc comment with a verbose first line; clap renders the entire first paragraph in `--help` and the output is unreadable.
**Why it happens:** clap's derive treats the first line of the `///` doc as `about` and the rest as `long_about`. A first line longer than ~80 chars wraps badly.
**How to avoid:** Audit every `Cmd::*` and `*Args` doc-string. First line ≤ 80 chars, declarative. Subsequent paragraphs (for `--help` long mode) are fine.
**Warning signs:** `bootroom --help` lines wrap mid-sentence; users complain `--help` is hard to scan.

## Code Examples

### Adding `Cmd::Doctor(DoctorArgs)`

```rust
// crates/bootroom/src/cli.rs — extend the enum
#[derive(Debug, Subcommand)]
pub enum Cmd {
    Serve(ServeArgs),
    Run(RunArgs),
    Check(CheckArgs),
    Init(InitArgs),
    /// Run preflight checks (version, browser, headers, config).
    ///
    /// Exits 0 when all required checks pass; 1 otherwise. Designed for
    /// CI preflight steps and operator self-diagnosis before filing bugs.
    Doctor(DoctorArgs),
}

#[derive(Debug, Args, Clone)]
pub struct DoctorArgs {
    /// Path to bootroom.toml; default = ./bootroom.toml. Missing file is informational, not a failure.
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,

    /// Output format: human (default, section-headered report) or json (stable schema, schema_version=1).
    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    pub format: OutputFormat,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum OutputFormat {
    Human,
    Json,
}
```

### Dispatch in `main.rs`

```rust
match cli.cmd {
    Cmd::Serve(args)  => { bootroom::server::run(args).await?; Ok(ExitCode::SUCCESS) }
    Cmd::Run(args)    => Ok(bootroom::run_cmd::run(args).await),
    Cmd::Check(args)  => Ok(bootroom::check_cmd::run(args)),
    Cmd::Init(args)   => Ok(bootroom::init_cmd::run(&args)),
    Cmd::Doctor(args) => Ok(bootroom::doctor_cmd::run(args).await),
}
```

`doctor_cmd::run` is `async` because the COOP/COEP self-check awaits `app.oneshot(...)`.

### JSON Output Schema (formal)

```json
{
  "schema_version": 1,
  "version": "0.1.0",
  "git_sha": "1a224cf",
  "checks": [
    {"name": "version",        "status": "info", "detail": "bootroom 0.1.0 (1a224cf)"},
    {"name": "qemu_wasm_rev",  "status": "info", "detail": "qemu-wasm rev 0ef7b4e"},
    {"name": "browser",        "status": "pass", "detail": "/usr/bin/chromium (Chromium 144.0.6929.0)"},
    {"name": "headers",        "status": "pass", "detail": "COOP=same-origin, COEP=require-corp on /"},
    {"name": "config",         "status": "info", "detail": "no bootroom.toml in CWD (use --config to specify)"},
    {"name": "cli_surface",    "status": "info", "detail": "serve, run, check, init, doctor"}
  ],
  "overall": "pass"
}
```

Pin in `tests/doctor_json_schema.rs`:
- `schema_version == 1`
- `version` is non-empty
- `git_sha` is present (may be `"unknown"`)
- `checks` is an array
- Every `check.name` is one of the six known names
- Every `check.status` is one of `"pass" | "fail" | "info"`
- `overall` is `"pass"` or `"fail"`

### Human Output Format (formal)

```
bootroom doctor — preflight checks

## Version
~ version          bootroom 0.1.0 (1a224cf)
~ qemu_wasm_rev    qemu-wasm rev 0ef7b4e

## Browser
+ browser          /usr/bin/chromium (Chromium 144.0.6929.0)

## Server headers
+ headers          COOP=same-origin, COEP=require-corp on /

## Config
~ config           no bootroom.toml in CWD (use --config to specify)

## CLI surface
~ cli_surface      serve, run, check, init, doctor

Overall: pass
```

On failure, the same body plus a stderr line: `bootroom doctor: 2/6 checks failed (browser, headers)`.

### `build.rs` extension (snippet, append after existing body)

```rust
// At end of build.rs main()
let sha = std::process::Command::new("git")
    .args(["rev-parse", "--short", "HEAD"])
    .output()
    .ok()
    .filter(|o| o.status.success())
    .and_then(|o| String::from_utf8(o.stdout).ok())
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())
    .unwrap_or_else(|| "unknown".to_string());
println!("cargo:rustc-env=BOOTROOM_GIT_SHA={sha}");
// HEAD-move invalidation. .git/HEAD changes on every commit/checkout; .git/refs covers branch updates.
println!("cargo:rerun-if-changed=.git/HEAD");
println!("cargo:rerun-if-changed=.git/refs");
```

### Makefile `qemu-assets` extension

Add as a new Step 5b in the recipe (between current Step 5 and the final echo lines):

```make
@echo ">>> Step 5b/5: Recording qemu-wasm git rev..."
@git -C $(QEMU_WASM_DIR) rev-parse --short HEAD > $(QEMU_OUT_DIR)/qemu-wasm-rev.txt
@echo "  qemu-wasm rev: $$(cat $(QEMU_OUT_DIR)/qemu-wasm-rev.txt)"
```

Update the existing `clean-qemu-assets` target to also remove `qemu-wasm-rev.txt`.

`build.rs` should NOT add `qemu-wasm-rev.txt` to its `REQUIRED` list — its absence is non-fatal (degrades to `"unknown"` at runtime).

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Hand-rolled JSON output | `serde_json` | Always preferred in Rust | Stable, tested, zero-bug. |
| `git2` / `gix` build dep for SHA | `Command::new("git")` shell-out | Standard pattern | Saves 30+ transitive deps. |
| `Command::new("which")` shell-out | Pure-Rust `$PATH` walk | Phase 4 WR-07 | Works on minimal CI images, Alpine, Windows. |
| Phase 4's hardcoded glyph chars | ASCII-only `verbose.rs` glyphs | Phase 4 OQ #4 | Windows console rendering, CI log grep stability. |

**Deprecated/outdated:** Nothing in this phase. Phase 5 builds on stable primitives.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `tower::util` feature can be promoted from dev-dep to regular dep without breaking binary size or version conflicts | Standard Stack | Low — `tower` is already in the dep graph via `tower-http`; promoting one feature adds negligible size. [ASSUMED] |
| A2 | ASCII-only glyph convention (Phase 4 precedent) is preferred over CONTEXT.md's unicode `✓ / ✗ / –` | Pattern 1 | Medium — if user prefers unicode, the human formatter changes but the JSON schema is unaffected. Worth confirming in discuss-phase. [ASSUMED] |
| A3 | `cargo install bootroom` from crates.io will not include `.git/`, so build.rs git invocation must tolerate failure | Pitfall 1 | High if wrong but unlikely — this is a documented crates.io behavior. [CITED: cargo book — "Cargo.toml manifest"] |
| A4 | `AppState::new_for_test` does NOT require the kernel file to exist | Pattern 4, Variant A | Low — verified by reading `state.rs::new_for_test` which uses `canonicalize(...).unwrap_or_else(|_| kernel.clone())` [VERIFIED: state.rs:157] |
| A5 | `include_dir!`-captured directory listing is fixed at compile time and tolerates missing `qemu-wasm-rev.txt` via `Option<&File>` | Pattern 5 | Low — `include_dir` crate docs explicitly call out the `Option` return type. [CITED: docs.rs/include_dir] |
| A6 | The current `bootroom run` ExitCode contract (0/1/2/3) where 0=pass, 1=fail, 2=config error, 3=startup error does NOT extend to doctor (CONTEXT pins 0/1 only) | User Constraints | None — explicitly locked in CONTEXT. |

## Open Questions

1. **Should the unicode glyph decision in CONTEXT.md (`✓ / ✗ / –`) override the Phase 4 ASCII convention?**
   - What we know: CONTEXT.md `<specifics>` says `✓ / ✗ / –`. Phase 4 `verbose.rs` is ASCII-only with documented rationale (Windows + CI grep).
   - What's unclear: Whether the user is aware of the conflict.
   - Recommendation: Plan author should propose ASCII (matching Phase 4) and flag in plan-checker review. If user prefers unicode, the change is two const strings in `doctor_cmd.rs` — low cost.

2. **Should `cli_surface` be a separate check or merged into `version`?**
   - What we know: CONTEXT explicitly lists it as check #6.
   - What's unclear: Whether it provides operational value vs. just bloating the report.
   - Recommendation: Keep as a separate check (status `info`); zero additional cost, useful for verifying misbuilds where a subcommand was cfg-gated out.

3. **Should `bootroom doctor` reuse `discover_chromium`'s `BOOTROOM_CHROMIUM_ARGS` env handling?**
   - What we know: `discover_chromium` is the discovery primitive only; `BOOTROOM_CHROMIUM_ARGS` is parsed inside `run_inner` for launch.
   - What's unclear: Whether doctor should also report `BOOTROOM_CHROMIUM_ARGS` for diagnosis.
   - Recommendation: Out of scope for v1. Add as an `info`-level check in a future release if operators ask.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust integration tests via `cargo test` (workspace test target) + `tokio::test` |
| Config file | `crates/bootroom/Cargo.toml` `[dev-dependencies]` |
| Quick run command | `cargo test --package bootroom doctor` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| CLI-01 | `bootroom --help` lists serve, run, check, init, doctor | integration (subprocess) | `cargo test --package bootroom --test cli_subcommands top_level_help_lists_doctor` | ❌ Wave 0 (extend existing `tests/cli_subcommands.rs`) |
| CLI-01 | `bootroom doctor --help` mentions --config and --format | integration (subprocess) | `cargo test --package bootroom --test doctor_subcommand doctor_help_mentions_flags` | ❌ Wave 0 |
| DOC-01 | Doctor reports version with git SHA | unit + integration | `cargo test --package bootroom doctor_cmd::tests::check_version_includes_sha` | ❌ Wave 0 |
| DOC-01 | Doctor reads qemu-wasm-rev.txt from embedded dir | unit | `cargo test --package bootroom doctor_cmd::tests::check_qemu_rev_reads_embedded_file` | ❌ Wave 0 |
| DOC-01 | Doctor's browser check degrades to info/fail (NOT exit 1) when chromium absent | unit | `cargo test --package bootroom doctor_cmd::tests::browser_missing_does_not_set_overall_fail` | ❌ Wave 0 |
| DOC-01 | Doctor's COOP/COEP self-check passes against `build_router(state)` | integration | `cargo test --package bootroom --test doctor_headers_check headers_self_check_passes` | ❌ Wave 0 |
| DOC-01 | Doctor's config check returns info when no bootroom.toml exists | unit | `cargo test --package bootroom doctor_cmd::tests::config_missing_is_info_not_fail` | ❌ Wave 0 |
| DOC-01 | Doctor's config check returns pass when valid config exists | integration | `cargo test --package bootroom --test doctor_subcommand doctor_exits_zero_with_valid_config` | ❌ Wave 0 |
| DOC-01 | Doctor's config check returns fail with span on parse error | integration | `cargo test --package bootroom --test doctor_subcommand doctor_exits_one_on_parse_error` | ❌ Wave 0 |
| DOC-01 | JSON output has schema_version=1, required keys, valid overall | integration | `cargo test --package bootroom --test doctor_json_schema doctor_json_pins_schema_keys` | ❌ Wave 0 |
| DOC-01 | Human output has section headers + glyph prefixes | integration | `cargo test --package bootroom --test doctor_human_format doctor_human_section_headers_present` | ❌ Wave 0 |
| DOC-01 | Failure mode writes one-line summary to stderr | integration | `cargo test --package bootroom --test doctor_subcommand doctor_failure_writes_stderr_summary` | ❌ Wave 0 |
| DOC-01 | Exit 0 on all-pass; exit 1 on any fail | integration | `cargo test --package bootroom --test doctor_exit_codes doctor_zero_when_all_pass / doctor_one_when_any_fail` | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test --package bootroom doctor` (unit + per-module integration tests)
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full suite green before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] `crates/bootroom/tests/doctor_subcommand.rs` — CLI shape + exit codes + stderr summary
- [ ] `crates/bootroom/tests/doctor_human_format.rs` — section headers, glyph prefixes, overall line
- [ ] `crates/bootroom/tests/doctor_json_schema.rs` — schema_version, required keys, value enums
- [ ] `crates/bootroom/tests/doctor_headers_check.rs` — in-process router self-check
- [ ] `crates/bootroom/tests/doctor_exit_codes.rs` — 0/1 dispatch
- [ ] Extend `crates/bootroom/tests/cli_subcommands.rs` with `top_level_help_lists_doctor` + `doctor_subcommand_help_mentions_flags`
- [ ] Framework install: none — `tokio::test` + `cargo test` already wired

## Security Domain

> security_enforcement is not explicitly disabled. Phase 5 has minimal security surface (no network exposure, no user input parsing beyond clap's value enum), but document where it touches existing controls.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | n/a — bootroom is loopback-only; doctor binds nothing |
| V3 Session Management | no | n/a |
| V4 Access Control | no | n/a — doctor reads only files the user already owns |
| V5 Input Validation | yes | clap `ValueEnum` rejects unknown `--format` values; `--config` is parsed as `PathBuf` and passed to `LoadedConfig` which has its own validation (Phase 3+4 hardening) |
| V6 Cryptography | no | n/a — no crypto in this phase |
| V12 File Handling | yes | Doctor reads `bootroom.toml` from the operator's CWD when `--config` is absent; same posture as `bootroom check` (already accepted) |
| V14 Configuration | yes | git SHA fallback to `"unknown"` for unreproducible builds is the documented degraded state, not a vulnerability |

### Known Threat Patterns for {stack}

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Path traversal via `--config <attacker-supplied>` | Tampering / Info Disclosure | Doctor only READS `--config`; no write. Same control as `bootroom check`. Acceptable. |
| Maliciously crafted `bootroom.toml` exploiting TOML parser | Tampering | `toml` crate is the workspace dep; same surface as serve / check. No additional exposure. |
| Stderr summary leaking sensitive file paths | Info Disclosure | Operator-supplied path appears in error message. Acceptable for a local dev tool; same posture as `bootroom check`. |
| Env-var injection via `BOOTROOM_GIT_SHA` | Tampering | build.rs reads `git` output; runtime reads compile-time constant. No runtime env-var read for SHA. Not exploitable post-build. |

## Sources

### Primary (HIGH confidence)
- `crates/bootroom/src/cli.rs` — current CLI structure, doc-string patterns
- `crates/bootroom/src/run_cmd.rs` lines 396-432 — `discover_chromium`, `which_via_path_env` (reuse target)
- `crates/bootroom/src/server.rs` lines 209-303 — `build_router` + oneshot self-check pattern
- `crates/bootroom/src/state.rs` lines 156-174 — `AppState::new_for_test` (no kernel file required)
- `crates/bootroom/src/check_cmd.rs` — ExitCode-returning subcommand pattern (template for doctor_cmd.rs)
- `crates/bootroom/src/init_cmd.rs` — same pattern reference
- `crates/bootroom/src/headers.rs` — the COOP/COEP layer being verified
- `crates/bootroom/src/embed.rs` — `embed::QEMU` `include_dir!` handle
- `crates/bootroom/src/verbose.rs` — ASCII glyph convention precedent (Phase 4)
- `crates/bootroom/build.rs` — existing build script (extend with git SHA)
- `Makefile` — existing `qemu-assets` recipe (extend with rev.txt)
- `crates/bootroom-core/src/config.rs` — `LoadedConfig::load_from_str` + `LoadError` (reuse target)
- `crates/bootroom/tests/coop_coep_headers.rs` — `reqwest`-based COOP/COEP integration pattern
- `crates/bootroom/tests/common/mod.rs` — `spawn` helper + `TestServer` cleanup pattern
- `.planning/phases/05-diagnostics-doctor/05-CONTEXT.md` — user decisions (locked)
- `.planning/ROADMAP.md` Phase 5 section — phase goal + success criteria
- `.planning/REQUIREMENTS.md` lines 91-97, 202, 205 — CLI-01 and DOC-01 definitions

### Secondary (MEDIUM confidence)
- `include_dir` crate docs (via existing usage in `embed.rs`) — `Dir::get_file` returns `Option<&File>` [VERIFIED in tree]
- Phase 4 04-RESEARCH (referenced by `verbose.rs` comment) — Open Question 4 rationale for ASCII glyphs
- clap derive docs — `///` first line → `about`; subsequent paragraphs → `long_about`

### Tertiary (LOW confidence)
- None — Phase 5 is composition over already-validated primitives. No untested external claims.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — every dep is already in the workspace; only minor `tower::util` dep-section promotion needed
- Architecture: HIGH — patterns mirror Phase 1-4 conventions exactly; in-process `oneshot` is documented in existing tests
- Pitfalls: HIGH — derived from concrete inspection of build.rs, embed.rs, run_cmd.rs visibility, and crates.io packaging behavior

**Research date:** 2026-05-19
**Valid until:** 2026-06-19 (~30 days; primitives are stable, only crates.io packaging behavior could conceivably change)
