---
phase: 05-diagnostics-doctor
reviewed: 2026-05-19T00:00:00Z
depth: standard
files_reviewed: 12
files_reviewed_list:
  - crates/bootroom/src/doctor_cmd.rs
  - crates/bootroom/src/cli.rs
  - crates/bootroom/src/main.rs
  - crates/bootroom/src/lib.rs
  - crates/bootroom/src/run_cmd.rs
  - crates/bootroom/src/verbose.rs
  - crates/bootroom/build.rs
  - crates/bootroom/Cargo.toml
  - crates/bootroom/assets/qemu/qemu-wasm-rev.txt
  - Makefile
  - crates/bootroom/tests/doctor_subcommand.rs
  - crates/bootroom/tests/doctor_human_format.rs
  - crates/bootroom/tests/doctor_json_schema.rs
  - crates/bootroom/tests/doctor_headers_check.rs
  - crates/bootroom/tests/doctor_exit_codes.rs
  - crates/bootroom/tests/cli_subcommands.rs
findings:
  critical: 1
  blocker: 1
  warning: 5
  info: 4
  total: 11
status: issues_found
---

# Phase 5: Code Review Report

**Reviewed:** 2026-05-19
**Depth:** standard
**Files Reviewed:** 12 source + tests
**Status:** issues_found

## Summary

Phase 5 lands the `bootroom doctor` subcommand cleanly: six checks in a fixed
order, stable JSON v1 schema with strong pin tests, ASCII glyph discipline,
correct exit-code policy (browser=Info, missing config=Info, broken
config=Fail), and an in-process `tower::ServiceExt::oneshot` self-check that
exercises the real `build_router`. The CLI shape is tightly pinned by the
five-subcommand regression test.

The review surfaced **one BLOCKER**: the existing Phase-1 escape hatch
`BOOTROOM_SKIP_QEMU_ASSET_CHECK=1` now causes a compile-time error in
`doctor_cmd.rs` because `build.rs` exits before emitting
`cargo:rustc-env=BOOTROOM_GIT_SHA`. A previously documented dev workflow
silently broke.

A second high-impact issue: `qemu-wasm-rev.txt` is not on `build.rs`'s
per-file `rerun-if-changed` list, so editing the rev in place (the exact
case the Phase-1 WR-04 fix targeted) will not re-trigger the embed and the
doctor reports a stale rev.

Other findings are quality issues (blocking sync `Command` from an async
context, hard-coded shared temp paths in unit tests, two assertion paths
where a swapped header value would still be reported as a pass).

## Blocker Issues

### BL-01: `BOOTROOM_SKIP_QEMU_ASSET_CHECK=1` now fails to compile

**File:** `crates/bootroom/build.rs:47-52` (interaction with `crates/bootroom/src/doctor_cmd.rs:126,447`)
**Issue:**

The Phase-1 escape hatch lets a developer build the crate without the
qemu-wasm artifacts present. `build.rs` early-returns at line 51 when
`BOOTROOM_SKIP_QEMU_ASSET_CHECK=1`:

```rust
if std::env::var("BOOTROOM_SKIP_QEMU_ASSET_CHECK").is_ok() {
    println!("cargo:warning=…");
    return;                       // <-- skips the SHA capture below
}
// …
println!("cargo:rustc-env=BOOTROOM_GIT_SHA={sha}");
```

On that path, `BOOTROOM_GIT_SHA` is never published. Then
`doctor_cmd.rs::check_version` (line 126) and `format_json` (line 447) both
call `env!("BOOTROOM_GIT_SHA")`, which is a **compile-time** error when the
env var is unset, not a runtime miss. The crate no longer compiles under
the documented dev escape hatch.

Phase-1 build.rs comment explicitly invites this workflow:
"Intended for short-lived dev scenarios (e.g. iterating on unrelated crate
code before the qemu artifacts have been built)."

**Fix:** Move the SHA capture above the early-return, or emit a
sentinel SHA on the bypass path. Capture-above-early-return is the
minimal change:

```rust
fn main() {
    // … rerun-if-changed declarations …

    // Capture git SHA FIRST so env!("BOOTROOM_GIT_SHA") always resolves,
    // even on the BOOTROOM_SKIP_QEMU_ASSET_CHECK bypass path.
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
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs");

    if std::env::var("BOOTROOM_SKIP_QEMU_ASSET_CHECK").is_ok() {
        println!("cargo:warning=…");
        return;
    }
    // … rest unchanged …
}
```

Add a regression test pin under `tests/doctor_subcommand.rs` that documents
the contract: `BOOTROOM_GIT_SHA` is always set to a non-empty value
regardless of build-environment state.

## Critical Issues

### CR-01: `qemu-wasm-rev.txt` is not on the per-file `rerun-if-changed` list

**File:** `crates/bootroom/build.rs:15-22, 31-37`
**Issue:**

`build.rs`'s `REQUIRED` array enumerates the qemu-wasm artifacts that get a
per-file `rerun-if-changed`. The comment at lines 28-30 explicitly
documents why per-file is needed: editing a file in place (the
`make qemu-assets`-driven workflow) does not fire `rerun-if-changed` on
the directory alone. WR-04 history.

The newly committed `assets/qemu/qemu-wasm-rev.txt` is the **most
in-place-edited** file in that directory (it is rewritten on every
`make qemu-assets` run — line 68 of the Makefile, `git -C $(QEMU_WASM_DIR)
rev-parse --short HEAD > $(QEMU_OUT_DIR)/qemu-wasm-rev.txt`), yet it is not
in `REQUIRED`. The directory-level watch at line 37
(`rerun-if-changed=assets/qemu`) is exactly the case the per-file pattern
was added to avoid.

Effect: an operator runs `make qemu-assets`, the rev file updates on disk,
but `cargo build` does not rebuild `embed.rs` / `lib.rs`, so
`include_dir!` keeps the previous embed. `bootroom doctor` then reports a
stale `qemu_wasm_rev` value. This breaks the DOC-01 "verify what you've
got" contract silently.

**Fix:** Add the rev file to `REQUIRED` so the per-file watch fires:

```rust
const REQUIRED: &[&str] = &[
    "assets/qemu/qemu-system-riscv64.wasm",
    "assets/qemu/qemu-system-riscv64.worker.js",
    "assets/qemu/qemu-system-riscv64.data",
    "assets/qemu/out.js",
    "assets/qemu/load.js",
    "assets/qemu/module.js",
    "assets/qemu/qemu-wasm-rev.txt", // <-- add
];
```

The rev file already exists as a committed sentinel (`unknown\n`), so the
presence check at lines 54-69 will not erroneously fail clean checkouts.

## Warnings

### WR-01: `check_browser` runs `Command::output()` synchronously in an async function

**File:** `crates/bootroom/src/doctor_cmd.rs:166-168`
**Issue:**

`check_browser` is called from `run(...)` which is `async`. Inside the
function:

```rust
let probe = std::process::Command::new(&path)
    .arg("--version")
    .output();
```

`std::process::Command::output()` is **synchronous and blocking** — it
waits on the child process while holding the current tokio worker
thread. Chromium's `--version` typically returns in 10-50 ms, but a
hung/unresponsive binary (or `--version` blocking on display-server
initialization on a misconfigured runner) would freeze the executor
for the full timeout / indefinitely.

Doctor advertises a ~100 ms target; one slow `--version` probe blocks
the only async progress (the in-process router self-check that follows
also runs on the same executor).

**Fix:** Either use `tokio::process::Command` with an `.await` on
`.output()`, or wrap the sync call in `tokio::task::spawn_blocking`.
The former is the simplest change:

```rust
let probe = tokio::process::Command::new(&path)
    .arg("--version")
    .output()
    .await;
```

Note: `discover_chromium` itself (in `run_cmd.rs`) also uses sync
`Command` and is called from `run(...)` here. Per the run-mode precedent
it is left sync; if you touch that path, audit both callers.

### WR-02: `check_config` swallows reachable I/O errors as Fail without distinguishing the cause

**File:** `crates/bootroom/src/doctor_cmd.rs:296-303`
**Issue:**

The non-NotFound arm catches **every other** `io::Error` and stamps the
result `Fail`:

```rust
Err(e) => {
    return Check {
        name: "config".to_string(),
        status: CheckStatus::Fail,
        detail: format!("{}: {e}", path_buf.display()),
    };
}
```

Cases collapsed into a generic Fail:

- `PermissionDenied` — operator passes a path inside a restricted dir.
- `IsADirectory` — operator passes `--config .` by accident.
- `InvalidInput` — path contains an interior NUL (Windows / odd FS).

Some of these (e.g. PermissionDenied on a path the operator chose) are
genuinely Fail-worthy, but a directory-not-file case is closer to a
usage error and would benefit from a sharper message. The exit-code
contract (1 on Fail) currently makes all of them indistinguishable
from "valid path, broken TOML".

This is a minor diagnostic-quality regression vs. `bootroom check` which
hits the same loader directly and gives parser-tier errors only when
the file *is* readable. The doctor variant is the user's friendly
front-end, so the messaging should be sharper.

**Fix:** Branch on `e.kind()` for the common readable-but-broken cases:

```rust
Err(e) => {
    let kind_hint = match e.kind() {
        std::io::ErrorKind::PermissionDenied => " (permission denied)",
        std::io::ErrorKind::IsADirectory      => " (is a directory, not a file)",
        _                                     => "",
    };
    return Check {
        name: "config".to_string(),
        status: CheckStatus::Fail,
        detail: format!("{}: {e}{kind_hint}", path_buf.display()),
    };
}
```

### WR-03: Unit tests share hard-coded `/tmp/bootroom-doctor-*` paths across the suite

**File:** `crates/bootroom/src/doctor_cmd.rs:677-679, 712-714, 729-731, 218-220`
**Issue:**

Four call sites in the test/runtime code use `std::env::temp_dir().join(...)`
with a fixed name:

- L218: `"bootroom-doctor-noop"` (used by `check_headers` placeholder kernel)
- L678: `"bootroom-doctor-test-no-such-file-xyz-9f8e7d6c.toml"`
- L713: `"bootroom-doctor-broken-toml"` (dir; writes `bad.toml`)
- L730: `"bootroom-doctor-valid-toml"`  (dir; writes `good.toml`)

The integration tests under `tests/doctor_*.rs` correctly use
`tempfile::tempdir()` and so are immune. The in-crate unit tests are
not.

Failure modes:

1. **Parallel test runs (`cargo test --jobs N`)** of these unit tests
   in the same compile target are fine today (different filenames) but
   degrade as soon as someone adds a second test that uses the same
   directory.
2. **Cross-user / multi-user CI runner** — `/tmp/bootroom-doctor-broken-toml`
   created by user A's run is then `chmod`-locked when user B's run
   tries `fs::create_dir_all`. `create_dir_all` does not fail on
   existing directories, but `write` to `bad.toml` will fail if
   permissions are inherited from a prior run.
3. **Stale state from a previously-failed run** can mask regressions
   (e.g. a test asserting the rev token is non-empty would pass against
   a stale leftover file even if the embed regressed to empty).

The codebase already depends on `tempfile` (Cargo.toml `[dev-dependencies]`,
line 54). Use it consistently.

**Fix:** Replace each of the four sites with `tempfile::tempdir()` /
`tempfile::NamedTempFile`:

```rust
#[test]
fn check_config_broken_toml_is_fail() {
    let dir = tempfile::tempdir().expect("mkdir tmp");
    let p = dir.path().join("bad.toml");
    std::fs::write(&p, "this is not valid toml [[[\n").expect("write bad toml");
    let c = check_config(Some(&p));
    // … existing assertions …
    // tempdir auto-cleans on drop; no manual fs::remove_file needed.
}
```

For the `check_headers` placeholder kernel (line 219), `tempfile` adds
overhead each call; an alternative is a dedicated per-process path like
`std::env::temp_dir().join(format!("bootroom-doctor-noop-{}", std::process::id()))`
so parallel cargo test runners get distinct paths.

### WR-04: `check_headers` does not distinguish a missing header from a swapped header in coverage

**File:** `crates/bootroom/src/doctor_cmd.rs:240-265`
**Issue:**

The check correctly asserts both the COOP and COEP values together. But
the single `if` collapses four distinct failure modes into one Fail
result:

1. COOP header absent, COEP correct.
2. COEP header absent, COOP correct.
3. COOP present but value wrong (e.g. `unsafe-none`).
4. Both missing.

The detail string surfaces the actual values, which helps an operator
debug — good. But the **load-bearing regression test**
(`tests/doctor_headers_check.rs::check_headers_passes_against_build_router`)
only asserts on the green path (Pass with `COOP=same-origin` and
`COEP=require-corp` in the detail). There is **no negative test** that
constructs a router missing one of the layers and verifies doctor
reports Fail with the right detail. If a future refactor swaps the
COOP layer for a NOP, the test suite catches it via the green-path
assert breaking — but the precise failure-detail wording is unpinned
and could regress to "expected COOP=… got COOP=Some(\"\")" which would
hurt operator triage.

**Fix:** Add a small unit test in the `mod tests` block that constructs
a `Router` *without* the COOP/COEP layers, calls a private helper that
takes the router as input, and asserts the Fail detail wording. This
requires factoring the inner `oneshot + header read + match` block out
of `check_headers` so it can be tested without spinning up
`AppState::new_for_test`. Example signature:

```rust
async fn check_headers_against_router(app: axum::Router) -> Check { … }

pub async fn check_headers() -> Check {
    let kernel = …;
    let state = …;
    check_headers_against_router(crate::build_router(state)).await
}
```

Then a unit test can pass a bare `Router::new().route("/", get(|| async { "ok" }))`
and pin the Fail detail.

### WR-05: Inverse pin for "no surprise check" missing in the doctor checks-name test

**File:** `crates/bootroom/tests/doctor_json_schema.rs:66-86`
**Issue:**

`json_checks_names_are_the_six_known` asserts EXACT membership against
the six known names. Good. But the helper `find_check` in
`doctor_cmd.rs::363-365` returns `Option<&Check>` and the formatter
silently omits any check whose name is unknown to the section
templates. So a future refactor that ADDS a seventh check named
"foo" would:

1. Trip the JSON test (exact-set mismatch). Good.
2. Make the human formatter omit "foo" from the rendered report.
   The check still contributes to the `overall_failed` calculation
   in `run(...)` (line 89), so if "foo" is Fail the exit code is 1
   but the human report shows no failure reason. Operator confusion.

The five `tests/doctor_human_format.rs` tests pin **presence** of the
five section headers but do not pin "every check in the input list
appears somewhere in the rendered output". The formatter's "drop on
unknown name" behavior is silent.

**Fix:** Either (a) make `format_human` fall back to rendering unknown
checks in a final "## Other" section so they cannot vanish, or
(b) add a debug assertion at the top of `format_human` that every
name in `checks` is one of the six template-known names. Option (a):

```rust
out.push_str("## CLI surface\n");
// … existing …
out.push('\n');

// Catch-all: any check whose name does not match the five section
// templates above renders here instead of silently disappearing.
let known: &[&str] = &["version", "qemu_wasm_rev", "browser", "headers", "config", "cli_surface"];
let unknown: Vec<&Check> = checks.iter().filter(|c| !known.contains(&c.name.as_str())).collect();
if !unknown.is_empty() {
    out.push_str("## Other\n");
    for c in unknown {
        out.push_str(&render_check_line(c));
        out.push('\n');
    }
    out.push('\n');
}

out.push_str(if overall_failed { "Overall: fail" } else { "Overall: pass" });
```

## Info

### IN-01: Doc-string contains an em-dash, but glyph policy is "no unicode anywhere"

**File:** `crates/bootroom/src/doctor_cmd.rs:8,11,12,13,14,15,116,131,139,168,213,213,261` (multiple)
**Issue:**

Module doc comments and inline comments contain en-dash (U+2013) and
em-dash (U+2014) characters — e.g. line 8 "Failure semantics:" block,
line 116 "all-Pass results — including a missing browser",
line 213 "regression test for Phase-1's COOP/COEP middleware lives outside this
crate". These are rendered into rustdoc HTML and do not affect runtime
output, so the human-format ASCII test passes.

However, Phase-4's stated rule (verbose.rs line 3) is "All output is
ASCII-only", and the orchestrator's open-question resolution
(05-PLAN.md "Glyph Convention Deviation") extends ASCII discipline to
Phase 5. Per the strictest reading this should be applied to source
comments too for consistency / grep stability across the codebase.

This is purely a hygiene call — no functional impact. Leaving as Info.

**Fix (optional):** Run `rg '[–—]' crates/bootroom/src/doctor_cmd.rs`
and replace each `–`/`—` with `-`.

### IN-02: `truncate_for_error` (cli.rs:211-220) is unrelated to Phase 5 but unchanged

**File:** `crates/bootroom/src/cli.rs:211-220`
**Issue:** Note for context only — `truncate_for_error` is a Phase-4
helper. Phase 5 does not modify it. No action.

### IN-03: `format_json::Report.git_sha` is `&'a str` referencing `env!()`

**File:** `crates/bootroom/src/doctor_cmd.rs:64-70, 444-451`
**Issue:**

`Report.git_sha: &'a str` is filled in from `env!("BOOTROOM_GIT_SHA")`,
which is `&'static str`. The `'a` lifetime is fine because `'static`
satisfies `'a`. No bug, just a code-smell: the `version` and `git_sha`
fields could be `&'static str` (with no `'a` parameter) since they
always come from compile-time constants. Tightening the type would
make the contract clearer.

**Fix (optional):**

```rust
struct Report<'a> {
    schema_version: u32,
    version: &'static str,
    git_sha: &'static str,
    checks: &'a [Check],
    overall: &'static str,
}
```

### IN-04: `Cargo.toml` lists `tower = { features = ["util"] }` in both `[dependencies]` and `[dev-dependencies]`

**File:** `crates/bootroom/Cargo.toml:37, 51`
**Issue:**

Line 37 (new in Phase 5 per Pitfall 6) promotes tower::util from
dev-dep to dep so `doctor_cmd::check_headers` can call
`ServiceExt::oneshot` from library code:

```toml
[dependencies]
tower = { workspace = true, features = ["util"] }
…
[dev-dependencies]
tower = { workspace = true, features = ["util"] }
```

Cargo merges feature sets across `[dependencies]` and
`[dev-dependencies]`, so the dev-dep line is now redundant. Removing
it removes a maintenance footgun (someone bumping features in
one stanza but not the other).

**Fix (optional):** Drop the `tower = …` line from `[dev-dependencies]`
(line 51). Tests still compile against the prod dep.

---

_Reviewed: 2026-05-19_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
