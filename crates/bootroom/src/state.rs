//! Shared server state passed via `axum::extract::State`.

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AppState {
    /// Path to the kernel image, as supplied via `--kernel`.
    pub kernel: PathBuf,
    /// If set, serve UI/qemu assets from this disk path instead of the
    /// embedded copy. Layout: `<dir>/web/` + `<dir>/assets/qemu/`.
    pub assets_dir: Option<PathBuf>,
    /// Canonicalized form of `assets_dir`, computed once at startup so
    /// the per-request path-traversal check (CR-02) does not need to
    /// recursively canonicalize the root on every asset GET. `None` if
    /// `assets_dir` is `None` or could not be canonicalized at startup.
    pub assets_dir_canon: Option<PathBuf>,
}

impl AppState {
    #[must_use]
    pub fn new(kernel: PathBuf, assets_dir: Option<PathBuf>) -> Self {
        let assets_dir_canon = assets_dir
            .as_ref()
            .and_then(|d| std::fs::canonicalize(d).ok());
        Self {
            kernel,
            assets_dir,
            assets_dir_canon,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_appstate_construct() {
        let s = AppState::new(PathBuf::from("/tmp/Image"), None);
        assert_eq!(s.kernel, PathBuf::from("/tmp/Image"));
        assert!(s.assets_dir.is_none());
        assert!(s.assets_dir_canon.is_none());
    }

    #[test]
    fn test_appstate_canonicalizes_assets_dir() {
        // Real existing dir — canonicalization succeeds.
        let tmp = std::env::temp_dir();
        let s = AppState::new(PathBuf::from("/tmp/Image"), Some(tmp.clone()));
        assert!(s.assets_dir_canon.is_some());
    }
}
