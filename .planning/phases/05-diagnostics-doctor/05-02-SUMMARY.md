---
phase: 05-diagnostics-doctor
plan: 02
subsystem: diagnostics
tags: [makefile, embed, qemu-wasm, doctor, build]
requires: []
provides:
  - "Sentinel `crates/bootroom/assets/qemu/qemu-wasm-rev.txt` available to `include_dir!` at compile time"
  - "Makefile recipe that writes the qemu-wasm submodule short SHA into the same file on every `make qemu-assets` run"
  - "`clean-qemu-assets` also removes the rev file"
affects:
  - crates/bootroom/assets/qemu/
  - Makefile
tech-stack:
  added: []
  patterns:
    - "On-disk sentinel pattern (per Research Pitfall 2) — file always present, contents degrade gracefully to `unknown`"
key-files:
  created:
    - crates/bootroom/assets/qemu/qemu-wasm-rev.txt
  modified:
    - Makefile
decisions:
  - "Sentinel literal is `unknown\\n` (8 bytes) — matches D-DOC-05 degraded display string and the existing build.rs `unknown` SHA fallback"
  - "Step 5b lives AFTER the asset copy / `docker cp` steps so a partial failure leaves the prior rev consistent with the still-present prior artifacts (T-05-02-04 mitigation)"
  - "Rev file is NOT added to `build.rs` REQUIRED list — its absence must remain non-fatal across branches (Pitfall 3)"
  - "Used `@git -C $(QEMU_WASM_DIR) rev-parse --short HEAD > …` — works in detached-HEAD and any branch state"
metrics:
  duration_seconds: 145
  completed_date: 2026-05-19
  tasks_completed: 2
  files_created: 1
  files_modified: 1
---

# Phase 05 Plan 02: qemu-wasm-rev Sentinel + Makefile Hook Summary

Adds a committed `qemu-wasm-rev.txt` sentinel under `crates/bootroom/assets/qemu/` (initial value `unknown\n`, 8 bytes) and extends `make qemu-assets` to overwrite that file with `git -C qemu-wasm rev-parse --short HEAD` on every rebuild, giving the upcoming `bootroom doctor` (Plan 05-04) a stable on-disk source for the qemu-wasm rev without requiring a fresh 10–30-minute docker build.

## Tasks Completed

| Task | Name                                                                         | Commit  | Files                                              |
| ---- | ---------------------------------------------------------------------------- | ------- | -------------------------------------------------- |
| 1    | Commit sentinel `qemu-wasm-rev.txt` so `include_dir!` has something to embed | 995a888 | `crates/bootroom/assets/qemu/qemu-wasm-rev.txt`    |
| 2    | Extend Makefile `qemu-assets` recipe + update `clean-qemu-assets`            | 4adffc2 | `Makefile`                                         |

## Makefile Changes — Exact Line Numbers

The new step lives between the existing `docker cp` artifact copy chain and the final user-facing summary line, as required by the plan.

- `Makefile:13` — `QEMU_OUT_DIR := crates/bootroom/assets/qemu` (unchanged; resolves to the same directory `include_dir!` ingests).
- `Makefile:67` — `@echo ">>> Step 5b/5: Recording qemu-wasm git rev..."`
- `Makefile:68` — `@git -C $(QEMU_WASM_DIR) rev-parse --short HEAD > $(QEMU_OUT_DIR)/qemu-wasm-rev.txt`
- `Makefile:69` — `@echo "  qemu-wasm rev: $$(cat $(QEMU_OUT_DIR)/qemu-wasm-rev.txt)"`
- `Makefile:80` — added `$(QEMU_OUT_DIR)/qemu-wasm-rev.txt` to the `clean-qemu-assets` rm list (still a single `rm -f`, no `-rf` per AGENTS.md system safety).

Step 5b runs AFTER `docker rm -f $(QEMU_BUILDER)` and BEFORE the closing `>>> Done.` echo, so it's the last action in the recipe that touches the output directory. This ordering means if any earlier `docker cp` fails, the rev file is left untouched and continues to reflect whatever rev produced the still-present artifacts (T-05-02-04 mitigation).

## QEMU_OUT_DIR Resolution

`QEMU_OUT_DIR` resolves to `crates/bootroom/assets/qemu` (Makefile line 13). Plan 05-04 should reference this same path when implementing the `qemu_wasm_rev` doctor check; the embedded lookup is `embed::QEMU.get_file("qemu-wasm-rev.txt").and_then(|f| f.contents_utf8()).unwrap_or("unknown")` with a `.trim()` on the value before display.

## Sentinel Bytes — Verified

```
$ od -c crates/bootroom/assets/qemu/qemu-wasm-rev.txt
0000000   u   n   k   n   o   w   n  \n
0000010
$ wc -c < crates/bootroom/assets/qemu/qemu-wasm-rev.txt
8
```

Exactly 8 bytes: `u n k n o w n \n`. No BOM, no leading whitespace, no quotes.

## build.rs — Confirmed Unchanged

`grep -v '^#' crates/bootroom/build.rs | grep -c 'qemu-wasm-rev.txt'` returns `0`. The rev file is intentionally NOT added to the build.rs REQUIRED list so its absence is non-fatal on branches that lack it (Research Pitfall 3). `include_dir!` ingests every regular file under `assets/qemu/` automatically — no `embed.rs` change was needed.

## Verification Results

- `cargo build -p bootroom` — succeeds (finished in ~52s on a cold build), confirms `include_dir!` happily captures the new file.
- `cargo test --package bootroom --lib --no-run` — compiles cleanly, no regressions.
- `make -n qemu-assets` — parses successfully and the dry-run output includes:
  - `git -C qemu-wasm rev-parse --short HEAD > crates/bootroom/assets/qemu/qemu-wasm-rev.txt`
  - `echo "  qemu-wasm rev: $(cat crates/bootroom/assets/qemu/qemu-wasm-rev.txt)"`
- `grep -v '^#' Makefile | grep -c 'qemu-wasm-rev.txt'` returns `3` (≥ 2 as required; counts the step's two lines plus the `clean-qemu-assets` entry).
- `make help` still renders cleanly (no syntax breakage from the help text edits; existing text was left as-is — help-text update was OPTIONAL per the plan and not required for correctness).

## Success Criteria

- [x] `crates/bootroom/assets/qemu/qemu-wasm-rev.txt` exists, is 8 bytes (`unknown\n`).
- [x] `Makefile` references the file in two+ places (write + clean — actually 3 with the operator-confirmation echo).
- [x] `make -n qemu-assets` shows the new `rev-parse` step.
- [x] `build.rs` REQUIRED list is unchanged (Pitfall 3 — file absence remains non-fatal).
- [x] No new dependencies introduced.

## Deviations from Plan

None — plan executed exactly as written. The "Step 5b/5" phrasing was used verbatim from the plan's example.

## Threat Flags

None — no new security-relevant surface introduced. The new write surface is the qemu-wasm submodule's already-public SHA; threat register dispositions T-05-02-01 through T-05-02-04 are all in the plan and were honored.

## Self-Check: PASSED

- `crates/bootroom/assets/qemu/qemu-wasm-rev.txt` — FOUND (8 bytes, `unknown\n`)
- `Makefile` — modified, 3 references to `qemu-wasm-rev.txt`
- Commit `995a888` — FOUND in git log
- Commit `4adffc2` — FOUND in git log
