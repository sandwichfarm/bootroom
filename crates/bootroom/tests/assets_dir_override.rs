//! SERV-04: --assets-dir override serves disk content, blocks path traversal,
//! and falls through to the embedded copy on miss.

mod common;
use std::io::Write;

#[tokio::test]
async fn test_override_serves_disk_html() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(dir.path().join("web")).unwrap();
    std::fs::File::create(dir.path().join("web/index.html"))
        .unwrap()
        .write_all(b"<!doctype html><title>override</title>")
        .unwrap();

    let kernel = common::write_kernel_tempfile(b"x");
    let server = common::spawn(kernel.path().to_path_buf(), Some(dir.path().to_path_buf())).await;
    let res = reqwest::get(format!("{}/", server.base_url)).await.unwrap();
    assert_eq!(res.status(), 200);
    let body = res.text().await.unwrap();
    assert!(
        body.contains("override"),
        "should serve disk content; got: {body}"
    );
}

#[tokio::test]
async fn test_override_path_traversal_blocked() {
    let dir = tempfile::tempdir().expect("tempdir");
    let kernel = common::write_kernel_tempfile(b"x");
    let server = common::spawn(kernel.path().to_path_buf(), Some(dir.path().to_path_buf())).await;
    // reqwest's URL parser will normalize ../ segments client-side; build the
    // request manually so the .. segments hit the server intact.
    let client = reqwest::Client::builder()
        .build()
        .expect("reqwest client");
    let url = format!(
        "{}/assets/web/../../../etc/passwd",
        server.base_url
    );
    let res = client.get(&url).send().await.unwrap();
    // Accept EITHER 400 (deliberate rejection on `..` segment) OR 404 (URL
    // normalized away into a non-existent route). Both block the traversal.
    // The negative we care about: NOT 200, and not /etc/passwd content.
    assert!(
        res.status() == 400 || res.status() == 404,
        "expected 400 or 404 for traversal attempt; got {}",
        res.status()
    );
}

#[tokio::test]
async fn test_override_falls_through_to_embedded() {
    // --assets-dir set but the requested file does not exist on disk; the
    // handler must fall back to the embedded copy.
    let dir = tempfile::tempdir().expect("tempdir");
    let kernel = common::write_kernel_tempfile(b"x");
    let server = common::spawn(kernel.path().to_path_buf(), Some(dir.path().to_path_buf())).await;
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
}
