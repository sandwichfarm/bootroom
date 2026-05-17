//! UI-07 (API surface): GET /api/kernel/info JSON shape contract.

mod common;
use serde_json::Value;

#[tokio::test]
async fn test_kernel_info_shape() {
    // sha256(b"abc") = ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
    let kernel = common::write_kernel_tempfile(b"abc");
    let server = common::spawn(kernel.path().to_path_buf(), None).await;
    let res = reqwest::get(format!("{}/api/kernel/info", server.base_url))
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let json: Value = res.json().await.unwrap();
    assert!(json.get("path").is_some(), "missing 'path'");
    assert!(json.get("size").is_some(), "missing 'size'");
    assert!(json.get("mtime").is_some(), "missing 'mtime'");
    assert!(json.get("sha256_prefix").is_some(), "missing 'sha256_prefix'");

    assert_eq!(json["size"].as_u64().unwrap(), 3);

    let sha = json["sha256_prefix"].as_str().unwrap();
    assert_eq!(sha.len(), 12, "sha256_prefix must be exactly 12 chars");
    assert!(
        sha.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
        "sha256_prefix must be lowercase hex; got {sha}"
    );
    assert_eq!(sha, "ba7816bf8f01", "known sha256(b\"abc\") prefix mismatch");

    let path = json["path"].as_str().unwrap();
    assert_eq!(path, kernel.path().display().to_string());
}

#[tokio::test]
async fn test_kernel_info_missing_file_404() {
    // The spawn helper doesn't run startup validation (that's server::run's
    // job); pass a deleted path directly so we exercise only the handler's
    // 404 behavior.
    let server = common::spawn(
        std::path::PathBuf::from("/does/not/exist/at/all/xyz"),
        None,
    )
    .await;
    let res = reqwest::get(format!("{}/api/kernel/info", server.base_url))
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}
