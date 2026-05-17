//! SERV-05: --port 0 ephemeral binding works.
//!
//! The --host flag path is exercised at the clap layer (plan 01-04 manual
//! smoke); spawning a 0.0.0.0-bound server inside a test would require either
//! shelling to the binary or root, both overkill for an integration test.

mod common;

#[tokio::test]
async fn test_port_zero_binds_ephemeral() {
    let kernel = common::write_kernel_tempfile(b"hello");
    let server = common::spawn(kernel.path().to_path_buf(), None).await;
    // base_url has the form "http://127.0.0.1:NNNNN" — extract the port suffix.
    let port: u16 = server
        .base_url
        .rsplit(':')
        .next()
        .unwrap()
        .parse()
        .expect("port");
    assert_ne!(port, 0, "ephemeral bind should give a real port");
    assert!(port > 1024, "ephemeral port should be > 1024 (got {port})");

    // Sanity: server actually answers on that port.
    let res = reqwest::get(format!("{}/api/kernel/info", server.base_url))
        .await
        .expect("get");
    assert_eq!(res.status(), 200);
}
