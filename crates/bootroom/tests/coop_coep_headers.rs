//! SERV-02: COOP + COEP headers on every response, including the 404 path.
//!
//! Pitfall 1 regression gate: the 404 case is the one that handler-level
//! header sets cannot reach — the router middleware must add them.

mod common;

async fn assert_coop_coep(url: &str) -> reqwest::Response {
    let res = reqwest::get(url).await.expect("get");
    assert_eq!(
        res.headers().get("cross-origin-opener-policy").unwrap(),
        "same-origin",
        "COOP missing on {url}"
    );
    assert_eq!(
        res.headers().get("cross-origin-embedder-policy").unwrap(),
        "require-corp",
        "COEP missing on {url}"
    );
    res
}

#[tokio::test]
async fn test_coop_coep_on_index() {
    let kernel = common::write_kernel_tempfile(b"hello");
    let server = common::spawn(kernel.path().to_path_buf(), None).await;
    let res = assert_coop_coep(&format!("{}/", server.base_url)).await;
    // / serves embedded web/index.html which exists post-01-06.
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_coop_coep_on_api() {
    let kernel = common::write_kernel_tempfile(b"hello");
    let server = common::spawn(kernel.path().to_path_buf(), None).await;
    let res = assert_coop_coep(&format!("{}/api/kernel/info", server.base_url)).await;
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_coop_coep_on_wasm_asset() {
    let kernel = common::write_kernel_tempfile(b"hello");
    let server = common::spawn(kernel.path().to_path_buf(), None).await;
    let res = assert_coop_coep(&format!(
        "{}/assets/qemu/qemu-system-riscv64.wasm",
        server.base_url
    ))
    .await;
    assert_eq!(res.status(), 200);
    assert_eq!(
        res.headers().get("content-type").unwrap(),
        "application/wasm"
    );
}

#[tokio::test]
async fn test_coop_coep_on_404() {
    let kernel = common::write_kernel_tempfile(b"hello");
    let server = common::spawn(kernel.path().to_path_buf(), None).await;
    let res = assert_coop_coep(&format!("{}/totally-bogus-path-zzz", server.base_url)).await;
    assert_eq!(res.status(), 404);
}
