# Pitfalls Research

**Domain:** Rust CLI + qemu-wasm browser kernel test harness (RISC-V)
**Researched:** 2026-05-17
**Confidence:** HIGH for headers/distribution/file-watching pitfalls (well-documented), MEDIUM for qemu-wasm-specific behaviors (project is experimental, less written about), MEDIUM for serial-console parsing edge cases (drawn from embedded HIL literature).

This document is opinionated and specific to `bootroom`. Generic web/Rust advice is omitted. Every pitfall is sourced from real failure modes documented in qemu-wasm, embedded HIL CI, Rust CLI distribution, and buildless web-UI ecosystems.

---

## Critical Pitfalls

### Pitfall 1: Missing COOP/COEP — qemu-wasm silently won't boot

**What goes wrong:**
qemu-wasm needs `SharedArrayBuffer` for MTTCG and Emscripten pthreads. Without `Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy: require-corp` both set on the page response, `crossOriginIsolated` is `false`, `SharedArrayBuffer` is `undefined`, and the wasm guest either fails to initialize threads, hangs on first instance, or throws a confusing module-instantiation error. Worst case: things "work" on Chrome with insecure flags during dev, then break in CI / on the user's laptop.

**Why it happens:**
Devs treat the dev server as "just static files." Plain `python -m http.server` and most ad-hoc Rust static-file servers don't set COOP/COEP. The failure mode is silent: `typeof SharedArrayBuffer === "undefined"` only surfaces deep inside Emscripten's runtime, looking like a wasm bug, not a header bug. Compounding: many cross-origin subresources (CDN scripts, fonts) need `Cross-Origin-Resource-Policy: cross-origin` or they'll be blocked by COEP.

**How to avoid:**
- Make the Rust HTTP server set COOP=`same-origin` and COEP=`require-corp` on **every** response that loads the harness page (HTML, JS, wasm, worker scripts). Do this in the lowest-level middleware so it can't be forgotten.
- Embed and self-host **all** assets (xterm.js, any UI libs) so no cross-origin subresources exist. Do not ship a CDN dependency. The `include_dir!` / `rust-embed` choice for static assets makes this natural — just don't reach for a `<script src="https://...">` "for convenience."
- Boot-time smoke check: on `serve`, after starting, hit `/` with an in-process check and assert headers are present; fail fast if not.
- Add a JS-side guard rail that reports `crossOriginIsolated`, `typeof SharedArrayBuffer`, and `navigator.hardwareConcurrency` to the UI on load, and refuses to start the guest with a clear inline error if any are wrong.

**Warning signs:**
- `Uncaught ReferenceError: SharedArrayBuffer is not defined` in the browser console.
- Emscripten wasm aborts with cryptic `pthread_create` errors.
- Works in one tab, breaks in another (a reload that picked up a cross-origin asset triggered COEP block).
- DevTools → Application → "Frames" shows `crossOriginIsolated: false`.

**Phase to address:** Early (must be in Phase 1 — the very first "serve a static page" milestone). If this is wrong it blocks every subsequent feature.

Source: [web.dev COOP/COEP guide](https://web.dev/articles/coop-coep), [Cinevva COOP/COEP tutorial](https://app.cinevva.com/tutorials/coop-coep-sharedarraybuffer.html).

---

### Pitfall 2: Treating qemu-wasm like native QEMU (feature/flag drift)

**What goes wrong:**
Developers copy a working `qemu-system-riscv64` CLI from a Linux setup and try to drive qemu-wasm with the same flags. Several things diverge:
- Most non-virtio device backends aren't compiled in (no KVM, no host audio, no host-network NIC; networking is Fetch-API-restricted to HTTP(S) only).
- Disk images load via Emscripten's virtual FS / Fetch, not host paths — `-drive file=/abs/path` is meaningless.
- `-enable-kvm`, `-accel kvm`, `-accel hvf`, `tcg,thread=multi` accel options behave differently or not at all; TCG is the only accelerator and it's TCI-first with selective wasm JIT.
- `-display` / `-vnc` make no sense — display is the browser canvas.
- Snapshot/savevm (migration) state likely won't survive a page reload in any clean way; "reset" usually means tearing down the WebAssembly.Instance and rebuilding from the kernel asset.

**Why it happens:**
The project ships as a QEMU patch series, so it *looks* like normal QEMU. Documentation focuses on what works (boot a container) rather than what doesn't.

**How to avoid:**
- Pin a known-good QEMU command line for the RISC-V `virt` machine inside `bootroom`'s server (kernel direct boot via `-kernel`, ns16550a UART to `chardev/stdio`-equivalent, virtio-blk for any disk needed). Treat this as `bootroom`'s curated subset — don't expose the full QEMU CLI as a passthrough.
- Have the launch path **always** re-create a fresh `WebAssembly.Instance` rather than try to reset internal QEMU state. "Click Launch → fresh boot from latest kernel artifact" is the contract; everything else is best-effort.
- Document an explicit feature matrix (yes/no/partial) for QEMU options that bootroom supports, in `bootroom.toml`'s comment header or a `bootroom doctor` subcommand.
- Use the RISC-V `virt` machine specifically (it's the one with documented MTTCG, ns16550a UART at 0x10000000, and OpenSBI/fw_dynamic path qemu-wasm targets); avoid esoteric machines.

**Warning signs:**
- A flag works in headed Chrome but the same scenario in headless CI prints an "unknown device" error.
- "Reset" leaves the guest in a half-state (kernel sees stale memory, or hangs on second boot).
- Disk images "load" but read returns zeros — actually backing the virtual FS, not what the user thinks.

**Phase to address:** Early (Phase 2 — first kernel boot). Lock the supported flag set before scenarios get built on top of it; once scenarios depend on flags, removing them is breaking.

Source: [ktock/qemu-wasm](https://github.com/ktock/qemu-wasm), [QEMU RISC-V virt docs](https://www.qemu.org/docs/master/system/riscv/virt.html), [FOSDEM 2025 qemu-wasm slides](https://archive.fosdem.org/2025/events/attachments/fosdem-2025-6290-running-qemu-inside-browser/slides/238760/slides_1dDtpcS.pdf).

---

### Pitfall 3: Embedded-assets workflow that's impossible to iterate on

**What goes wrong:**
Using `include_dir!("$CARGO_MANIFEST_DIR/assets")` for the web UI embeds the files at compile time. Every UI tweak — change a button label, fix a CSS px value — requires `cargo build`. Worse: edit `index.html`, refresh browser, see no change, lose 20 minutes wondering why. Iteration speed for the harness's own UI collapses, and contributors stop iterating on it.

**Why it happens:**
The PROJECT.md correctly chose `include_dir!`-style embedding for the release artifact (single static binary). But that decision is also applied in `cargo run` / dev mode, where it's wrong.

**How to avoid:**
- Use `rust-embed` (debug = filesystem, release = embedded) **or** keep `include_dir!` but add a `--assets-dir <path>` CLI flag that overrides the embedded copy at runtime. The flag costs ~10 lines and unlocks live editing.
- For the kernel binary path itself (a separate concern), the watcher path handles iteration. Make sure both kinds of "freshness" — UI assets and kernel artifact — work in dev mode without a rebuild of `bootroom`.
- Document in CONTRIBUTING the dev loop: `cargo run -- serve --assets-dir web/ --kernel ...`. New contributors hit this on day one.

**Warning signs:**
- "Why isn't my CSS change showing up?" in the issue tracker.
- People committing print statements to debug "did the file even load."
- Build times measured in seconds for every UI tweak.

**Phase to address:** Early (Phase 1 — alongside first asset embedding). The longer you wait, the more habitually contributors run `cargo build` for UI changes and stop noticing the friction.

Source: [rust-embed docs](https://docs.rs/rust-embed/latest/rust_embed/trait.RustEmbed.html), [include_dir docs](https://docs.rs/include_dir/latest/include_dir/).

---

### Pitfall 4: File-watcher fires mid-write — Launch boots a corrupt kernel

**What goes wrong:**
The kernel build pipeline (e.g. `cargo build` in NORN) writes the kernel ELF/binary in chunks. The watcher fires on the first `Write` event, the user (or auto-relaunch) loads the partial bytes, qemu-wasm either fails to verify the ELF, hangs at first instruction, or — worst — boots a half-flashed image and produces nonsense serial output that wastes 30 minutes of debugging. Some toolchains use atomic rename (write `kernel.tmp`, rename to `kernel`), which produces Delete+Create events on Linux/macOS and a different sequence on Windows.

**Why it happens:**
The `notify` crate exposes raw FS events. A naive `if event.is_write { reload() }` triggers on every chunk. Editors vary: some truncate-in-place, some write-then-rename, some emit both. mtime races compound this: rapid writes can produce mtime = previous mtime, and you'll miss the change.

**How to avoid:**
- Use `notify-debouncer-full` with a 300–500 ms debounce window. This is the standard fix.
- Don't trust mtime alone — also check file size stabilization (size hasn't changed across two debounce ticks) before declaring the artifact "ready."
- Optionally validate the file is a plausible RISC-V ELF/raw image before allowing Launch (magic-byte sniff). Cheap and prevents the "partial flash → garbage serial" trap.
- Treat the watcher as a *hint*, not a trigger: the watcher updates the UI ("fresher kernel available — click Launch"), and the user/CI explicitly initiates the reload. Implicit auto-relaunch makes the partial-write race much more dangerous.
- Handle the cross-FS case (kernel lives on a mounted dir, NFS, container bind mount): test on at least one bind-mount setup, since inotify behavior across FS boundaries is well-known to be flaky.

**Warning signs:**
- Intermittent "ELF magic mismatch" or qemu-wasm aborts at very early boot — often coincides with build completion timing.
- CI passes locally, fails when `make` and `bootroom run` are issued back-to-back in the same script.
- Tests pass with `sleep 1` inserted before `bootroom run`, fail without it.

**Phase to address:** Early (Phase 2/3 — watcher milestone). The "fresh kernel" UX is in the PROJECT.md core value statement, so this bug breaks the headline feature.

Source: [oneuptime Rust file-watcher debouncing guide](https://oneuptime.com/blog/post/2026-01-25-file-watcher-debouncing-rust/view), [notify-rs repo](https://github.com/notify-rs/notify).

---

### Pitfall 5: Serial-output regex assertions that look right but flake forever

**What goes wrong:**
Scenarios assert on serial output via regex (per PROJECT.md: "asserts on serial output, exits 0/1"). Three failure modes compound:
1. **Partial-line matches:** Output arrives byte-by-byte from the UART model. A regex `^PANIC` may match a `PANIC` that's actually `NOT A PANIC` because only `PANIC` has arrived so far when the test polls.
2. **CR/LF ambiguity:** qemu-wasm's ns16550a emits `\r\n` (UART convention); some kernels emit just `\n`; some emit `\n\r`. Regexes anchored on `\n$` or `^` then fail unpredictably.
3. **Stale buffer matching:** If the assertion buffer isn't cleared between actions, a regex matches text from boot, not from the current step. The Zephyr/expect community calls this out as one of the most common HIL flakiness sources.
4. **ANSI escape sequences:** kernels and especially userspace (systemd, busybox) emit color codes (`\x1b[...m`) and cursor sequences. A regex for `error:` may not match `\x1b[31merror:\x1b[0m`.

**Why it happens:**
Writing assertions feels like writing log regexes — but a live serial stream isn't a log file, it's a stream that the test sees in chunks at unpredictable times.

**How to avoid:**
- **Line-buffer with explicit boundaries:** parse the byte stream into a line list using `\r?\n` as the separator; never match against raw byte buffers. Only match completed lines (or have an explicit "match streaming prefix" mode that's opt-in).
- **Strip ANSI on a per-line basis before matching** (a small VT100 state machine, or a battle-tested crate like `strip-ansi-escapes`). Keep both raw and stripped buffers so the UI can render colors but assertions match plain text.
- **Per-action buffer reset:** each action click/step starts a fresh "since this point" view of the serial stream. Assertions match only against output produced after the action was sent. Make this the default; require opt-in to match cumulative.
- **Use anchored regex with explicit timeouts**, not "wait until you see X" with no upper bound. Default timeout per assertion (say 5s) with a clear failure: "expected /pattern/ within 5s, last 10 lines were: …".
- **Echo + line-ending normalization:** decide one canonical line ending (`\n`) at the harness layer; document it. The TTY layer / xterm.js should render whatever the guest sends, but the assertion layer should normalize before regex.
- **Inter-character delay on send** (already standard in embedded HIL): when injecting input bytes for an action, throttle to ~10–20 ms per byte. UARTs and shell readlines do drop characters under no-flow-control bursts.

**Warning signs:**
- Tests pass under heavy CPU load on a beefy machine, fail on a small CI runner — classic timing flake.
- Adding a `sleep 0.5` between actions "fixes" the test.
- Tests that rerun in a loop fail 1-in-N times for no reason.
- Failures whose error message says "expected X, got '…X'" — the test matched too early.

**Phase to address:** Early to mid (Phase 3/4 — the scenario assertion milestone). Bake the conventions into the scenario engine before users author many scenarios, or you'll break their TOML files later.

Source: [The Good Penguin — 5 Serial Automation Gotchas](https://www.thegoodpenguin.co.uk/blog/5-serial-automation-gotchas/), [Golioth automated hardware testing](https://blog.golioth.io/automated-hardware-testing-using-pytest/), [Reverse to Build — Zephyr HIL CI Pipeline](https://reversetobuild.com/firmware-hil-ci-pipeline/).

---

### Pitfall 6: Headless CI runner missing `crossOriginIsolated` / SharedArrayBuffer

**What goes wrong:**
`bootroom run --scenario` is intended to run in any kernel project's CI. But headless Chromium in CI containers needs *exactly* the right combination: the bootroom server must send COOP/COEP, Chromium must be launched with flags that let it actually become cross-origin-isolated under those headers, and the CI environment must not be running an old Chromium build (< 92 for desktop SAB-by-isolation, with deprecation-trial complications later). On top of that, the GitHub-Actions-style ubuntu runner doesn't ship Chrome by default; users will install whatever they can get.

**Why it happens:**
"It works in my browser" is a lie when the browser is a different version, the headless flags differ, or `--disable-web-security` was set in dev and not in CI. Many guides recommend `--enable-features=SharedArrayBuffer` as a "make it work" flag, which masks missing headers — and silently stops working when Chrome drops the flag (it has been on a deprecation rail).

**How to avoid:**
- Don't depend on Chrome flags as a substitute for correct headers. Headers must be right; flags are a fallback when something else is wrong.
- Pin the Chromium version that `bootroom run` uses for headless mode. Three reasonable options, in order of preference:
  1. Use the user's installed Chrome/Chromium with a documented minimum version, and a startup check that exits with a clear error if it's too old.
  2. Use a `chromiumoxide` / `headless_chrome` / CDP driver against an explicitly downloaded Chromium for Testing, version-pinned.
  3. Embed Chrome download via something like `puppeteer`-equivalent.
- Document the GitHub Actions setup explicitly in the README: `browser-actions/setup-chrome@v1` + the bootroom invocation. Provide a copy-pasteable CI snippet so kernel projects don't reinvent it.
- On startup, the headless driver should `eval()` `crossOriginIsolated` and `typeof SharedArrayBuffer` and refuse to run if either is wrong, with a specific error pointing at this pitfall.
- Avoid `--disable-web-security` in CI mode. It papers over real bugs and changes Chromium behavior in ways unrelated to bootroom.

**Warning signs:**
- CI works on one runner type and fails on another.
- Local CI runs pass; GitHub Actions runs hang at "booting kernel" then time out.
- `bootroom run` succeeds but the serial buffer is empty — wasm never actually started threads.

**Phase to address:** Mid (Phase 4 — headless run milestone). Don't ship `bootroom run` without this guard rail.

Source: [Testing in headless browsers (wasm-bindgen)](https://rustwasm.github.io/docs/wasm-bindgen/wasm-bindgen-test/browsers.html), [Chrome SAB origin-trial extension (deprecation context)](https://developer.chrome.com/blog/shared-array-buffer-origin-trial-extension-124), [browser-actions/setup-chrome](https://github.com/browser-actions/setup-chrome).

---

### Pitfall 7: `cargo install bootroom` succeeds, then crashes at runtime — missing assets

**What goes wrong:**
The README says "install with `cargo install bootroom` or download from Releases." Both should produce the same working binary. The classic failure: assets are embedded via `include_dir!("$CARGO_MANIFEST_DIR/web")`, but the `web/` directory was in `.gitignore` (generated content), so when someone `cargo install`s from crates.io, the published crate has no `web/`, and the binary panics at first request with `web/index.html: file not found` — or worse, serves empty content.

The mirror problem on the release-binary side: building the prebuilt binary on a build host that has glibc 2.38 means it won't run on Ubuntu 20.04 / older CI runners (glibc 2.31). Or building on macOS 15 and the user is on 13. Or the binary works on the developer's Mac but isn't notarized, so end users see "cannot be opened because the developer cannot be verified."

**Why it happens:**
- `cargo publish` only ships what's listed in `Cargo.toml`'s `include` (or all non-gitignored files). Generated assets get skipped silently.
- `cargo build --release` on a modern Linux dynamically links glibc; the binary inherits a minimum glibc version from the build host.
- Ad-hoc macOS releases that just `cargo build --release` and `tar` the output are unsigned and unnotarized.

**How to avoid:**
- **`cargo publish --dry-run`** in CI, then **install from the packaged tarball** and run a smoke test: `bootroom serve --kernel <fixture>` should succeed. Catches the "missing assets" problem before users hit it.
- Set `Cargo.toml`'s `include = ["src/**", "web/**", "Cargo.toml", "README.md", "LICENSE-*"]` (or `package.include`) explicitly. Don't rely on `.gitignore` semantics for packaging.
- Build release binaries on `musl` for Linux: `cargo build --target x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl`. Statically linked — runs on glibc, musl, old, new, Alpine, NixOS.
- For macOS: build both `aarch64-apple-darwin` and `x86_64-apple-darwin`, sign with a Developer ID, and notarize via `notarytool`. If notarization is too heavy for early releases, document the workaround clearly (`xattr -d com.apple.quarantine`) and don't pretend it's a smooth experience.
- Have a release CI workflow that runs the binary on a clean container/VM (one for old Ubuntu, one for Alpine, one for fresh macOS) before declaring the release good.

**Warning signs:**
- Users open issues with "`bootroom serve` fails with file-not-found on a path inside its own assets."
- `version: GLIBC_2.34 not found` from CI runners after a release.
- macOS users report quarantine pop-ups that scare them off.
- "Works when I run from the repo, fails when installed."

**Phase to address:** Late (Phase 5/6 — release/distribution milestone). But the prevention checklist (`include` in Cargo.toml, musl targets, smoke test) should be added the first time release tooling is touched — retrofitting after the first bad release is more expensive than getting it right once.

Source: [Rust 2018 musl docs](https://doc.rust-lang.org/edition-guide/rust-2018/platform-and-target-support/musl-support-for-fully-static-binaries.html), [rust-lang/rust#114796 macOS notarization](https://github.com/rust-lang/rust/issues/114796), [emk/rust-musl-builder](https://github.com/emk/rust-musl-builder).

---

### Pitfall 8: TOML schema drift — scenarios silently break or get accepted with typos

**What goes wrong:**
Action and scenario config evolves. v0.1 has `[actions.send_string]`; v0.2 renames to `[actions.input]`; v0.3 adds `expect = "..."`. Old TOMLs keep "working" (serde with `#[serde(default)]` swallows missing fields), but the scenarios silently do nothing. Or a user types `expects = "..."` (plural) and serde ignores it because the field is `deny_unknown_fields = false` (the serde default). A scenario referencing a button id that doesn't exist (because of a rename) errors at *click time*, not at config load — surfacing only after a CI run, not before.

Bonus failure: serde-toml error messages are notoriously bad. Rust forum thread referenced below: errors collapse onto one line with no line numbers, dumping the TOML content as part of the message.

**Why it happens:**
- `#[serde(default)]` and not setting `deny_unknown_fields` are the path of least resistance and are commonly chosen for "future-compatible" config — but they mask real bugs.
- TOML parser errors propagate as serde errors without source location.
- No `schema_version` field at the top of the config, so there's no way to tell users "your config is v1; current is v2; here's the migration."

**How to avoid:**
- Add a `schema_version = "1"` (or numeric) field at the top of `bootroom.toml` from day one. Validate it; refuse to load with a clear message if it's missing or unknown. Document migration when bumping.
- Use `#[serde(deny_unknown_fields)]` on all config structs. Yes, this means typos error out — that's the point. Misspelled fields are the #1 silent failure source.
- Use `toml_edit` or `toml` v0.8+ which give span/line info on parse errors. Wrap errors to print `bootroom.toml:42:5: unknown field 'expects'; did you mean 'expect'?`.
- **Validate references at load time, not at click time.** Walk all scenarios, verify every action id and group reference resolves. Fail at startup with a list of all problems, not one at a time.
- Add `bootroom check` / `bootroom config validate` subcommand that exits non-zero on any issue. Doc it so users run it in CI / pre-commit.

**Warning signs:**
- Issues like "my scenario passed but the assertions don't seem to fire."
- A user copy-pastes a config from an older version of the docs and gets vague error messages.
- Scenario fails halfway through with "no such button" — but only sometimes, depending on which scenarios run first.

**Phase to address:** Early to mid (Phase 3 — when TOML schema is first defined). Schema version + `deny_unknown_fields` cost nothing on day one; both are painful to add after users have configs in the wild.

Source: [serde-toml error reporting](https://users.rust-lang.org/t/serde-toml-error-reporting/127521), [clap + figment layered config](https://www.hecatron.com/posts/2025/rust-cli-cfg-opts/), [Generating a config reference for a Rust CLI](https://tarquin-the-brave.github.io/blog/posts/generating-config-reference-rust-cli/).

---

### Pitfall 9: Concurrent serial writes from UI and scenario reorder bytes

**What goes wrong:**
A user clicks a button in the browser UI while a scenario step is running. Both end up writing to the guest's UART character-by-character. Without serialization at the harness layer, the wasm-side stdin queue interleaves bytes mid-line: `helo<USER_KEYSTROKE>lo\n` reaches the guest's getty as `heloXlo`, and the kernel's shell parser gets noise. Worse, in CI mode no user is clicking but a watchdog/timeout in one task and a send-input in another can still race if both run on the same channel.

**Why it happens:**
The UI and the scenario engine see the serial channel as fire-and-forget. JavaScript promises and Rust task spawning make it easy to call `send_bytes` from anywhere; without explicit serialization, there is none.

**How to avoid:**
- Funnel every write through a single serialization point — a queue (Tokio `mpsc`, or a `tokio::sync::Mutex` over the write half of the serial channel). Document this is the only correct path.
- During scenario execution, **disable manual input** in the UI (gray out buttons, show a "scenario running" banner) so the human can't race the script. Re-enable on scenario completion or abort.
- Per-action: send the entire byte string as one queued operation, not byte-by-byte from multiple call sites. (Inter-character throttling inside that single operation is fine; what matters is no other writer can interleave.)
- Distinguish "input bytes to guest" from "control to wasm runtime (reset, pause)" — they go through different channels; conflating them is its own bug.

**Warning signs:**
- A kernel's shell complains about garbled commands intermittently in CI, but never when manually reproduced.
- Test logs show the input that was sent and the input the kernel received, and they differ subtly.
- "Worked yesterday" CI failures coinciding with timeout-driven retries.

**Phase to address:** Mid (Phase 3 — action engine milestone). Once multi-step scenarios exist, this lurks; once UI + scenarios coexist, it's guaranteed to surface.

Source: [The Good Penguin — Serial Automation Gotchas](https://www.thegoodpenguin.co.uk/blog/5-serial-automation-gotchas/) (inter-character delay), general queue-serialization is standard concurrency hygiene.

---

### Pitfall 10: `bootroom init` / `bootroom serve` requires repo checkout to find assets

**What goes wrong:**
PROJECT.md is explicit: "the binary, once installed, must run anywhere; no assumption that bootroom's repo is checked out." Easy to violate by accident:
- Loading `bootroom.toml` from a hardcoded relative path that happens to work in the dev repo but not when run from `/some/kernel-project/`.
- Looking for `web/index.html` on disk when assets *should* be embedded.
- A `qemu-wasm` submodule asset referenced by relative path that breaks when the binary is `cargo install`ed and the submodule isn't present.
- Tests that pass because they cd into the repo first; users running `bootroom serve --kernel ./kernel.bin` from a different directory hit "file not found".

**Why it happens:**
Convenience in dev: `std::fs::read("web/index.html")` is one line and works during `cargo run`. The dev never tests `bootroom` from outside its own repo until someone files an issue.

**How to avoid:**
- All qemu-wasm and UI assets must be embedded (`include_dir!` / `rust-embed`); no runtime filesystem reads of bundled assets.
- The default `bootroom.toml` lookup: CWD first, then `XDG_CONFIG_HOME/bootroom/bootroom.toml`, then a built-in default. Document the lookup order. Never look in the binary's installation directory — that breaks under `cargo install` (binary in `~/.cargo/bin`, no adjacent files).
- A `bootroom init` subcommand that **writes a starter `bootroom.toml` into the CWD** so new users get a working config without copying from the repo.
- Have a CI job that does `cargo install --path .` to `/tmp/somewhere`, then runs the binary from a completely unrelated directory against a fixture kernel. If this passes, the external-callable contract holds.

**Warning signs:**
- Issues like "I `cargo install`ed and it crashes on startup."
- Internal tests pass only because the test harness uses `assert_cmd` with `current_dir(repo_root)`.
- README instructions implicitly assume the user has the repo checked out.

**Phase to address:** Continuous, but specifically verified in the release/distribution milestone (late). Add the "external dir" CI job as part of release tooling.

---

## Moderate Pitfalls

### Pitfall 11: Output backlog overflow / xterm.js memory blowup on chatty kernels

A kernel that prints heavy debug output (every interrupt, every page table walk) can produce MB/s of serial text. xterm.js's scrollback default is 1000 lines; the harness's internal buffer (for assertions) may be unbounded. Result: memory creeps, then the tab dies after a long run.

**Prevention:** Cap the assertion buffer at N MB (configurable per scenario, default ~16MB), drop oldest with a warning. Reduce xterm.js scrollback to ~10000 lines or make it a config. Provide a "save full log" option for CI mode that writes to disk instead of holding in memory.

**Phase:** Mid.

---

### Pitfall 12: Button-order non-determinism in the rendered UI

If actions live in a `HashMap` in Rust and get serialized to the UI in iteration order, the buttons reorder on every restart. Users grow distrustful. Also: scenarios reference buttons by id, so reordering doesn't break them, but it ruins screenshots, docs, and muscle memory.

**Prevention:** Use `IndexMap` / `BTreeMap` / preserve TOML declaration order via `toml_edit` for groups. Specify explicit ordering in the schema (`order = 10`) as an override.

**Phase:** Early-mid (Phase 3 — config schema).

---

### Pitfall 13: Recursive vs single-file watch — rebuilding storms

Watching the entire kernel project directory recursively means any source-tree change (editor swap files, `.git/`, `target/` writes during build) fires events. Result: thrashing, lots of false "kernel changed" hints.

**Prevention:** Default watch target = the kernel artifact *file*, not its directory. If a directory must be watched, provide explicit `ignore = ["target/", ".git/", "*.tmp", "*.swp"]` config. Don't watch `target/` ever — that's where the build *is*, and inotify on it is a fire hose.

**Phase:** Mid (Phase 3 — watcher milestone).

---

### Pitfall 14: 4 GB wasm memory ceiling on long runs

WebAssembly (32-bit, Wasm 1.0) caps linear memory at 4 GB. qemu-wasm uses linear memory for guest RAM + QEMU heap. A scenario that allocates a large `-m` value or runs long enough to grow fragmentation can OOM the wasm instance with an opaque error. Wasm64 mitigates this but isn't universally available.

**Prevention:** Cap `-m` in the harness's QEMU command (e.g., default 256 MB, configurable up to ~1.5 GB). Detect OOM-style errors and surface them with a "guest exceeded available wasm memory" message, not a generic "wasm aborted."

**Phase:** Mid (Phase 2 — first kernel boot).

Source: [WebAssembly memory limits / Tommie's blog](https://tommie.github.io/a/2024/06/webassembly-memory).

---

### Pitfall 15: Non-Chromium browsers — Firefox / Safari quirks

The PROJECT.md doesn't promise multi-browser support, but contributors will try. SharedArrayBuffer + threads work in Firefox 79+ and Safari 14.1+/14.5+ (desktop/iOS), but with quirks: Safari's COEP enforcement has historically been stricter, and `WebAssembly.Module` instantiation behavior differs slightly. wasm-bindgen-test's headless support is primarily Chrome.

**Prevention:** Be explicit in docs: "supported browser for `bootroom serve`: Chromium 92+, Firefox 79+; CI mode uses headless Chromium." Don't claim more than is tested.

**Phase:** Continuous; documented at Phase 1.

Source: [WebAssembly threads / Apryse FAQ](https://docs.apryse.com/web/faq/wasm-threads), [TestMu WASM threads browser support](https://www.testmuai.com/learning-hub/wasm-threads-browser-support/).

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| `--disable-web-security` / `--enable-features=SharedArrayBuffer` flags instead of proper COOP/COEP headers | "It just works" in dev | Bug doesn't reproduce in CI / on user machines; flag may be removed by Chrome | Never. Fix the headers. |
| `include_dir!` only (no filesystem fallback) | Single static binary, simple build | Every UI tweak requires `cargo build`; contributors disengage from UI work | Acceptable only after Phase 6 if UI is fully stable and rarely changes. Add `--assets-dir` override before then. |
| `#[serde(default)]` everywhere without `deny_unknown_fields` | Forward-compat without thought | Typos silently ignored; broken configs go undetected; users lose hours | Never. `deny_unknown_fields` from day one. |
| Raw `notify` events without debouncing | Saves a dependency | Partial-write boots, mass-rebuild storms, mtime races | Never — `notify-debouncer-full` exists and is small. |
| Single binary built on dev's Linux without musl/Alpine test | Faster release tooling | `GLIBC_2.34 not found` on user CI | Acceptable in pre-1.0 releases with prominent "build from source if your distro is old" doc. |
| Hardcoded QEMU command line as a single string instead of a typed builder | Less Rust code to write | Adding/changing devices means string-bashing; testing the command line is painful | Acceptable in Phase 2 prototype; refactor to a typed builder by Phase 3 / 4. |
| Auto-relaunch on file-watch event | "Magical" UX | Boots half-flashed kernels, wastes debugging hours | Never default. Optional with explicit opt-in via config. |
| Unbounded assertion buffer | Simple memory model | Long CI runs OOM the harness | Never in CI mode; acceptable in headed dev mode with default ceiling. |

---

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| qemu-wasm submodule | Building it as part of `cargo build` every time | Pre-built wasm/JS artifacts checked into a `vendored/` dir or downloaded once via a build script; submodule provides source for upgrades, not per-build inputs. |
| Browser → wasm guest stdin | Writing characters as fast as possible | Throttle ~10–20 ms/char; serialize via a single mpsc; let the guest UART drain. |
| Kernel build (NORN consumer) → `bootroom run` | `make && bootroom run` in CI without artifact-stability check | Build target writes to a temp path, atomic-renames to final path; bootroom validates ELF magic before booting. |
| GitHub Actions CI | Letting Actions install whatever Chromium is "current" | Pin via `browser-actions/setup-chrome@v1` with an explicit version; record minimum version in `bootroom`'s startup check. |
| TOML config + CLI overrides | Two precedence orders depending on subcommand | One documented precedence: `CLI > env (BOOTROOM_*) > project bootroom.toml > defaults`. Use figment or hand-rolled but consistent. |
| qemu-wasm Fetch-based "disk" | Using `file://` URLs in scenarios | All disk/asset URLs must be HTTP(S) and served by the bootroom server; `file://` won't work and produces confusing errors. |

---

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| Compiling too many TBs into individual wasm modules | Page slows down then tab tab dies; OOM | Stick with qemu-wasm's default TCI-first + selective wasm JIT; don't try to force everything to wasm | Long-running scenarios or kernels with huge code coverage |
| Unbounded scrollback / log buffer | Tab memory climbs over a run | Cap scrollback at ~10k lines; cap assertion buffer at MB-size with eviction | Multi-minute scenarios producing chatty serial output |
| File-watcher running on `target/` | CPU pegged, "kernel changed" hint fires constantly | Default watch target is the artifact file, not its parent dir; ignore `target/`, `.git/` always | Any real workflow within 1 minute |
| Synchronously embedding very large assets via `include_dir!` | Slow `cargo build`, huge RAM use during compilation, binary bloat | Keep embedded assets small; large fixtures (kernel images, disk images) load from filesystem at runtime via flag/config | Crate size > ~20 MB or build time noticeably regresses |
| Per-byte send with no batching | Long input sequences (paste-a-config) take seconds | Batch into one queued op with internal throttling; visible progress in UI for long inputs | Input strings > a few hundred bytes |
| Spawning a new headless Chromium per scenario in CI | Adds 2–5 s startup × N scenarios | Run a scenario suite in one browser session when possible; provide isolation between scenarios at the harness level (reset wasm instance, not the browser) | A test suite with > 10 scenarios |

---

## Security Mistakes

| Mistake | Risk | Prevention |
|---------|------|------------|
| Binding the dev server to `0.0.0.0` by default | Anyone on the LAN can drive your kernel/UART; over Wi-Fi this is a real exposure | Bind to `127.0.0.1` by default; opt-in `--host 0.0.0.0` with a startup warning. |
| Serving the kernel binary at a predictable URL with no auth | Curious LAN visitor can download your in-progress kernel | Document that bootroom is **local-only** (already in PROJECT.md "Out of Scope: Authentication"). Reinforce by default-binding to localhost. |
| Allowing scenarios to read arbitrary files via TOML path fields | A malicious `bootroom.toml` (pulled into a CI job from a PR) reads `~/.ssh/id_rsa` | Constrain readable paths to the project dir / explicit allow-list; reject `..` in scenario file refs. |
| Trusting an embedded CDN script (xterm.js, etc.) | Polyfill.io 2024 supply-chain attack precedent: a sold/compromised CDN injects malware | Self-host all JS; no `<script src="https://cdn...">`. Vendor and pin. |
| Auto-loading any kernel mtime change without confirmation in a shared session | A teammate's build event hijacks your debug session | Use the watcher as a hint, not an action; explicit user click to launch. |

Source: [Go Make Things — vanilla JS, polyfill.io warning](https://gomakethings.com/the-easy-way-to-support-almost-every-browser-with-vanilla-javascript/).

---

## UX Pitfalls

| Pitfall | User Impact | Better Approach |
|---------|-------------|-----------------|
| No visible boot/loading state — page just sits while wasm instantiates | User reloads, thinks it's broken | Show explicit phases: "loading wasm module" → "instantiating" → "booting kernel" → "guest running." Each with a spinner and elapsed time. |
| Error messages from misconfigured COOP/COEP are cryptic browser-console messages | User has no idea why nothing works | The page itself does a `crossOriginIsolated`/`SharedArrayBuffer` probe and renders a banner with the actual fix ("Your COOP/COEP headers are missing — see bootroom docs section X"). |
| Cryptic TOML parse errors | User edits config, gets a serde error dump, gives up | Wrap config errors with line/column from `toml`'s span info; suggest the nearest valid field on typos. |
| Buttons rearrange between runs | Muscle memory dies | Stable declaration order via `IndexMap` + optional explicit `order =`. |
| Watcher fires constantly — UI flashes "fresh kernel available" forever | User stops trusting the indicator | Debounce hint + only show when artifact actually changed (size + mtime stable). |
| Scenario fails with "regex didn't match" — no context | User stares at the output trying to figure out what was expected | On failure, print: regex, the last N lines of serial, where in the scenario it was, and how long it waited. |
| `bootroom run` outputs nothing until exit | CI users have no idea what's happening | Stream a one-line summary per action ("→ action `boot_login`: PASS in 1.2s"); include `--verbose` for full serial. |
| Manual buttons stay clickable during a scenario run | Race conditions, user confusion | Disable manual input while a scenario runs; show "scenario in progress" state. |
| No `bootroom doctor` / `bootroom version --verbose` | When something goes wrong, user can't gather info to file a bug | Provide a doctor command: prints version, qemu-wasm submodule rev, browser detected, header check, etc. |

---

## "Looks Done But Isn't" Checklist

- [ ] **COOP/COEP headers:** Server sends them on the HTML page — also verify on every wasm/JS/worker subresource (some configurations only cover the HTML).
- [ ] **`bootroom serve` works from any CWD:** Test from `/tmp/empty` with `--kernel /abs/path.bin`; lots of dev test only inside the repo.
- [ ] **`cargo install bootroom` works in a clean container:** Bring up a fresh Ubuntu/Alpine, install, run against a fixture kernel. Embedded assets must travel; binary must run.
- [ ] **`bootroom run --scenario X` exits non-zero on failure:** Verify exit code; many test harnesses accidentally exit 0 when a step fails because the wrapping logic swallows errors.
- [ ] **Headless CI run on a stock GitHub Actions runner:** Run the CI scenario in a minimal Action; many "works locally" runs fail under restricted CI Chromium.
- [ ] **File watcher does not auto-relaunch:** Verify it surfaces a hint only; auto-relaunch must require explicit opt-in.
- [ ] **Unknown TOML fields fail loudly:** Add a `[actions.foo] expects = "..."` typo; loader must error with line/column.
- [ ] **Scenario assertions strip ANSI before matching:** Add an action that triggers a colorized kernel oops; assertion for `panic` must still match.
- [ ] **Two scenarios in sequence get isolated serial buffers:** Run scenario A then scenario B; B's assertions must not see A's residual output.
- [ ] **Partial kernel writes don't boot a garbage image:** Atomic-rename a kernel artifact; verify either a clean boot or a clean "invalid ELF" rejection, never a mid-flash boot.
- [ ] **`bootroom doctor`:** Reports header check, browser detected, version of qemu-wasm submodule, missing prerequisites.
- [ ] **`bootroom init` produces a TOML that loads without errors:** Frequent regression target; lock it down with a smoke test.

---

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| Missing COOP/COEP discovered after release | LOW | Patch headers, release a minor version, document the fix. No user data loss. |
| qemu-wasm flag drift broke a scenario | MEDIUM | Diff supported-flags doc, mark scenario as needing migration, ship a `bootroom migrate` helper or a clear error pointing to the doc. |
| Embedded asset missing in a published crate | LOW | Yank the affected crates.io version, fix `Cargo.toml` `include`, republish; users `cargo install --force`. |
| glibc-too-new binary in a release | LOW | Add musl target to release pipeline, re-cut the binary; old release still exists for newer distros. |
| TOML schema breaking change shipped without `schema_version` | MEDIUM | Introduce `schema_version`, treat absent = "v0", auto-migrate to v1 on next save (carefully) or print explicit migration steps. |
| Flaky scenario from regex race | MEDIUM | Switch matching to line-buffered + ANSI-stripped; bump per-action buffer reset to default. Re-run scenario suite to flush stale flakes. |
| File-watcher boots partial kernel image | LOW per occurrence, MEDIUM cumulative | Add ELF magic check + size-stable debounce; backfill with explicit user-initiated launch only. |
| Concurrent UART writes garbled input | HIGH (debugging time) | Funnel all writes through a Mutex/mpsc; disable manual input during scenarios. Audit existing call sites. |
| Headless CI fails in users' environments | MEDIUM | Document supported headless setup with version pin; add startup check that errors with specific guidance. |

---

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| 1. Missing COOP/COEP | Phase 1 (first serve) | Server smoke test asserts headers; browser-side `crossOriginIsolated` probe in UI |
| 2. qemu-wasm flag drift | Phase 2 (first boot) | Doc'd supported-flag matrix; `bootroom doctor` lists them |
| 3. Embedded-assets dev workflow | Phase 1 (asset embedding) | `cargo run` UI edits live-reload via `--assets-dir` or rust-embed debug fallback |
| 4. File-watcher partial writes | Phase 2/3 (watcher) | ELF magic check + debounce; integration test mid-write |
| 5. Serial-output regex flakes | Phase 3/4 (assertions) | Line-buffer + ANSI strip + per-action reset all default; soak test 100× |
| 6. Headless CI missing SAB | Phase 4 (CI mode) | Stock GH Actions run on minimal runner passes |
| 7. `cargo install` runtime failures | Phase 5/6 (release) | Clean-container install test in release CI |
| 8. TOML schema drift | Phase 3 (config) | `schema_version` + `deny_unknown_fields` + `bootroom check` from day one |
| 9. Concurrent serial writes | Phase 3 (actions) | Single write funnel; UI disables manual input during scenarios |
| 10. Repo-checkout assumption | Phase 5/6 (release) | "External directory" CI job |
| 11. Output-backlog overflow | Phase 3/4 | Bounded buffer with eviction; long-run soak test |
| 12. Button-order non-determinism | Phase 3 (config) | `IndexMap` + visual diff test |
| 13. Watcher rebuild storms | Phase 3 (watcher) | Watch the artifact file, not its dir; ignore patterns |
| 14. 4 GB wasm memory ceiling | Phase 2 (first boot) | Default `-m` cap; OOM detection with clear error |
| 15. Non-Chromium quirks | Continuous; doc at Phase 1 | Explicit supported-browser matrix in README |

---

## Sources

- [web.dev — Making your website cross-origin isolated using COOP and COEP](https://web.dev/articles/coop-coep)
- [Cinevva — Enable Wasm threads with COOP/COEP](https://app.cinevva.com/tutorials/coop-coep-sharedarraybuffer.html)
- [TestMu AI — WASM Threads: Browser Support, Atomics, COOP/COEP](https://www.testmuai.com/learning-hub/wasm-threads-browser-support/)
- [ktock/qemu-wasm — QEMU on browser](https://github.com/ktock/qemu-wasm)
- [FOSDEM 2025 — Running QEMU Inside Browser (slides)](https://archive.fosdem.org/2025/events/attachments/fosdem-2025-6290-running-qemu-inside-browser/slides/238760/slides_1dDtpcS.pdf)
- [QEMU patch series — Enable QEMU to run on browsers](https://lists.gnu.org/archive/html/qemu-arm/2025-04/msg00153.html)
- [QEMU docs — RISC-V virt machine](https://www.qemu.org/docs/master/system/riscv/virt.html)
- [The Good Penguin — 5 Serial Automation Gotchas](https://www.thegoodpenguin.co.uk/blog/5-serial-automation-gotchas/)
- [Reverse to Build — Zephyr HIL CI Pipeline](https://reversetobuild.com/firmware-hil-ci-pipeline/)
- [Golioth — Automated hardware testing using pytest](https://blog.golioth.io/automated-hardware-testing-using-pytest/)
- [notify-rs](https://github.com/notify-rs/notify) and [notify-debouncer-full docs](https://docs.rs/notify-debouncer-full/latest/notify_debouncer_full/)
- [oneuptime — Build a File Watcher with Debouncing in Rust](https://oneuptime.com/blog/post/2026-01-25-file-watcher-debouncing-rust/view)
- [Rust musl static binaries](https://doc.rust-lang.org/edition-guide/rust-2018/platform-and-target-support/musl-support-for-fully-static-binaries.html)
- [rust-lang/rust#114796 — macOS notarization](https://github.com/rust-lang/rust/issues/114796)
- [emk/rust-musl-builder](https://github.com/emk/rust-musl-builder)
- [Apryse — WebAssembly Threads FAQ](https://docs.apryse.com/web/faq/wasm-threads)
- [Chrome — SharedArrayBuffer origin trial extension (deprecation context)](https://developer.chrome.com/blog/shared-array-buffer-origin-trial-extension-124)
- [browser-actions/setup-chrome](https://github.com/browser-actions/setup-chrome)
- [Testing in headless browsers (wasm-bindgen guide)](https://rustwasm.github.io/docs/wasm-bindgen/wasm-bindgen-test/browsers.html)
- [rust-embed docs](https://docs.rs/rust-embed/latest/rust_embed/trait.RustEmbed.html)
- [include_dir docs](https://docs.rs/include_dir/latest/include_dir/)
- [Rust users forum — Serde / TOML error reporting](https://users.rust-lang.org/t/serde-toml-error-reporting/127521)
- [Hecatronic — Clap + Figment Rust CLI configuration](https://www.hecatron.com/posts/2025/rust-cli-cfg-opts/)
- [Tarquin the brave — Generating a Config File Reference for a Rust CLI](https://tarquin-the-brave.github.io/blog/posts/generating-config-reference-rust-cli/)
- [Go Make Things — vanilla JS browser support (incl. polyfill.io supply-chain warning)](https://gomakethings.com/the-easy-way-to-support-almost-every-browser-with-vanilla-javascript/)
- [Tommie's blog — WebAssembly memory limits](https://tommie.github.io/a/2024/06/webassembly-memory)
- [xterm.js — Parser hooks & terminal sequences](https://xtermjs.org/docs/guides/hooks/)

---
*Pitfalls research for: Rust CLI + qemu-wasm browser kernel test harness (bootroom)*
*Researched: 2026-05-17*
