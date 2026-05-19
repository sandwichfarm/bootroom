---
phase: 3
slug: config-buttons-watcher
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-05-19
---

# Phase 3 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test` (Rust unit + integration) + `node --check` for JS syntax |
| **Config file** | `Cargo.toml` workspace + per-crate `[dev-dependencies]` |
| **Quick run command** | `cargo test --workspace --lib` |
| **Full suite command** | `cargo test --workspace && node --check crates/bootroom/web/{app,funnel}.js && cargo clippy --workspace --all-targets -- -D warnings` |
| **Estimated runtime** | ~5s warm-cache lib tests; ~45s full suite (more integration tests this phase) |

Phase 2 `TestServer` harness reused unchanged.

---

## Sampling Rate

- **Per task commit:** `cargo test --workspace --lib`
- **Per wave merge:** full suite (cargo test + node --check + clippy)
- **Phase gate:** Full suite green + headed-browser smoke (ACT-02 click + ACT-04 lock + WCH banner)

---

## Per-Task Verification Map

| Req ID | Behavior | Test Type | Automated Command | File Exists | Status |
|--------|----------|-----------|-------------------|-------------|--------|
| CFG-01 | `--config` override; CWD default | integration | `cargo test --test config_loading` | ❌ W0 | ⬜ |
| CFG-02 | TOML actions with label, bytes, group | unit | `cargo test -p bootroom-core config::tests::actions_roundtrip` | ❌ W0 | ⬜ |
| CFG-03 | Scenarios w/ action refs + assertions + timeout | unit | `cargo test -p bootroom-core config::tests::scenarios_parse` | ❌ W0 | ⬜ |
| CFG-04 | `schema_version = 1` required | unit | `cargo test -p bootroom-core config::tests::schema_version_rejected` | ❌ W0 | ⬜ |
| CFG-05 | `deny_unknown_fields` w/ line:col | unit | `cargo test -p bootroom-core config::tests::deny_unknown_fields_with_span` | ❌ W0 | ⬜ |
| CFG-06 | Scenario→action ref validation | unit | `cargo test -p bootroom-core config::tests::scenario_unknown_action_ref` | ❌ W0 | ⬜ |
| CFG-07 | `bootroom check` exit codes | integration | `cargo test --test check_subcommand` | ❌ W0 | ⬜ |
| CFG-08 | `bootroom init` writes + refuses overwrite | integration | `cargo test --test init_subcommand` | ❌ W0 | ⬜ |
| CFG-09 | Action button insertion order via /api/config | integration | `cargo test --test api_config_endpoint::order_preserved` | ❌ W0 | ⬜ |
| CFG-10 | Live TOML edit → ConfigUpdate frame | integration | `cargo test --test watcher_live_reload::toml_reload` | ❌ W0 | ⬜ |
| ACT-01 | `/api/config` JSON projection shape | integration | `cargo test --test api_config_endpoint::shape_includes_base64_bytes` | ❌ W0 | ⬜ |
| ACT-02 | Button click writes bytes to guest | manual | headed smoke: click action, observe response | n/a | ⬜ |
| ACT-03 | `--action` parsing + repeatable + override | unit + integration | `cargo test -p bootroom -- cli::tests::parse_cli_action` + `cargo test --test serve_with_cli_action` | ❌ W0 | ⬜ |
| ACT-04 | funnel.lockInput API + button disable | manual (DevTools) | paste lockInput() in console; observe BUSY pill + disabled buttons | n/a | ⬜ |
| WCH-01 | notify-debouncer-full 300ms debounce | integration | `cargo test --test watcher_debounce::burst_collapses_to_one_event` | ❌ W0 | ⬜ |
| WCH-02 | Atomic-rename detection | integration | `cargo test --test watcher_atomic_rename::tempfile_rename_fires_kernel_changed` | ❌ W0 | ⬜ |
| WCH-03 | Size-stability gate | integration | `cargo test --test watcher_size_stability::partial_write_held_until_stable` | ❌ W0 | ⬜ |
| WCH-04 | ELF magic sniff (non-ELF warns) | integration | `cargo test --test watcher_elf_magic::non_elf_yields_ok_false` | ❌ W0 | ⬜ |
| WCH-05 | KernelChanged frame payload | integration | `cargo test --test watcher_ws_frame::kernel_changed_payload_shape` | ❌ W0 | ⬜ |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/bootroom-core/src/config.rs` — Config/Action/Scenario/Assertion types + unit tests (CFG-02..06)
- [ ] `crates/bootroom-core/src/escape.rs` — `decode_bytes_escape` helper + unit tests (shared by TOML + CLI)
- [ ] `crates/bootroom/tests/config_loading.rs` (CFG-01)
- [ ] `crates/bootroom/tests/check_subcommand.rs` (CFG-07)
- [ ] `crates/bootroom/tests/init_subcommand.rs` (CFG-08)
- [ ] `crates/bootroom/tests/api_config_endpoint.rs` (CFG-09, ACT-01)
- [ ] `crates/bootroom/tests/serve_with_cli_action.rs` (ACT-03)
- [ ] `crates/bootroom/tests/watcher_debounce.rs` (WCH-01)
- [ ] `crates/bootroom/tests/watcher_atomic_rename.rs` (WCH-02)
- [ ] `crates/bootroom/tests/watcher_size_stability.rs` (WCH-03)
- [ ] `crates/bootroom/tests/watcher_elf_magic.rs` (WCH-04)
- [ ] `crates/bootroom/tests/watcher_ws_frame.rs` (WCH-05)
- [ ] `crates/bootroom/tests/watcher_live_reload.rs` (CFG-10)
- [ ] `crates/bootroom/assets/bootroom-example.toml` (inline `const` in init handler)

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Button click reaches guest | ACT-02 | Needs real qemu + guest shell | bootroom serve + demo kernel; click TOML-defined action; observe response in xterm |
| funnel.lockInput disables UI | ACT-04 | DevTools-driven; Phase 4 will be first real caller | Paste `funnel.lockInput()` in console; verify pill → BUSY, action buttons disabled, typing no-op |
| Fresh-kernel banner | WCH-05 + UI | Visual + interactive | `touch <kernel>`; observe non-intrusive banner with LAUNCH + × |
| Config-invalid red banner | CFG-10 + UI | Visual | Save broken `bootroom.toml`; observe red banner; fix; observe it auto-clear |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 10s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
