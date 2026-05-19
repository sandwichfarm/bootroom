//! Wave-0 scaffold for Phase 5 doctor subcommand integration tests.
//!
//! This file pins the contract that `crates/bootroom/build.rs` exposes
//! `BOOTROOM_GIT_SHA` as a compile-time env var via
//! `cargo:rustc-env=BOOTROOM_GIT_SHA=...`. Plan 05-04 consumes this via
//! `env!("BOOTROOM_GIT_SHA")` in the doctor `version` check. Plan 05-05
//! will append CLI shape / exit-code / stderr-summary tests to this file.

const SHA: &str = env!("BOOTROOM_GIT_SHA");

#[test]
fn git_sha_env_is_set() {
    // The macro resolves at compile time; this asserts the value is a
    // non-empty &str at runtime.
    assert!(!SHA.is_empty(), "BOOTROOM_GIT_SHA must not be empty");
}

#[test]
fn git_sha_env_has_no_whitespace() {
    assert!(
        !SHA.chars().any(|c| c.is_whitespace()),
        "BOOTROOM_GIT_SHA must not contain whitespace, got {SHA:?}"
    );
}

#[test]
fn git_sha_env_shape_is_short_sha_or_unknown() {
    if SHA == "unknown" {
        return;
    }
    let len_ok = (7..=40).contains(&SHA.len());
    let hex_ok = SHA.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c));
    assert!(
        len_ok && hex_ok,
        "BOOTROOM_GIT_SHA must be \"unknown\" or [0-9a-f]{{7,40}}, got {SHA:?}"
    );
}
