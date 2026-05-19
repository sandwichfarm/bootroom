---
phase: 06-distribution
plan: 01
subsystem: distribution
tags: [licensing, spdx, dual-license, metadata, publish-prep]
dependency_graph:
  requires: []
  provides:
    - "Verbatim SPDX MIT and Apache-2.0 license text at repo root"
    - "cargo metadata reports `license: MIT OR Apache-2.0` for both bootroom and bootroom-core"
    - "README dual-license badge above the fold"
  affects:
    - "06-02 (cargo package metadata + include allow-list will pull LICENSE-MIT and LICENSE-APACHE into the publish tarball)"
    - "06-05 (cargo-deny licenses check reads the resolved SPDX expression from cargo metadata)"
tech_stack:
  added: []
  patterns:
    - "Workspace inheritance: `[workspace.package].license = \"MIT OR Apache-2.0\"` + `license.workspace = true` in each crate"
    - "Verbatim SPDX templates at repo root; no `license-file` keys; no top-level `LICENSE` ambiguity file"
    - "shields.io static badge (encoded license string, no telemetry)"
key_files:
  created: []
  modified:
    - "LICENSE-APACHE — restored APPENDIX template's copyright line to canonical apache.org placeholder"
    - "README.md — inserted dual-license shields.io badge below the H1 tagline"
decisions:
  - "Apache-2.0 APPENDIX shipped with placeholder text (`Copyright [yyyy] [name of copyright owner]`) rather than filled-in copyright, per SPDX/apache.org canonical verbatim template. Per-file copyright statements live in source-file headers, not the LICENSE file itself."
  - "No top-level `LICENSE` (singular) file — dual-licensing means BOTH licenses apply at licensee's choice; a single `LICENSE` would imply one canonical license."
  - "No `license-file` keys in per-crate manifests; would defeat SPDX expression resolution."
  - "shields.io static badge over a dynamic build/version/docs row — those belong to 06-07's install-matrix scope and keep this plan minimal."
metrics:
  duration_seconds: 171
  duration_human: "2 min 51 sec"
  tasks_completed: "3 / 3"
  files_modified: 2
  commits: 2
  completed_date: "2026-05-19"
---

# Phase 6 Plan 1: License Files & SPDX Posture Summary

Audited and finalized the dual MIT OR Apache-2.0 license surface — verbatim SPDX templates at repo root, workspace-inherited `license` key resolving end-to-end via `cargo metadata`, and a visible README badge — unblocking 06-02's publish-tarball include list and 06-05's cargo-deny licenses gate.

## What was done

### Task 1: LICENSE-MIT and LICENSE-APACHE audit

- **`LICENSE-MIT`**: already verbatim SPDX-canonical (3 paragraphs: grant, notice, disclaimer; correct "MIT License" header; correct copyright line `Copyright (c) 2026 sandwich <sandwich.farm@protonmail.com>`). **No changes.**
- **`LICENSE-APACHE`**: header (`Apache License` / `Version 2.0, January 2004`), 200-line body, and `APPENDIX: How to apply the Apache License to your work.` block were all canonical, but the appendix's instructional template (`Copyright [yyyy] [name of copyright owner]`) had been replaced with a filled-in copyright (`Copyright 2026 sandwich <sandwich.farm@protonmail.com>`). Per the plan's explicit "Apache-2.0 distribution-style is to ship the unmodified license text; copyright statements live in source-file headers, not the LICENSE-APACHE file itself" instruction, restored the placeholder text to match apache.org/licenses/LICENSE-2.0.txt verbatim. **One-line change.**
- Confirmed no `LICENSE` (singular) or `LICENSE.md` file at the repo root.

### Task 2: Per-crate manifest license resolution

Pure verification pass — no file changes needed. Confirmed:

- Root `Cargo.toml` `[workspace.package].license = "MIT OR Apache-2.0"` is present with exact SPDX expression (uppercase `OR`, single space).
- `crates/bootroom/Cargo.toml` has `license.workspace = true`, `repository.workspace = true`, `authors.workspace = true`, and a non-empty `description`.
- `crates/bootroom-core/Cargo.toml` has the same shape.
- No `license-file` keys anywhere.
- `cargo metadata --format-version=1 --no-deps --offline` resolves both packages to `MIT OR Apache-2.0`:

  ```
  bootroom-core: license=MIT OR Apache-2.0
  bootroom: license=MIT OR Apache-2.0
  ```

  (The workspace also contains `spike-b` under `crates/bootroom/spikes/`; it inherits the workspace license too — a free side benefit of the workspace-inheritance pattern.)

No commit needed for Task 2 — verification only, no diff.

### Task 3: README license badge

Inserted a single line between the tagline block and the development-host note:

```diff
 click-to-trigger scenario library.

+[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
+
 > Note: the working directory on this development host is still
```

The badge:

- Uses Markdown image syntax (not raw HTML), as the plan requires.
- Anchor `#license` links to the existing bottom-of-file `## License` section, which was preserved verbatim.
- Uses shields.io's static-badge endpoint (encoded license string only, no dynamic build/version data).

## Verification

All gates from `<verification>` pass:

```
=== cargo metadata ===
bootroom-core: license=MIT OR Apache-2.0
bootroom: license=MIT OR Apache-2.0

=== LICENSE-MIT head ===
MIT License

=== LICENSE-APACHE head ===
                                 Apache License

=== APPENDIX present === OK
=== MIT permission present === OK
=== no license-file keys === OK
=== no top-level LICENSE/LICENSE.md === OK
=== README badge === OK
```

`cargo build --workspace --offline` succeeded — no Cargo.toml syntax regression.

## Commits

| Task | Hash | Message |
|------|------|---------|
| 1 | `e22e776` | `docs(06-01): restore SPDX-canonical Apache-2.0 APPENDIX template` |
| 3 | `6a6ab73` | `docs(06-01): add MIT OR Apache-2.0 license badge to README` |

Task 2 was a pure verification pass — no diff, no commit.

## Deviations from Plan

None. Plan executed exactly as written. The Apache-2.0 placeholder restoration was explicitly scoped in Task 1 ("If a copyright line was added, remove it").

## Known Stubs

None. All work is metadata + docs; no runtime code paths affected.

## Success Criteria

DIST-07 is fully satisfied:

- ✅ Workspace publishes under SPDX expression `MIT OR Apache-2.0`.
- ✅ Both license files are verbatim SPDX-canonical text at the repo root.
- ✅ Both crate manifests resolve the workspace `license` key (`cargo metadata` confirms).
- ✅ README surfaces the dual-license posture above the fold.
- ✅ Ready for 06-02's `[package].include` allow-list and 06-05's `cargo deny check licenses` enforcement.

## Self-Check: PASSED

- File `LICENSE-APACHE` exists and contains canonical placeholder line: FOUND
- File `README.md` exists and contains shields.io badge: FOUND
- Commit `e22e776`: FOUND
- Commit `6a6ab73`: FOUND
