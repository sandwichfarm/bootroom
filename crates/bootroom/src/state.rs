//! Shared server state passed via `axum::extract::State`.

use bootroom_core::{WsMessage, config::LoadedConfig};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::broadcast;

/// WR-03: cached SHA-256 digest keyed by file identity. The kernel is
/// re-hashed only when `(size, mtime_sec)` changes. Phase 3's watcher
/// can also invalidate by writing `None` directly.
#[derive(Debug, Clone)]
pub struct CachedDigest {
    pub size: u64,
    pub mtime_sec: i64,
    /// Hex-encoded SHA-256 prefix (first 12 hex chars = first 6 bytes).
    pub sha256_prefix: String,
}

/// Broadcast-channel capacity for Phase 3 server -> WS-subscriber fan-out.
///
/// Per `03-CONTEXT.md` `<specifics>`: capacity 16. Slow consumers receive
/// `Lagged(n)` and drop the oldest frames; Plan 08 (WS forwarder) logs and
/// continues per Pitfall #3 in `03-RESEARCH.md`.
pub const WS_BROADCAST_CAPACITY: usize = 16;

#[derive(Debug, Clone)]
pub struct AppState {
    /// Path to the kernel image, as supplied via `--kernel`.
    pub kernel: PathBuf,
    /// Canonicalized form of `kernel`, computed once at startup. The watcher
    /// (Plan 06) compares filesystem events against this absolute path to
    /// demux kernel-vs-config rebuilds (Pitfall #1 in `03-RESEARCH.md` — a
    /// relative `--kernel ./Image` would otherwise silently miss its own
    /// rebuild events because notify reports absolute paths).
    pub kernel_canon: PathBuf,
    /// If set, serve UI/qemu assets from this disk path instead of the
    /// embedded copy. Layout: `<dir>/web/` + `<dir>/assets/qemu/`.
    pub assets_dir: Option<PathBuf>,
    /// Canonicalized form of `assets_dir`, computed once at startup so
    /// the per-request path-traversal check (CR-02) does not need to
    /// recursively canonicalize the root on every asset GET. `None` when
    /// `assets_dir` is `None` (no override active). WR-08: when an
    /// override IS passed, `server::run` canonicalizes it strictly and
    /// surfaces failure as a fatal startup error — so production code
    /// never sees `Some(assets_dir) + None(assets_dir_canon)`. Test
    /// fixtures may still hit the silent `.ok()` fallback below; those
    /// tests do not exercise the per-request traversal check.
    pub assets_dir_canon: Option<PathBuf>,
    /// WR-03 cache: kernel SHA-256 keyed by `(size, mtime_sec)`. The
    /// `tokio::sync::RwLock` keeps the hot path (`read()` returning
    /// `Some(matching)`) lock-free except for the read guard. Wrapped
    /// in `Arc` so the state itself stays `Clone`.
    pub digest_cache: Arc<tokio::sync::RwLock<Option<CachedDigest>>>,
    /// Path to `bootroom.toml`, as supplied via `--config` (or the default
    /// `./bootroom.toml`). Kept verbatim for display in startup banners /
    /// error messages.
    pub config_path: PathBuf,
    /// Canonicalized form of `config_path`. Plan 06's watcher compares
    /// notify events against this absolute path (Pitfall #1 mitigation).
    pub config_path_canon: PathBuf,
    /// Current in-flight config. Wrapped in `Arc<RwLock<_>>` so the
    /// watcher (one writer at a time) and `/api/config` handlers
    /// (many concurrent readers) share one canonical view.
    pub loaded_config: Arc<tokio::sync::RwLock<LoadedConfig>>,
    /// Server -> WS-subscriber fan-out channel. The watcher (Plan 06)
    /// and `/api/config` (Plan 07) send `WsMessage` frames into this
    /// `Sender`; per-connection WS tasks (Plan 08) subscribe and forward
    /// to their socket. Capacity is [`WS_BROADCAST_CAPACITY`].
    pub ws_broadcast: broadcast::Sender<WsMessage>,
    /// CR-02: allowed `Origin` header values for WebSocket upgrades.
    ///
    /// Same-origin policy is NOT auto-enforced by the browser on the
    /// WS handshake — the browser attaches `Origin` but it is the
    /// server's responsibility to compare and reject mismatches. Without
    /// this check any web page the operator visits in the same browser
    /// can `new WebSocket("ws://127.0.0.1:8765/ws")` and subscribe to
    /// every server-pushed frame (leaking `bootroom.toml` action labels
    /// + `bytes_b64` payloads) plus inject `Launch` / `Reset` frames.
    ///
    /// Populated by `server::run` from the bound `SocketAddr`. An empty
    /// `Vec` means "deny all" — used by tests that exercise `AppState`
    /// without going through `server::run` (those tests do not hit
    /// `ws_handler`).
    pub allowed_origins: Vec<String>,
}

impl AppState {
    /// Construct full `AppState` with all Phase-3 fields populated. Called
    /// by `server::run` after it has performed kernel-exists, config-load,
    /// and canonicalize-both-paths preflight.
    #[must_use]
    pub fn new(
        kernel: PathBuf,
        kernel_canon: PathBuf,
        assets_dir: Option<PathBuf>,
        config_path: PathBuf,
        config_path_canon: PathBuf,
        loaded_config: LoadedConfig,
        allowed_origins: Vec<String>,
    ) -> Self {
        let assets_dir_canon = assets_dir
            .as_ref()
            .and_then(|d| std::fs::canonicalize(d).ok());
        let (ws_broadcast, _) = broadcast::channel::<WsMessage>(WS_BROADCAST_CAPACITY);
        Self {
            kernel,
            kernel_canon,
            assets_dir,
            assets_dir_canon,
            digest_cache: Arc::new(tokio::sync::RwLock::new(None)),
            config_path,
            config_path_canon,
            loaded_config: Arc::new(tokio::sync::RwLock::new(loaded_config)),
            ws_broadcast,
            allowed_origins,
        }
    }

    /// Phase-2 compatibility shim for tests that pre-date the Phase-3
    /// `AppState` surface. Constructs an `AppState` with:
    ///
    /// - `kernel_canon` = `canonicalize(kernel)` falling back to `kernel`
    ///   itself when the file does not exist (tests use fake paths).
    /// - `config_path` = placeholder `bootroom.toml` (no watcher in tests).
    /// - `loaded_config` = empty config (no actions, no scenarios).
    /// - `ws_broadcast` = fresh `broadcast::channel(16)` sender.
    ///
    /// Tests that need to exercise watcher / `/api/config` behavior should
    /// construct `AppState` manually via [`AppState::new`].
    ///
    /// # Panics
    ///
    /// Panics if the hard-coded `schema_version = 1` trivial config fails to
    /// parse — that would be a Plan-01 regression (the trivial config is the
    /// minimum-syntactic-acceptance fixture).
    #[must_use]
    pub fn new_for_test(kernel: PathBuf, assets_dir: Option<PathBuf>) -> Self {
        let kernel_canon = std::fs::canonicalize(&kernel).unwrap_or_else(|_| kernel.clone());
        let config_path = PathBuf::from("bootroom.toml");
        let config_path_canon = config_path.clone();
        let loaded_config = LoadedConfig::load_from_str("schema_version = 1\n")
            .expect("trivial schema_version=1 config must parse");
        // CR-02: tests do not hit `ws_handler` so an empty origin list is
        // safe (any inbound WS upgrade would be rejected with 403, which
        // is the correct posture for an unconfigured handler).
        Self::new(
            kernel,
            kernel_canon,
            assets_dir,
            config_path,
            config_path_canon,
            loaded_config,
            Vec::new(),
        )
    }

    /// Test-only constructor that mirrors [`AppState::new_for_test`] but
    /// accepts an externally-built [`LoadedConfig`]. Used by the
    /// `/api/config` integration tests in Plan 03-07 to exercise the
    /// projection shape against an arbitrary TOML without spinning up the
    /// real watcher / `server::run` preflight.
    ///
    /// Scope note: this method was added in Plan 03-07 (not Plan 03-05)
    /// because the need surfaced when the `/api/config` test surface was
    /// being assembled. Treat the addition as test infrastructure — no
    /// production code path constructs an `AppState` this way.
    #[must_use]
    pub fn new_for_test_with_loaded(
        kernel: PathBuf,
        assets_dir: Option<PathBuf>,
        loaded_config: LoadedConfig,
    ) -> Self {
        let kernel_canon = std::fs::canonicalize(&kernel).unwrap_or_else(|_| kernel.clone());
        let config_path = PathBuf::from("bootroom.toml");
        let config_path_canon = config_path.clone();
        Self::new(
            kernel,
            kernel_canon,
            assets_dir,
            config_path,
            config_path_canon,
            loaded_config,
            Vec::new(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bootroom_core::WsMessage;
    use std::path::PathBuf;

    #[tokio::test]
    async fn appstate_new_for_test_has_empty_config() {
        let s = AppState::new_for_test(PathBuf::from("/tmp/Image"), None);
        let cfg = s.loaded_config.read().await;
        assert_eq!(cfg.actions().len(), 0);
        assert_eq!(cfg.scenarios().len(), 0);
    }

    #[tokio::test]
    async fn appstate_broadcast_subscribe_works() {
        let s = AppState::new_for_test(PathBuf::from("/tmp/Image"), None);
        let mut rx = s.ws_broadcast.subscribe();
        // Send Launch — a unit variant — so the round-trip is easy to assert.
        s.ws_broadcast
            .send(WsMessage::Launch)
            .expect("send to live subscriber");
        let received = rx.recv().await.expect("recv");
        assert_eq!(received, WsMessage::Launch);
    }

    #[test]
    fn appstate_clone_shares_loaded_config() {
        let s1 = AppState::new_for_test(PathBuf::from("/tmp/Image"), None);
        let s2 = s1.clone();
        // Both clones must point to the same Arc<RwLock<LoadedConfig>>:
        // mutating through one is visible through the other.
        assert!(Arc::ptr_eq(&s1.loaded_config, &s2.loaded_config));
        // Broadcast::Sender clones share one underlying channel — sending
        // through s2 should reach a subscriber on s1.
        let mut rx = s1.ws_broadcast.subscribe();
        s2.ws_broadcast.send(WsMessage::Reset).expect("send");
        // Drain synchronously via try_recv: the message is already enqueued
        // on the channel by the time send returns.
        let msg = rx.try_recv().expect("try_recv");
        assert_eq!(msg, WsMessage::Reset);
    }

    #[test]
    fn appstate_canonical_kernel_is_absolute() {
        // Use a tempfile so canonicalize succeeds.
        let f = tempfile::NamedTempFile::new().expect("tempfile");
        let path = f.path().to_path_buf();
        let s = AppState::new_for_test(path.clone(), None);
        assert!(
            s.kernel_canon.is_absolute(),
            "kernel_canon must be absolute, got {}",
            s.kernel_canon.display()
        );
        // assets_dir was None — canonical form stays None.
        assert!(s.assets_dir.is_none());
        assert!(s.assets_dir_canon.is_none());
    }

    #[test]
    fn appstate_canonicalizes_assets_dir() {
        // Real existing dir — canonicalization succeeds.
        let tmp = std::env::temp_dir();
        let s = AppState::new_for_test(PathBuf::from("/tmp/Image"), Some(tmp.clone()));
        assert!(s.assets_dir_canon.is_some());
    }
}
