# Feature Research

**Domain:** Kernel-agnostic web test harness over qemu-wasm (action-button-driven UI + headless CI mode)
**Researched:** 2026-05-17
**Confidence:** MEDIUM-HIGH (HIGH on comparable-tool conventions; MEDIUM on UI/UX specifics for the qemu-wasm niche — bootroom appears to be carving new ground, so several features are extrapolated from analogous tools rather than direct prior art)

## Scope Reminder

PROJECT.md already commits to:
- TOML-config action buttons grouped & rendered in UI
- Serial/stdin injection as the action mechanism
- Watch-the-kernel-path + Launch button for "freshest build"
- Headless `bootroom run --scenario <name>` with serial-output assertions and 0/1 exit
- Out-of-scope: GDB, multi-kernel compare, persistent history/dashboards, non-RISC-V, hot-swap, auth

This document **does not relitigate** those decisions. It surfaces the *additional* features that comparable tools provide so we know what's table stakes vs differentiator vs scope-trap.

---

## Feature Landscape

### Table Stakes (Users Expect These)

Features that — across bootimage, OSDK, Twister, phil-opp's blog_os runner, v86, and qemu-wasm-demo — appear consistently enough that omitting them will make bootroom feel half-finished. The first three are already in PROJECT.md; included here so the table is complete.

| Feature | Why Expected | Complexity | Notes / Comparable |
|---------|--------------|------------|--------------------|
| Serial console rendered live in browser UI | Every kernel-on-QEMU tool centers on the serial log: phil-opp routes `-serial stdio`, OSDK/bootimage do the same, v86 has explicit serial console support. The UI is useless without it. | MEDIUM | Use [xterm.js](https://xtermjs.org/) — battle-tested (powers VS Code, Linode console), handles ANSI/Unicode, GPU-accelerated. Don't roll a textarea. |
| Single "Launch" / restart button | bootimage's `cargo run`, OSDK's `cargo osdk run`, v86's reset button, phil-opp's runner — every comparable tool boils down to "one command/click → guest boots." This *is* the headline UX. | LOW | Already in PROJECT.md. Just wire to qemu-wasm reset. |
| Action buttons fire pre-defined serial input | Differentiator vs the prior-art baseline, but in *this* tool's framing it's table stakes — without it bootroom is just a qemu-wasm wrapper. | MEDIUM | Already in PROJECT.md. Serial-write via qemu-wasm bridge. |
| Action button grouping in UI | TOML config defines groups; users will expect collapsible/visually-separated groups in the rendered UI. v86 groups its controls (disk / state / view); twister groups tests by suite. | LOW | CSS grid + a `<details>` or simple header per group. No framework needed. |
| Reset / power-cycle button | Universal across v86, basilisk-ii web, pebble-qemu-wasm. Distinct from Launch — sometimes you want to nuke state without re-reading the kernel binary. | LOW | qemu-wasm reset call. |
| Exit code 0/1 in headless mode | bootimage codifies `test-success-exit-code`; OSDK does too; this is *how* CI runners detect pass/fail. Already in PROJECT.md. | LOW | Already specified. |
| Configurable per-action and per-scenario timeout | bootimage defaults `test-timeout=300s`; Twister has per-test timeouts; expect-based serial automation universally requires timeouts (covered explicitly in The Good Penguin's "5 Serial Automation Gotchas"). Without timeouts, a hung kernel hangs the whole runner. | LOW | Per-action and per-scenario overrides in TOML; CLI flag for global override. |
| Capture full serial log to file in CI mode | Theseus's `make run` saves serial log; QEMU docs recommend `console.log`; expected by anyone investigating CI failures. | LOW | Append to `--log-file` or stdout in `bootroom run`. |
| Assertion on serial output substring/regex | Twister, OP-TEE's `qemu-check.exp`, phil-opp's integration tests, xqemu's `boot-serial-test`, every Expect-style serial test in the field — they all match on output text. The unit of "did this scenario pass?" is "did this string appear?". | MEDIUM | Already implied by PROJECT.md "asserts on serial output". Make assertions first-class TOML entries (`expect = "..."` or `expect_regex = "..."`) inline with actions. |
| `init` subcommand to scaffold `bootroom.toml` | `cargo init`, `cargo osdk new`, `twister --generate-skeleton` patterns. PROJECT.md already lists `init` in the command surface. Without it, every new kernel project has to copy/paste a config. | LOW | Ship a template via `include_str!`. |
| Live reload on `bootroom.toml` change | Users will save the TOML and expect new buttons to appear without restart. Standard dev-server expectation (vite, mdbook serve, any modern dev tool). | MEDIUM | Already partially implied by "watch kernel path"; extend the watcher to the config file. Send SSE/WebSocket nudge to the UI. |
| Send raw text input from UI (free-form input box) | xterm.js gives this naturally; phil-opp's serial setup supports it; v86's terminal supports typing. Required for the "I want to debug interactively without making a button for every keystroke" case. | LOW | Wire xterm.js `onData` to serial-write. Free with xterm.js. |
| Clear/scroll/copy serial output in UI | Universal terminal expectations; xterm.js gives copy/select; clear button is one line. | LOW | xterm.js built-ins + a Clear button. |
| Visible boot progress / "guest is running" indicator | v86 has status indicators; qemu-wasm-demo shows progress; without it users will think the page is dead during a long boot. | LOW | Simple status pill: Idle / Loading / Running / Halted. Driven by serial activity + qemu-wasm lifecycle hooks. |
| Surface kernel binary path/size/mtime in UI | "Did the new build actually get picked up?" is the #1 question users will ask. Twister and bootimage both echo the artifact path on each run. | LOW | One info line at the top of the UI. mtime answers "freshest build" doubts. |
| Verbose mode for CI (`--verbose` / `-v`) | bootimage, OSDK, twister all have it. CI failures with no logs are useless. | LOW | Standard `tracing` / `env_logger` plumbing. |
| Reasonable defaults so no config works | phil-opp's runner has sane defaults; bootimage works on a fresh kernel with minimal config. If `bootroom.toml` is missing or empty, the tool should still launch the kernel and show the serial console. | LOW | Defaults: no actions, no scenarios, just the launch button + serial view. |
| Documented machine/CPU defaults per arch | qemu-system-riscv64 docs explicitly note "there is no default machine — you must specify -M". bootroom must pick one (`virt`) or surface the choice clearly. Twister abstracts this per platform. | LOW | Default `-machine virt -cpu rv64` for RISC-V. Make overridable in TOML. |

### Differentiators (Competitive Advantage)

Where bootroom can genuinely beat the alternatives (bootimage/OSDK runners, raw `make qemu`, hand-rolled Expect scripts). These should align with PROJECT.md's Core Value: "Press one button, get the freshest kernel running in a browser with a click-to-trigger scenario library."

| Feature | Value Proposition | Complexity | Notes / Comparable |
|---------|-------------------|------------|--------------------|
| Browser-based, zero-install for reviewers | Unique vs *every* native-QEMU tool. A NORN contributor can hand a colleague a URL (or a static site artifact) and they reproduce the bug without installing QEMU/cross-toolchain/etc. Direct riff on what qemu-wasm-demo enables but generalized. | Already in scope | Falls out of qemu-wasm + serving static assets. |
| One TOML file defines *both* the interactive UI *and* the CI scenarios | bootimage/OSDK separate test runner config from any UI. Expect scripts duplicate logic that would be in tests. Twister test config is YAML-heavy. **Unifying both surfaces under one source of truth is genuinely novel** in this niche. | Already in scope | The whole UX hinges on this — make sure interactive actions and headless scenarios share the same primitives in the TOML schema. |
| Scenarios composed of named actions (not duplicated byte strings) | Expect scripts are notoriously copy-pasted. Lets a user click action buttons interactively, then declare `scenario = ["boot_ok", "login_root", "run_smoke"]` referencing those same actions. | MEDIUM | Schema: actions are named; scenarios are ordered lists of action refs + interleaved assertions. |
| Record-and-replay: capture interactive button sequence → scenario stub | A user clicks through to reproduce a bug, then hits "Save as scenario" — bootroom emits TOML they can paste into config. Massive UX win, has no equivalent in bootimage/OSDK/twister/expect. | MEDIUM | Track action-click timeline + serial-output snapshots, emit TOML. Optional v1.x feature. |
| Static export of a "frozen reproducer" | `bootroom export --kernel … --scenario bug42 --out repro/` produces a static HTML+wasm+kernel bundle anyone can open. Bug reports become URLs, not "run these 5 commands". No comparable tool offers this. | MEDIUM | Wraps existing assets + a single auto-running scenario. Lands well after MVP. |
| CLI flags append/override actions for ad-hoc experiments | Already in PROJECT.md. **This is differentiated** — bootimage/OSDK have no equivalent. Lets users prototype an action without editing TOML. | LOW | Already specified. CLI `--action 'name=label:bytes'` etc. |
| JUnit XML / TAP / GitHub Actions annotation output from `bootroom run` | Twister emits multiple formats; bootimage doesn't. For a CI tool, "integrates with the CI's native UI" is a real selling point. | LOW-MEDIUM | Emit JUnit XML behind `--report-format junit`. GitHub Actions `::error::` annotations behind `--report-format gha`. |
| Watch + auto-restart in CI mode for TDD | `bootroom run --watch` re-runs the scenario whenever the kernel binary changes. Bridges the dev-loop / CI-loop gap. | LOW | Same watcher as dev mode, different command. |
| Per-action keyboard shortcuts in UI | Twister and bootimage have no UI. xterm.js + a small keymap layer = "press F2 to fire action `boot`". Demonstrably useful for kernel devs who'd rather not mouse. | LOW | TOML `key = "F2"` per action. |
| Snapshot / save-state buttons | v86 has this prominently. qemu-wasm currently lacks snapshot support per the FOSDEM 2025 talk — flag as feasibility-dependent. If qemu-wasm gains it, exposing it as an action-class is one of the highest-value extensions. | MEDIUM (post-MVP) | Track upstream qemu-wasm for snapshot support; design the action schema so it can host non-serial action kinds later. |
| Screenshot of guest framebuffer button | Even for serial-only kernels, attaching a screenshot to a bug report is common UX. v86, basilisk-ii web, all expose it. For NORN-style serial-only kernels it's lower-value but trivial to support. | LOW | qemu-wasm canvas → `toDataURL`. |
| Headless mode runs *without* a browser at all | If we wire qemu-wasm to a headless WASM runtime (wasmtime/wasmer with WASI), CI doesn't need Chromium. **Huge** for CI runners that lack a display. Twister's QEMU path doesn't need a browser; matching that bar makes bootroom viable in restricted runners. | LARGE | Major feasibility question — qemu-wasm currently targets browsers (uses Web Workers, SharedArrayBuffer). May require running headless Chromium under the hood for v1; native-WASI path is v2+. Flag as a research question for STACK research. |
| `bootroom doctor` — preflight checks | Twister-style health check: is the kernel path readable, is the TOML parseable, do the byte sequences in actions look sane, can qemu-wasm assets be served? Catches 80% of "it doesn't work" tickets. | LOW | One-shot diagnostic subcommand. |
| Inline assertion failures shown in UI (red highlight in serial pane) | When an interactive session runs a scenario, failing assertions should be visually obvious in the same xterm.js view. Nobody else does this — even Expect scripts dump failure to stdout and you scroll. | MEDIUM | xterm.js custom renderer hooks or marker decorations. |

### Anti-Features (Commonly Requested, Often Problematic)

Things adjacent tools or naive users will ask for. PROJECT.md already covers most explicit out-of-scopes; this section documents the *less obvious* traps surfaced by the comparable-tool research.

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| In-browser GDB / breakpoints UI | "v86 has a debugger panel; PCjs has one." Sounds essential for a kernel tool. | qemu-wasm's TCG-in-wasm story doesn't currently expose GDB stub on the wasm side; building a UI around a missing primitive is a swamp. Already explicitly out-of-scope in PROJECT.md, but worth restating. | Users wanting GDB drop to native QEMU + `target remote :1234`. bootroom focuses on the serial-assertion path. |
| Custom plugin system for actions | "What if I want to fire a QMP command, or POST to a webhook, or run a shell command?" | Plugin systems balloon API surface, security review, and version-compat work. Twister learned this the hard way with handler classes. | Keep action kinds as a small closed enum: `serial_bytes`, `serial_text`, `reset`, `wait_ms`, plus future `screenshot`, `snapshot` when qemu-wasm supports them. Anything more exotic, the user wraps `bootroom run` in a shell script. |
| Real-time multi-user collaboration ("Google-Docs for kernel debugging") | Web-based tool → "let's add presence/cursors". | Massive complexity, sync conflicts on serial input, conflicts with the "no auth" stance. Already out-of-scope but seductive. | Static export ("frozen reproducer") covers the asynchronous-sharing case. |
| Auto-fuzz / random input generator | Once you have a serial-write primitive, "just fuzz it" is one-line away. | Fuzzing serial input rarely catches real kernel bugs; competes with syzkaller/AFL which are far more capable. Scope sprawl, no clear win. | Users wanting fuzzing run syzkaller against a native build. bootroom stays in scripted-scenario territory. |
| Web IDE / kernel source editor in the UI | "I'm already in the browser, why can't I edit the kernel here?" | bootroom is a *test harness*, not an IDE. Conflates concerns; building this competes with VS Code Web. | Keep the loop external: edit in your editor, `make` in your kernel repo, click Launch in bootroom. The "freshest build pickup" is what makes this loop work. |
| Persistent test history dashboard | "Show me pass-rate over time." | Already out-of-scope per PROJECT.md. Trap: small JSON store → "could you add charts?" → full DB. | Emit JUnit XML; CI systems (GitHub Actions, GitLab) own longitudinal data. |
| Multi-architecture support in v1 | "qemu-wasm supports x86_64 + AArch64; why limit to RISC-V?" | Per-arch defaults (machine, CPU, console device, debug-exit mechanism) all differ. RISC-V already enough surface. PROJECT.md is firm on this. | v1 = RISC-V. Schema design should allow per-action arch tagging so v2 can extend cleanly. |
| "Just embed bootimage's runner format" | "There's already a Rust-OS test convention; reuse it." | bootimage's runner expects a native QEMU binary and `isa-debug-exit` (x86-specific). RISC-V doesn't use isa-debug-exit; qemu-wasm doesn't expose the same exit semantics. | Define our own minimal scenario schema; *interoperate* by being callable from a `cargo runner` shim if someone wants it. |
| Browser dialogs/popups for action confirmation | "Don't let users accidentally fire destructive actions." | Confirmation modals destroy the flow; this is a dev tool. | TOML can mark actions as `confirm = true` if a user really wants it; off by default. |
| Hot-swap kernel mid-run | Out-of-scope per PROJECT.md; users hit Launch again. Listed here for the research-trail. | qemu-wasm restart is cheap; live-patching is a research project of its own. | Re-launch is the supported path. |

---

## Feature Dependencies

```
bootroom.toml schema (actions, groups, scenarios, assertions)
    │
    ├──required-by──> Action buttons render in UI
    │                     └──required-by──> Action-click → serial-write
    │                                            └──required-by──> Keyboard shortcuts per action
    │                                            └──required-by──> Inline assertion failures in UI
    ├──required-by──> Headless `bootroom run --scenario`
    │                     └──required-by──> Exit code 0/1
    │                     └──required-by──> JUnit/TAP/GHA report formats
    │                     └──required-by──> `--watch` CI mode
    └──required-by──> `init` subcommand template
                          └──enhances──> Record-and-replay → emit TOML

qemu-wasm submodule integration
    ├──required-by──> Serial console rendered in UI (xterm.js as front end)
    │                     └──required-by──> Send raw text from UI
    │                     └──required-by──> Clear/copy serial output
    ├──required-by──> Launch / Reset buttons
    ├──required-by──> Screenshot of framebuffer (post-MVP)
    └──required-by──> Snapshot/save-state (post-MVP, blocked on qemu-wasm support)

Kernel-path watcher
    ├──required-by──> "Freshest build" Launch behavior
    └──required-by──> `bootroom run --watch`

Config-file watcher
    └──required-by──> Live reload on bootroom.toml change

Static export (frozen reproducer)
    ├──requires──> Static asset embedding (already in scope via include_dir!)
    └──requires──> Self-contained kernel binary + scenario JSON next to assets

Headless-without-browser CI path  ─── BLOCKED ON ─── qemu-wasm native-WASI feasibility
                                                          (research flag for STACK)
```

### Dependency Notes

- **TOML schema is the keystone.** Both the interactive UI and the headless CI mode read it. Schema design needs to land before either rendering or runner work — getting the schema wrong forces breaking changes across both surfaces.
- **xterm.js gives multiple table-stakes features at once.** Picking it covers: serial render, free-form input, clear/copy, ANSI handling, copy/paste. Avoid temptation to roll a custom terminal.
- **Watcher infrastructure is shared.** The kernel-path watcher and config-file watcher should use the same primitive (`notify` crate in Rust). Build once, use twice.
- **Snapshot actions depend on qemu-wasm capability.** Per the FOSDEM 2025 slides, qemu-wasm is built on TCG-to-wasm and a partial JIT. Snapshot/save-state isn't called out as supported; design the action schema to host this kind later but don't promise it in v1.
- **Headless-without-browser is a feasibility question.** qemu-wasm is built around Web Workers + SharedArrayBuffer for performance. Running it under wasmtime/wasmer-WASI is *plausible* but unproven. For v1, `bootroom run` may shell out to headless Chromium under the hood — flag this for STACK research.
- **Static export builds on existing asset embedding.** `include_dir!` already serves wasm + JS; export = same assets + a kernel + a baked-in auto-run config. Low marginal cost once MVP ships.

---

## MVP Definition

### Launch With (v1)

The minimum bar for "press one button, get the freshest kernel running, click scenario, get pass/fail." Everything below is either already in PROJECT.md's Active list or a direct support-feature for it.

- [ ] `bootroom serve --kernel <path>` runs a local web server with the UI
- [ ] qemu-wasm submodule served as static assets, boots the given kernel
- [ ] Launch button re-reads the kernel binary (freshest-build pickup) and boots
- [ ] Reset button (separate from Launch — no kernel re-read)
- [ ] Live serial console rendered via xterm.js (read + write + free-form input)
- [ ] Status indicator (Idle / Loading / Running / Halted)
- [ ] Kernel info line (path, size, mtime) visible in UI
- [ ] TOML config loaded from `./bootroom.toml` or `--config <path>`
- [ ] Action buttons rendered, grouped per TOML, click → serial-write
- [ ] CLI `--action` flag appends/overrides without editing TOML
- [ ] Live reload on `bootroom.toml` change
- [ ] `bootroom run --kernel <path> --scenario <name>` headless mode
- [ ] Scenarios = ordered action refs + serial-output assertions (substring + regex)
- [ ] Per-action and per-scenario timeouts
- [ ] Exit code 0 on pass, non-zero on fail
- [ ] `--log-file` captures full serial transcript
- [ ] `bootroom init` scaffolds a starter `bootroom.toml`
- [ ] `--verbose` flag for debug-level CI logging
- [ ] Sensible defaults: works with no config, with `-machine virt -cpu rv64`
- [ ] `cargo install bootroom` works; prebuilt Linux+macOS binaries on GitHub Releases

### Add After Validation (v1.x)

Trigger: NORN's CI is running real scenarios, first external kernel adopts the tool, or a user files a request grounded in real friction.

- [ ] Per-action keyboard shortcuts (`key = "F2"` in TOML) — *trigger:* user with >10 actions complains about mousing
- [ ] JUnit XML report format for `bootroom run` — *trigger:* first CI integration that wants pretty test results
- [ ] GitHub Actions annotation format — *trigger:* same
- [ ] `bootroom run --watch` for local TDD loops — *trigger:* developer requests it (likely early; cheap)
- [ ] `bootroom doctor` preflight diagnostics — *trigger:* second support ticket of the form "it doesn't start"
- [ ] Screenshot button (framebuffer → PNG download) — *trigger:* any kernel with VGA/framebuffer adopts the tool
- [ ] Inline assertion failure decorations in the xterm.js view — *trigger:* users running scenarios interactively can't find where it failed
- [ ] Record-and-replay (button-click → TOML emit) — *trigger:* repeated "I did the steps but can't write the scenario" reports

### Future Consideration (v2+)

Defer until product-market fit is clear or upstream dependencies catch up.

- [ ] Static "frozen reproducer" export — *defer:* nice-to-have, large surface area for asset bundling
- [ ] Snapshot / save-state actions — *defer:* blocked on qemu-wasm support; design schema to accommodate
- [ ] Headless CI without browser (native-WASI qemu-wasm) — *defer:* major feasibility research; v1 can shell out to headless Chromium if needed
- [ ] Additional architectures (x86_64, AArch64) — *defer per PROJECT.md:* RISC-V only in v1; schema design should allow per-action arch tagging
- [ ] HMP/QMP-style action kind for richer guest control — *defer:* `serial_bytes` covers ~all NORN-class needs; QMP requires qemu-wasm to expose a control socket

---

## Feature Prioritization Matrix

Compact view of the top-of-mind items. P1 = MVP (above list). P2 = v1.x triggers. P3 = v2+.

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| Launch with freshest-build pickup | HIGH | LOW | P1 |
| xterm.js serial console | HIGH | MEDIUM | P1 |
| TOML-defined action buttons | HIGH | MEDIUM | P1 |
| Scenarios + serial assertions | HIGH | MEDIUM | P1 |
| Headless `run` with exit codes | HIGH | LOW | P1 |
| Live reload on TOML change | MEDIUM | MEDIUM | P1 |
| `init` subcommand | MEDIUM | LOW | P1 |
| `--action` CLI flag for ad-hoc actions | MEDIUM | LOW | P1 |
| Per-action keyboard shortcuts | MEDIUM | LOW | P2 |
| JUnit XML / GHA annotation output | MEDIUM | LOW | P2 |
| `bootroom run --watch` | MEDIUM | LOW | P2 |
| `bootroom doctor` | MEDIUM | LOW | P2 |
| Inline assertion failure markers in UI | MEDIUM | MEDIUM | P2 |
| Screenshot button | LOW (NORN is serial-only) | LOW | P2 |
| Record-and-replay → TOML | HIGH | MEDIUM | P2/P3 |
| Static frozen-reproducer export | HIGH | MEDIUM | P3 |
| Headless without browser | HIGH | HIGH | P3 |
| Snapshot/save-state actions | MEDIUM | MEDIUM (blocked) | P3 |
| Multi-arch | MEDIUM (no current consumer) | HIGH | P3 |
| GDB UI | (declined) | — | Out of scope |
| Plugin system | (declined) | — | Out of scope |

**Priority key:**
- P1: Must have for launch (MVP list above)
- P2: Should have, add when triggered
- P3: Future consideration

---

## Competitor Feature Analysis

| Feature | bootimage / phil-opp runner | Asterinas OSDK | Zephyr Twister | v86 / qemu-wasm-demo (browser) | bootroom plan |
|---------|------------------------------|----------------|----------------|--------------------------------|----------------|
| Boot kernel under QEMU | Yes, native | Yes, native | Yes, native | Yes, browser | Yes, browser (qemu-wasm) |
| Serial output assertions | Implicit via test framework | Implicit via test framework | Yes, first-class (`harness`, `regex_capture`) | Yes (v86 example) | Yes, first-class in TOML |
| UI for interactive use | None (CLI only) | None | None | Yes (control panel, terminal) | Yes (action buttons + xterm.js) |
| Headless CI mode | Yes (exit codes) | Yes (exit codes) | Yes (exit codes + reports) | No | Yes (`bootroom run`) |
| Action-button abstraction | No | No | No | Hardcoded controls (reset/pause/state) | **Yes, TOML-driven — bootroom's signature feature** |
| Same config for interactive + CI | N/A (CLI only) | N/A (CLI only) | Twister YAML is CI-only | N/A (no CI mode) | **Yes — unified TOML — bootroom's signature feature** |
| Watch + auto-reload kernel | No | No | No | No | **Yes — bootroom's signature feature** |
| Zero-install reviewer experience | No | No | No | Yes (URL → emulator) | Yes (URL → emulator + scenario library) |
| Reports (JUnit/TAP/GHA) | Limited | Limited | Yes, multiple formats | No | Planned for v1.x |
| Multi-arch | x86-centric (isa-debug-exit) | Multi (x86, RISC-V, ARM) | Very multi (its raison d'être) | x86 (v86) / multi (qemu-wasm) | RISC-V only in v1 |
| GDB integration | Yes, native | Yes, native | Yes, native | Limited / per-tool | **Explicitly no** |
| Snapshot/save-state | No | No | No | Yes (v86) | Future — blocked on qemu-wasm |
| Plugin system | No | No | Yes, handler classes | No | **Explicitly no** |

**What this matrix shows:** bootroom's defensible niche is the intersection of *browser-delivered* (v86/qemu-wasm side of the table) and *CI-integrated with scriptable assertions* (bootimage/twister side). No tool in the comparable set occupies both columns. The signature features — TOML-driven action library, unified interactive/CI config, freshest-build watch — are where bootroom can credibly claim to be doing something new, while everything else stays inside the well-trodden conventions users already expect.

---

## Sources

### Kernel test harnesses (rust-osdev space)
- [rust-osdev/bootimage on GitHub](https://github.com/rust-osdev/bootimage) — `cargo test`-based kernel runner, exit codes, timeouts, runner config
- [bootimage runner help](https://github.com/rust-osdev/bootimage/blob/master/src/help/runner_help.txt)
- [phil-opp — Testing chapter, Writing an OS in Rust](https://os.phil-opp.com/testing/) — `-serial stdio`, isa-debug-exit, integration test pattern
- [phil-opp — Integration Tests](https://os.phil-opp.com/integration-tests/)
- [asterinas/asterinas](https://github.com/asterinas/asterinas) + [Asterinas Book](https://asterinas.github.io/book/) — OSDK `cargo osdk test`/`run` patterns
- [theseus-os/Theseus](https://github.com/theseus-os/Theseus) — `make run` + serial logging conventions
- [Redox OS — Quick Workflow](https://doc.redox-os.org/book/quick-workflow.html)

### CI-oriented OS/firmware test runners
- [Zephyr Twister — Test Runner docs](https://docs.zephyrproject.org/latest/develop/test/twister.html) — handler classes, QEMUHandler, report formats, per-test timeouts
- [Espressif — Pytest + NuttX serial testing](https://developer.espressif.com/blog/2024/10/pytest-testing-with-nuttx/) — pattern of pytest fixtures wrapping a serial port
- [QEMU docs — Testing in QEMU](https://www.qemu.org/docs/master/devel/testing/main.html)
- [xqemu boot-serial-test.c](https://github.com/xqemu/xqemu/blob/master/tests/boot-serial-test.c) — "expect" field on serial output
- [OP-TEE qemu-check.exp](https://github.com/OP-TEE/build/blob/master/qemu-check.exp) — Expect-style serial assertions in practice
- [The Good Penguin — 5 Serial Automation Gotchas](https://www.thegoodpenguin.co.uk/blog/5-serial-automation-gotchas/) — timeouts, retries, kernel/userspace output mixing

### Browser-based emulator UIs
- [copy/v86 on GitHub](https://github.com/copy/v86) + [v86 demo page](https://copy.sh/v86/) — control panel surface (pause/reset/state/screenshot/serial)
- [v86 serial example](https://github.com/copy/v86/blob/master/examples/serial.html) — wait-for-prompt pattern in a browser context
- [ktock/qemu-wasm](https://github.com/ktock/qemu-wasm) + [qemu-wasm-demo](https://ktock.github.io/qemu-wasm-demo/) — the actual upstream this project sits on
- [FOSDEM 2025 — Running QEMU Inside Browser (slides PDF)](https://archive.fosdem.org/2025/events/attachments/fosdem-2025-6290-running-qemu-inside-browser/slides/238760/slides_1dDtpcS.pdf) — capability and limitation map for qemu-wasm
- [Pebble qemu-wasm](https://ericmigi.github.io/pebble-qemu-wasm/) — example of a domain-specific qemu-wasm UI in the wild
- [Runno (runno.dev)](https://runno.dev/) + [taybenlor/runno](https://github.com/taybenlor/runno) — WASI-side sandbox runtime, the closest "browser-or-not" reference
- [Leaning Tech WebVM / CheerpX](https://github.com/leaningtech/webvm) — sidebar UI pattern, status indicators, status pill design
- [jsdf/macemu](https://github.com/jsdf/macemu) + [oldweb-today/macemu](https://github.com/oldweb-today/macemu) — Emscripten + Web Worker + SharedArrayBuffer architecture pattern
- [PCjs about page](https://www.pcjs.org/about/) — XML-defined machine layout with built-in debugger UI

### Action / keystroke / scenario conventions
- [QEMU `sendkey` documentation context (HMP/QMP)](https://www.qemu.org/docs/master/devel/writing-monitor-commands.html) — convention for symbolic key names and `-`-joined chords
- [mvidner/sendkeys](https://github.com/mvidner/sendkeys) — text-to-sendkey translator (informs action TOML schema design)
- [qemu.qmp Python library docs](https://qemu.readthedocs.io/projects/python-qemu-qmp/en/latest/) — JSON-over-socket control surface (future action-kind reference)
- [Expect manpage](https://www.tcl-lang.org/man/expect5.31/expect.1.html) — the prior-art semantics for `expect`/`send` that bootroom scenarios should feel familiar to

### Terminal front-end
- [xterm.js homepage](https://xtermjs.org/) + [xtermjs/xterm.js GitHub](https://github.com/xtermjs/xterm.js/) — recommended serial-console front end

---

*Feature research for: kernel-agnostic web test harness over qemu-wasm*
*Researched: 2026-05-17*
