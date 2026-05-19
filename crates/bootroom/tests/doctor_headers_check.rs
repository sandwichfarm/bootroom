//! In-process router self-check pin for `doctor::check_headers`.
//!
//! This is the load-bearing regression test for Phase 1's COOP/COEP
//! middleware: if `tower-http`'s `SetResponseHeaderLayer` ever stops
//! attaching `Cross-Origin-Opener-Policy` / `Cross-Origin-Embedder-Policy`
//! to `/`, this test fails immediately without any browser involvement.
//!
//! Calls `bootroom::doctor_cmd::check_headers().await` directly
//! (Option A in 05-05-PLAN.md). The function is `pub` so integration
//! tests outside the crate can wire it up.
//!
//! Plan 05-05 — DOC-01.

use bootroom::doctor_cmd::{check_headers, CheckStatus};

#[tokio::test(flavor = "multi_thread")]
async fn check_headers_passes_against_build_router() {
    let check = check_headers().await;
    assert_eq!(
        check.name, "headers",
        "check name must be `headers`; got: {:?}",
        check.name
    );
    assert!(
        matches!(check.status, CheckStatus::Pass),
        "expected CheckStatus::Pass; got {:?}; detail: {}",
        check.status,
        check.detail
    );
    assert!(
        check.detail.contains("COOP=same-origin"),
        "detail must mention `COOP=same-origin`; got: {}",
        check.detail
    );
    assert!(
        check.detail.contains("COEP=require-corp"),
        "detail must mention `COEP=require-corp`; got: {}",
        check.detail
    );
}
