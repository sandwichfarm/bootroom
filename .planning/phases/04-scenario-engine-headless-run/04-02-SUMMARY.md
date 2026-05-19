---
phase: 04-scenario-engine-headless-run
plan: 02
subsystem: config
tags: [regex, assertions, scenarios, toml, validation, load-error, bootroom-core]

# Dependency graph
requires:
  - phase: 03-config-buttons-watcher
    provides: "LoadedConfig::from_config pipeline; Assertion / AssertionKind / Scenario types; LoadError span-aware reporting"
  - phase: 04-scenario-engine-headless-run
    provides: "04-01: ScenarioStart/Abort/Result WsMessage variants (this plan is independent at the type layer but lives in the same phase)"
provides:
  - "Load-time regex compile-check for every kind = \"regex\" Assertion.pattern"
  - "Load-time after-resolution check: every Assertion.after must equal \"any\" OR appear in the containing Scenario.actions Vec"
  - "Two new LoadError predicates: is_invalid_regex() and is_unresolvable_after()"
  - "Inline documentation of the supported Rust regex ∩ ECMAScript RegExp feature subset (no backrefs, no lookaround)"
  - "regex crate promoted from transitive to direct bootroom-core dependency"
affects:
  - "04-08 (browser scenario engine — can rely on the load-time compile-check having already passed before constructing JS RegExp)"
  - "04-07 (bootroom check CLI — surfaces these new diagnostics)"
  - "Any future plan that adds Assertion kinds; both checks compose alongside the existing scenario cross-validation loop"

# Tech tracking
tech-stack:
  added: [regex (1.x, MIT/Apache-2.0)]
  patterns:
    - "Load-time validation pass composes after scenario cross-validation; same per-scenario / per-assertion shape as existing checks"
    - "Stricter-engine-preflight: Rust regex (no backref/lookaround) gates JS RegExp at runtime so the browser never sees a pattern the Rust validator rejected"
    - "Span-less LoadError for cross-validation: line/col remain None for these failures (matches the existing DuplicateAction / UnknownActionRef pattern)"

key-files:
  modified:
    - "Cargo.toml (workspace) — added regex = \"1\""
    - "crates/bootroom-core/Cargo.toml — added regex.workspace = true"
    - "crates/bootroom-core/src/config.rs — load-time validation pass + LoadError variants/predicates + 9 new tests + Phase-4 module doc block"

key-decisions:
  - "Used the canonical Rust regex ∩ ECMAScript RegExp intersection as the supported subset — Rust is the stricter engine, so the load-time check guarantees the browser engine can later compile the same pattern"
  - "Added a positive AND a negative case for both 'after' typo and 'after-not-in-scenario' (4 after-resolution tests instead of the 3 the plan summary mentioned — the action steps spelled out 4 cases)"
  - "Took regex::Error by reference in LoadError::invalid_regex to satisfy clippy::needless_pass_by_value (pedantic) without weakening the API"

patterns-established:
  - "Sorted-list diagnostic for unresolvable references — sort the legal-values list inside the error constructor so error output is stable across runs (useful for snapshot testing in 04-07)"
  - "Module-level doc-comment block per phase: '## Phase N extensions: ...' lets future planners locate the right anchor point quickly"

requirements-completed: [RUN-04, RUN-05]

# Metrics
duration: 6m
completed: 2026-05-19
---

# Phase 04 Plan 02: Scenario Engine Load-Time Validation Summary

**Regex compile-check (Rust regex ∩ JS RegExp subset) and Assertion.after resolution validation in LoadedConfig::from_config, with two new dedicated LoadError predicates.**

## Performance

- **Duration:** 6m 12s
- **Started:** 2026-05-19T14:05:02Z
- **Completed:** 2026-05-19T14:11:14Z
- **Tasks:** 3 (Task 1 chore; Task 2 TDD red→green; Task 3 verify-only)
- **Files modified:** 3 (workspace Cargo.toml, bootroom-core Cargo.toml, config.rs)

## Accomplishments

- Every `kind = "regex"` assertion pattern is now compiled by the Rust `regex` crate at `LoadedConfig::load_from_str` time. Failures surface as `LoadError::is_invalid_regex() == true` with the scenario name, the `after` label, and the offending pattern in the diagnostic.
- Every `Assertion.after` is resolved at load time. Legal values are the literal `"any"` OR a label in the containing `Scenario.actions` Vec. Typos and references to a top-level action that isn't part of THIS scenario are rejected with `LoadError::is_unresolvable_after() == true` and a sorted listing of legal values plus the universal `"any"` literal in the diagnostic.
- The supported regex feature subset (Rust ∩ JS) is documented inline at the top of `config.rs` for 04-08 to reference when constructing JS `RegExp`.
- `regex` is now a direct `bootroom-core` dep (was transitively present per 04-RESEARCH).
- 21/21 `config::tests` pass; 48/48 workspace tests pass; `cargo clippy -- -D warnings` is clean.

## Task Commits

1. **Task 1: Promote `regex` to a direct `bootroom-core` dep** — `74326e3` (chore)
2. **Task 2 RED: failing tests for both new checks** — `db028c4` (test)
3. **Task 2 GREEN: regex compile-check + after-resolution implementation** — `85ab7d2` (feat)
4. **Task 3: Grep gates** — no commit (verify-only; gates run against the GREEN tree)

**Plan metadata commit:** see this SUMMARY's own commit.

## Files Created/Modified

- `Cargo.toml` — added `regex = "1"` workspace dep between `open` and `serde`
- `crates/bootroom-core/Cargo.toml` — added `regex.workspace = true` (alphabetically first under `[dependencies]`)
- `crates/bootroom-core/src/config.rs`:
  - Module-level doc block: new `## Phase 4 extensions` section
  - `enum LoadErrorKind`: added `InvalidRegex` and `UnresolvableAfter` arms
  - `LoadError`: added `invalid_regex(...)`, `unresolvable_after(...)` constructors and `is_invalid_regex()`, `is_unresolvable_after()` public predicates
  - `LoadedConfig::from_config`: appended a per-assertion validation pass running after the existing scenario cross-validation loop
  - `#[cfg(test)] mod tests`: appended 9 tests (5 regex + 4 after-resolution; see "Test Inventory" below)

## Test Inventory

Phase-3 baseline: 12 `config::tests`. Phase-4 plan 02 additions:

**Regex compile-check (5):**
- `regex_assertion_valid_pattern_loads_ok`
- `regex_assertion_invalid_pattern_rejected`
- `regex_assertion_backref_rejected`
- `regex_assertion_lookaround_rejected`
- `contains_assertion_with_bracket_loads_ok`

**`after`-resolution (4):**
- `assertion_after_resolves_to_scenario_action_loads_ok`
- `assertion_after_any_loads_ok`
- `assertion_after_typo_rejected`
- `assertion_after_references_action_not_in_scenario_rejected`

**Total `config::tests` after this plan: 12 + 9 = 21 passing.**

```
test result: ok. 21 passed; 0 failed; 0 ignored; 0 measured; 27 filtered out
```

## Grep Gate Output

```
Gate 1 OK: regex::Regex::new count = 1
Gate 2a OK: InvalidRegex present
Gate 2b OK: UnresolvableAfter present
Gate 3a OK: is_invalid_regex predicate present
Gate 3b OK: is_unresolvable_after predicate present
Gate 4 OK: AssertionKind::Regex matcher present
Gate 5 OK: "any" literal compare present
OK
```

All five grep gates emit `OK`.

## Verification Suite

- `cargo build -p bootroom-core` — succeeds.
- `cargo test -p bootroom-core --lib config::tests` — 21 passed.
- `cargo clippy -p bootroom-core -- -D warnings` — no warnings.
- `cargo clippy --workspace -- -D warnings` — no warnings across the workspace.
- `cargo test --workspace --no-fail-fast` — 48 passed total (bootroom-core 21 config + 18 other + bootroom 5 + spike-b 0 + doc-tests 0 + 4 more module-test totals = 48 passed in aggregate; no failures).
- `grep -c 'regex::Regex::new' crates/bootroom-core/src/config.rs` reports `1`.

## Decisions Made

- **Regex API arg by reference.** Plan code snippet shows `err: regex::Error` (by value); clippy pedantic flagged `needless_pass_by_value`. Switched to `err: &regex::Error` to satisfy `-D warnings` without changing the constructor's call-site semantics (the `map_err` closure now passes `&e`). No behaviour change.
- **Sort the `legal` list in `unresolvable_after`.** Plan comment said "Sort legal for stable error output across runs" — kept as specified. Helps when error messages are used in snapshot tests by future plans.
- **Test count for after-resolution is 4, not 3.** Plan summary header listed 3 but the action steps spelled out 4 (positive #1 = action-in-scenario, positive #2 = "any", negative #1 = typo, negative #2 = top-level-but-not-in-scenario). Implemented all 4 — they exercise distinct code paths.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 — Bug] Plan code snippet failed `clippy::needless_pass_by_value`**
- **Found during:** Task 2 GREEN (right after first compile attempt of the new constructor)
- **Issue:** Plan snippet for `LoadError::invalid_regex` declared `err: regex::Error` by value, but `bootroom-core` has `[lints.clippy] pedantic = "warn"` (treated as `-D warnings` in CI). Clippy fired `needless_pass_by_value`.
- **Fix:** Changed parameter to `err: &regex::Error` and updated the single call-site in the `map_err` closure to pass `&e`.
- **Files modified:** `crates/bootroom-core/src/config.rs`
- **Verification:** `cargo clippy -p bootroom-core -- -D warnings` is clean; all 21 tests still pass.
- **Committed in:** `85ab7d2` (Task 2 GREEN commit — fix folded in alongside the GREEN implementation)

### Plan-Spec Discrepancies (noted, not auto-fixed)

**2. [Plan-vs-reality] Plan's verify threshold (`>= 31` config tests) is unreachable given the Phase-3 baseline**
- **Found during:** Task 2 verify
- **Issue:** The plan's `<verify><automated>` block for Task 2 asserts `>= 31` config tests passing. The plan's must_haves field claims "23 pre-existing bootroom-core config tests" but the actual Phase-3 baseline is 12 `config::tests`. With 9 new tests added (per the spelled-out action steps) the total is 21, not 31.
- **Why not auto-fixed:** This is a plan authoring miscount, not a code bug. Adding 10 more synthetic tests purely to clear an arbitrary threshold would be padding. The code-level success criteria (regex compile-check ON, after-resolution check ON, dedicated predicates exposed, 9 named tests present) ARE satisfied.
- **Files modified:** none (documented here for the planner / verifier).
- **Impact:** None on correctness. The next executor of 04-07 or 04-11 should treat the spec as "21 config tests baseline post-04-02" rather than 31.

---

**Total deviations:** 1 auto-fixed (clippy pedantic), 1 plan-spec discrepancy documented but not patched.
**Impact on plan:** Behaviour matches plan's `<success_criteria>` exactly; the verify-script threshold over-counts the realistic Phase-3 baseline.

## Issues Encountered

- **Worktree branch lagged master.** When this executor started, the per-agent worktree branch was at `1a224cf` (pre-Phase-4) while `master` contained both the Phase-4 plan files AND the Phase-4 Plan 01 implementation. Rebased `worktree-agent-aceb2f8dad1644652` onto `master` cleanly with no conflicts before reading the plan. No code impact; just a one-step prerequisite.

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

- **04-08 (browser scenario engine)** can now assume every regex pattern reaching the browser has already compiled in Rust under the strict-subset rules. JS `RegExp` will not see backrefs or lookaround — those fail load.
- **04-07 (bootroom check CLI)** can surface `is_invalid_regex()` and `is_unresolvable_after()` as dedicated exit codes if desired.
- Schema is unchanged; no migration required for existing operator configs that already passed Phase-3 validation, unless they happen to contain a regex pattern that uses backref/lookaround (in which case they were already broken for the browser engine and `bootroom check` will now say so).

## Self-Check: PASSED

- `74326e3` — present in git log
- `db028c4` — present in git log
- `85ab7d2` — present in git log
- `Cargo.toml` — modified (regex workspace dep added)
- `crates/bootroom-core/Cargo.toml` — modified (regex.workspace dep added)
- `crates/bootroom-core/src/config.rs` — modified (validation pass + 9 tests + module doc block)

---
*Phase: 04-scenario-engine-headless-run*
*Plan: 02*
*Completed: 2026-05-19*
