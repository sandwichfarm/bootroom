//! SERV-03: embedded qemu-wasm + vendored UI assets reachable with correct MIME.

mod common;

#[tokio::test]
async fn test_serves_wasm() {
    let kernel = common::write_kernel_tempfile(b"x");
    let server = common::spawn(kernel.path().to_path_buf(), None).await;
    let res = reqwest::get(format!(
        "{}/assets/qemu/qemu-system-riscv64.wasm",
        server.base_url
    ))
    .await
    .unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(
        res.headers().get("content-type").unwrap(),
        "application/wasm"
    );
    let bytes = res.bytes().await.unwrap();
    assert!(
        bytes.len() > 1_000_000,
        "wasm should be > 1MB; got {} bytes",
        bytes.len()
    );
}

#[tokio::test]
async fn test_serves_vendored_xterm() {
    let kernel = common::write_kernel_tempfile(b"x");
    let server = common::spawn(kernel.path().to_path_buf(), None).await;
    let res = reqwest::get(format!("{}/assets/web/vendor/xterm.js", server.base_url))
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let ct = res
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(ct.contains("javascript"), "expected js MIME, got {ct}");
}

#[tokio::test]
async fn test_serves_index_html() {
    let kernel = common::write_kernel_tempfile(b"x");
    let server = common::spawn(kernel.path().to_path_buf(), None).await;
    let res = reqwest::get(format!("{}/", server.base_url)).await.unwrap();
    assert_eq!(res.status(), 200);
    let ct = res
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(ct.contains("html"), "expected html MIME, got {ct}");
    let body = res.text().await.unwrap();
    assert!(
        body.contains("id=\"terminal\""),
        "index.html must mount terminal"
    );
    assert!(
        body.contains("crossOriginIsolated"),
        "index.html must include SAB probe"
    );
}
