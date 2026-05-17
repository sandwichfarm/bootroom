# bootroom

## What This Is

A web-based test harness for RISC-V kernels (and any qemu-wasm guest). It serves QEMU compiled to WebAssembly with a config-driven UI of action buttons that drive scenarios against the running guest — for local debug *and* CI. First consumer is the NORN kernel; the tool itself is kernel-agnostic.

## Core Value

**Press one button, get the freshest kernel running in a browser with a click-to-trigger scenario library.** If everything else fails, that one path must stay friction-free.

## Requirements

### Validated

(None yet — ship to validate)

### Active

- [ ] Single command to build the tool (`make` or `cargo build`).
- [ ] Single command to launch the dev server pointed at a kernel binary (`bootroom serve --kernel <path>`).
- [ ] Browser UI loads the kernel via the bundled qemu-wasm submodule.
- [ ] Tool watches the kernel path; the "Launch" button reloads with the freshest build.
- [ ] Action buttons are defined in TOML config, grouped, and rendered in the UI.
- [ ] Pressing an action button sends pre-defined serial/stdin input to the guest.
- [ ] CLI flags can append/override action buttons for quick experiments without editing config.
- [ ] Headless CI mode: `bootroom run --kernel <path> --scenario <name>` runs a button sequence, asserts on serial output, exits 0/1.
- [ ] Installable from outside this repo via `cargo install bootroom` and prebuilt GitHub Release binaries (Linux + macOS).
- [ ] Command surface stays small: `make`, `make install`, `bootroom <subcommand> <args>`.

### Out of Scope

- GDB / step-through debugging — defer to v2; serial-based assertions cover initial test needs.
- Multi-kernel side-by-side comparison — niche; defer until requested.
- Persistent test history / dashboards — out of scope for a dev tool; CI artifacts handle longitudinal data.
- Non-RISC-V architectures — v1 is RISC-V only even though qemu-wasm supports more.
- Hot-swap kernel mid-run — user re-clicks "Launch" instead of live-replacing the running guest.
- Authentication / multi-user — local dev tool; not exposed to the public internet.

## Context

- **Workspace:** `~/Develop/norn-web` (will be renamed to `bootroom` — see Key Decisions). Already contains the `qemu-wasm` git submodule.
- **First consumer:** the NORN RISC-V kernel at `~/Develop/nostros`. NORN's CI/CD and local debug flows will pull `bootroom` as an external dependency.
- **qemu-wasm:** the submodule provides QEMU compiled to WebAssembly. `bootroom` serves its static assets and bridges UI events to the wasm runtime (serial/stdin injection initially).
- **Action semantics:** an "action" is a labeled, optionally-grouped button that sends a fixed byte sequence to the guest serial console. A "scenario" is an ordered sequence of actions plus optional serial-output assertions — the unit CI runs against.
- **HMR analogy:** not true HMR; rather, "freshest-build pickup" — user runs `make` in the kernel repo, then clicks Launch in `bootroom` to reboot the wasm guest with the new artifact.

## Constraints

- **Tech stack — Rust:** single static binary, embeds static assets via `include_dir!`. No Node.js or Python runtime required to run the tool. Web UI is vanilla JS + HTML (no build step).
- **Config format — TOML:** action buttons, groups, and scenarios are defined in a TOML file (default `bootroom.toml` in CWD; overridable via `--config`).
- **Distribution — cargo + binaries:** must be installable in one step from any kernel CI: `cargo install bootroom` or `curl | tar -xz` from a GitHub Release.
- **Command surface — minimal:** the user must never need >1 long-form command to do common tasks. Subcommands are short verbs (`serve`, `run`, `init`).
- **License — MIT OR Apache-2.0:** Rust-ecosystem dual license. Maximum downstream compatibility (kernel projects of either license can pull it in).
- **Repo external-callable:** the binary, once installed, must run anywhere; no assumption that `bootroom`'s repo is checked out.

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Project + binary name = `bootroom` | Generic, kernel-agnostic; survives if NORN is ever renamed or other kernels adopt the tool. | — Pending |
| Repo will be renamed `norn-web` → `bootroom` | Match project identity now that it's generalized. Action item for Phase 1 setup. | — Pending |
| Language = Rust | Single static binary, fits kernel-toolchain ecosystem, distributable via crates.io + GH Releases. | — Pending |
| Action model = serial/stdin injection | Works with any kernel, no kernel-side cooperation required; can grow to QMP/shell-cmd later. | — Pending |
| Kernel discovery = watch a path (default) + `--kernel` override | "Click Launch → freshest build" is the headline UX; mtime-watch makes it free. | — Pending |
| Config format = TOML | Readable, comment-friendly, Rust-ecosystem standard, layered/grouped structures map cleanly. | — Pending |
| UI = vanilla JS + HTML, embedded in the Rust binary | No npm in the toolchain, smallest install, fastest cold start. | — Pending |
| CI mode = `bootroom run --scenario …` with exit codes | Maps naturally to standard CI runners; no extra shim required. | — Pending |
| Distribution = `cargo install` + prebuilt release binaries | Covers both Rust-equipped devs and CI runners that don't want a toolchain. | — Pending |
| License = MIT OR Apache-2.0 | Rust convention; permissive for downstream kernels. | — Pending |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `/gsd-transition`):
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions
5. "What This Is" still accurate? → Update if drifted

**After each milestone** (via `/gsd-complete-milestone`):
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-05-17 after initialization*
