---
phase: 3
name: Config, Buttons, Watcher
date: 2026-05-19
mode: discuss
---

# Phase 3 Discussion Log

## Areas Presented

1. TOML schema + bootroom-core type model
2. Action invocation flow + funnel routing
3. Config live-reload + watcher
4. bootroom check + init + scenario WS protocol

User selected: all four.

## Area 1 — TOML schema

Selected: **Flat `[[action]]` arrays + group field, types in bootroom-core**. Easiest to author, insertion order preserved, types reusable in Phase 4 headless. `schema_version = 1` required at top level. `deny_unknown_fields` everywhere.

## Area 2 — Action invocation + CLI

Selected: **Direct client funnel + escape-sequence CLI**. Click → funnel.enqueue. Same single-writer path as user typing (post-Phase-2 fix). CLI `--action 'label=BYTES'` supports `\r \n \t \0 \\` and `\xHH` hex.

## Area 3 — Watcher

Selected: **One notify-debouncer pool, two watches, WS broadcast frames**. KernelChanged + ConfigUpdate + ConfigInvalid variants on WsMessage. Non-intrusive banner for kernel; in-place re-render for config; red banner for invalid TOML.

## Area 4 — Check/init/scenarios

Selected: **Phase 3 lands schema + check + minimal init; defer scenario WS additions to Phase 4**. `bootroom check` cross-validates and exits 0/1/2/3. `bootroom init` writes a 25-line commented example.

## Claude's Discretion

- ROADMAP success criterion 2 wording "via slave.write" is superseded by Phase 2's CR-01 fix; SUMMARY will note.
- Funnel lockInput/unlockInput primitive lands here even though Phase 3 has no caller — Phase 4 needs it without re-architecture.
- Exit codes for check (0/1/2/3) chosen for CI ergonomics; documented in --help.
- Banner placement (between header and terminal) and one-banner-at-a-time priority (iso > config-invalid > kernel-fresh) — UI-SPEC will detail.

## Deferred Ideas

(See CONTEXT.md `<deferred>` section.)
