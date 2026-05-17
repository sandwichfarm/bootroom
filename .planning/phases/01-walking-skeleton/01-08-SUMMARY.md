---
phase: 01
plan: 08
slug: spike-b-headless-chromium-sab
status: complete
requirements:
  - UI-01
tags:
  - spike
  - headless
  - chromium
  - chromiumoxide
  - de-risk
date: 2026-05-17
---

# Plan 01-08 — Spike B: Headless Chromium + SAB + qemu-wasm

## Outcome

**Verdict: green. Chosen path for Phase 4: chromiumoxide.**

The biggest unknown surfaced by 01-RESEARCH.md (Pitfall #2: "headless SAB
reliability across CI runner images") is retired. `chromiumoxide 0.9.1`
driving `/usr/bin/chromium` in `--headless=new` mode, against an in-process
bootroom server, observed `crossOriginIsolated === true`,
`typeof SharedArrayBuffer !== 'undefined'`, reached the `RUNNING` pill state,
and captured 49 chars of steady-state serial output from the live NORN
kernel within the 15 s polling window. Phase 4 can plan against
`chromiumoxide` without a Playwright subprocess fallback.

## Tasks

| # | Task | Commit | Status |
|---|------|--------|--------|
| 1 | Scaffold spike-b crate (workspace member, isolated dep) | `474a2c6` | done |
| 2 | Driver — chromiumoxide + headless Chromium end-to-end | `c810c7f` | done |
| 3 | Validate SPIKE-B-RESULT.md format (5 H2 sections, 4 frontmatter keys, constrained verdict + chosen_path) | (verify-only) | done |

## Verification

- `cargo build -p spike-b` succeeds.
- `cargo build --workspace` still succeeds.
- `cargo tree -p bootroom | grep chromiumoxide` returns 0 lines — main
  bootroom binary's dep tree is unchanged (Threat T-01-08-02 mitigated).
- `cargo run -p spike-b -- --kernel crates/bootroom/spikes/spike-b/fixtures/Image`
  produces `SPIKE-B-RESULT.md` with the canonical format.
- Result file: `verdict: green`, `chosen_path: chromiumoxide`,
  5 H2 sections (`Question`, `Method`, `Observations`, `Decision`,
  `Follow-ups`), all 4 frontmatter keys present.

## Fixture used

The actual NORN kernel binary at
`/home/sandwich/Develop/nostros/target/riscv64gc-unknown-none-elf/release/norn-kernel`
(1.28 MB RISC-V ELF), copied into `crates/bootroom/spikes/spike-b/fixtures/Image`
(the `fixtures/` directory is gitignored except for `.gitkeep`, so the
fixture itself is not committed). This is the same kernel verified via
headed browser in plan 01-07's smoke test, which gave a credible green
signal — qemu-wasm running NORN under Chromium prints its expected
banner lines (`[NORN ISA] base=rv64 …`, `[NORN PMP] region count = 16`,
etc.).

## Observed values

| Observable | Value |
|---|---|
| `self.crossOriginIsolated` | `true` |
| `typeof SharedArrayBuffer !== 'undefined'` | `true` |
| Final `#status data-state` | `RUNNING` |
| Terminal char count (steady-state) | 49 |
| Chromium user-agent | `HeadlessChrome/148.0.0.0` |
| Chromium binary version | `Chromium 148.0.7778.167` |

The 49-char terminal count is steady-state (the driver settles 1.5 s after
first detecting `RUNNING + bytes > 0` before sampling, which is enough for
the NORN kernel's early banner to land in xterm's row buffer). The first
sample saw only 1 character because polling caught the transition the moment
PROXY\_TO\_PTHREAD started flushing serial output.

## chromiumoxide 0.9.x surprises (A.4 in 01-RESEARCH.md)

The plan-text quoted the older 0.7-era API; the executor adjusted to 0.9.1.
Three breaking-change surprises worth recording for Phase 4 planning:

1. **`tokio-runtime` feature is gone.** 0.9.1 advertises only
   `default = [bytes]` plus `chromiumoxide_fetcher`, `native-tls`, `rustls`,
   `zip0`, `zip8`. The runtime split (`tokio-runtime` vs
   `async-std-runtime`) was removed; chromiumoxide is now tokio-only.
   Use `default-features = false` if you don't want `bytes` re-exported.
2. **`HeadlessMode` enum is not in any public path.** It's `pub enum`
   inside `browser::config`, but the `config` module is `mod` (private)
   and the enum isn't re-exported. Use the builder methods
   (`.new_headless_mode()`, `.with_head()`, or `.headless_mode(...)`) —
   the last one takes `HeadlessMode` so it's unreachable from outside the
   crate in 0.9.1; the first two are the only public way to set it.
3. **`BrowserConfig` fields are all `pub(crate)`.** Cannot use
   `BrowserConfig { headless: ..., ..config }` to override after `.build()`.
   Set every flag via the builder before calling `.build()`.

## Files changed

- `Cargo.toml` (workspace `members` += spike-b)
- `Cargo.lock` (chromiumoxide + transitive deps added)
- `.gitignore` (`fixtures/*` ignored except `.gitkeep`)
- `crates/bootroom/spikes/spike-b/Cargo.toml` (new)
- `crates/bootroom/spikes/spike-b/README.md` (new)
- `crates/bootroom/spikes/spike-b/src/main.rs` (new, 313 lines)
- `crates/bootroom/spikes/spike-b/fixtures/.gitkeep` (new)
- `crates/bootroom/spikes/spike-b/SPIKE-B-RESULT.md` (new, authoritative)

## Time spent

Target: 1 day. Actual: ~1 hour. The de-risking spike retired quickly
because (a) plan 01-07's earlier Playwright smoke had already confirmed
the underlying SAB + COOP/COEP + qemu-wasm path works headless on this
system, so the only remaining unknown was the chromiumoxide 0.9.1 API
surface, and (b) the three 0.9.1 API surprises (above) were all
mechanical fixes once the actual chromiumoxide source was inspected.

## Phase 4 hand-off

The `chosen_path: chromiumoxide` line in SPIKE-B-RESULT.md is the
machine-readable signal for Phase 4's `bootroom run` planning. No
additional spike work required. Phase 4 may import the same launch
incantation directly from `crates/bootroom/spikes/spike-b/src/main.rs`
(`BrowserConfig::builder().chrome_executable(...).new_headless_mode().no_sandbox()...`)
and the same evaluation idioms for terminal extraction.

## Self-Check: PASSED

- `crates/bootroom/spikes/spike-b/Cargo.toml` exists
- `crates/bootroom/spikes/spike-b/src/main.rs` exists
- `crates/bootroom/spikes/spike-b/README.md` exists
- `crates/bootroom/spikes/spike-b/SPIKE-B-RESULT.md` exists
- `crates/bootroom/spikes/spike-b/fixtures/.gitkeep` exists
- Commit `474a2c6` (scaffolding) present in `git log --oneline`
- Commit `c810c7f` (driver + result) present in `git log --oneline`
- `cargo tree -p bootroom | grep chromiumoxide` returns 0 lines (dep isolation verified)
- `grep -c '^## ' SPIKE-B-RESULT.md` = 5 (all required sections)
- `verdict: green` and `chosen_path: chromiumoxide` match the documented vocabulary
