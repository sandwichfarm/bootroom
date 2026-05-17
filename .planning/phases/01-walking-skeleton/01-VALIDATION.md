---
phase: 1
slug: walking-skeleton
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-05-17
---

# Phase 1 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `#[cfg(test)]` + `cargo test` (no external runner) |
| **Config file** | None — `Cargo.toml`'s `[dev-dependencies]` is the only config |
| **Quick run command** | `cargo test -p bootroom --lib` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~5s warm-cache lib tests; ~30s full suite (integration spawns listeners) |

Headless browser smoke (Spike B + Phase-1 acceptance):
`chromium --headless=new --disable-gpu http://127.0.0.1:8765` + DevTools-protocol probe, or the Spike B chromiumoxide binary once green.

---

## Sampling Rate

- **After every task commit:** Run `cargo build --workspace && cargo test -p bootroom --lib`
- **After every plan wave:** Run `cargo test --workspace` + manual headed-browser smoke against fixture kernel
- **Before `/gsd-verify-work`:** Full suite green + Spike A and Spike B verdicts recorded + manual browser smoke against NORN kernel (or fixture stand-in)
- **Max feedback latency:** ~5 seconds (per-commit)

---

## Per-Task Verification Map

| Req ID | Behavior | Test Type | Automated Command | File Exists | Status |
|--------|----------|-----------|-------------------|-------------|--------|
| DIST-01 | `cargo build` from clean checkout produces single binary | smoke | `cargo build --workspace --release` | ❌ Wave 0 | ⬜ pending |
| SERV-01 | `bootroom serve --kernel <path>` binds 127.0.0.1:<port> | integration | `cargo test -p bootroom --test serve_binds` | ❌ Wave 0 | ⬜ pending |
| SERV-02 | COOP and COEP headers on every response | integration | `cargo test -p bootroom --test coop_coep_headers` | ❌ Wave 0 | ⬜ pending |
| SERV-03 | Embedded qemu-wasm artifacts + UI served via include_dir | integration | `cargo test -p bootroom --test embedded_assets_served` | ❌ Wave 0 | ⬜ pending |
| SERV-04 | `--assets-dir <path>` overrides embedded assets | integration | `cargo test -p bootroom --test assets_dir_override` | ❌ Wave 0 | ⬜ pending |
| SERV-05 | `--port <N>` and `--host <addr>` flags work | integration | `cargo test -p bootroom --test port_host_flags` | ❌ Wave 0 | ⬜ pending |
| UI-01 | Page boots qemu-system-riscv64.wasm with supplied kernel | manual + headless smoke | Manual until Spike B green; then chromiumoxide assertion | ❌ Wave 0 | ⬜ pending |
| UI-05 | crossOriginIsolated probe banner appears when SAB unavailable | manual | Visual: open over a non-COOP server; observe red banner | manual-only | ⬜ pending |
| UI-07 | Header shows kernel path, size, mtime | integration (API) + manual (DOM) | `cargo test -p bootroom --test kernel_info_endpoint` + visual | ❌ Wave 0 | ⬜ pending |
| CLI-03 | Common task = one command (no >1-line invocations) | meta-test | `bootroom serve --kernel /tmp/fixture` succeeds in one command | manual | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/bootroom/tests/coop_coep_headers.rs` — covers SERV-02. Use `axum_test::TestServer` or `reqwest` against a spawned listener.
- [ ] `crates/bootroom/tests/serve_binds.rs` — covers SERV-01.
- [ ] `crates/bootroom/tests/embedded_assets_served.rs` — covers SERV-03; asserts `GET /assets/qemu/qemu-system-riscv64.wasm` returns 200 + correct MIME `application/wasm`.
- [ ] `crates/bootroom/tests/assets_dir_override.rs` — covers SERV-04; tempdir + override.
- [ ] `crates/bootroom/tests/port_host_flags.rs` — covers SERV-05; exercises `--port 0` ephemeral binding.
- [ ] `crates/bootroom/tests/kernel_info_endpoint.rs` — covers UI-07's API surface.
- [ ] `crates/bootroom/spikes/spike-b/SPIKE-B-RESULT.md` (test code scaffolded by Spike B)
- [ ] `crates/bootroom/spikes/spike-a/SPIKE-A-RESULT.md` (test code scaffolded by Spike A)

No external test framework install required (`#[cfg(test)]` only).

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| crossOriginIsolated probe banner renders | UI-05 | DOM/CSS state observable only in a real browser | Run `bootroom serve --kernel <fixture>`, open URL, then in DevTools temporarily disable COOP via a proxy or observe on a non-isolated host; red banner with fix hint must appear |
| One-command invocation usable | CLI-03 | UX validation — observe friction not behavior | Run exactly `bootroom serve --kernel /tmp/fixture` — verify it completes in one line, prints the URL, no extra setup |
| Kernel-info header visible in DOM | UI-07 (visual half) | API tested; visual must be eyeballed | Open URL, confirm path, size, mtime, sha256 prefix appear in header strip |
| qemu-system-riscv64.wasm boots in real Chrome/Firefox | UI-01 | Spike-B-dependent: until Spike B green, headless can't substitute | Open URL in headed Chrome 144+; observe serial output streaming into xterm.js terminal within 10s |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 10s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
