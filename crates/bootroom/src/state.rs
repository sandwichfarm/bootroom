//! Shared server state passed via `axum::extract::State`.

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AppState {
    /// Path to the kernel image, as supplied via `--kernel`.
    pub kernel: PathBuf,
    /// If set, serve UI/qemu assets from this disk path instead of the
    /// embedded copy. Layout: `<dir>/web/` + `<dir>/assets/qemu/`.
    pub assets_dir: Option<PathBuf>,
}

impl AppState {
    #[must_use]
    pub fn new(kernel: PathBuf, assets_dir: Option<PathBuf>) -> Self {
        Self { kernel, assets_dir }
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
    }
}
