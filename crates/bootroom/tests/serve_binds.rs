//! SERV-01: server binds and responds.

mod common;

#[tokio::test]
async fn test_server_binds_and_responds() {
    let kernel = common::write_kernel_tempfile(b"hello");
    let server = common::spawn(kernel.path().to_path_buf(), None).await;
    let res = reqwest::get(format!("{}/api/kernel/info", server.base_url))
        .await
        .expect("get");
    assert_eq!(res.status(), 200);
}
