//! Asset filesystem roots and index loading.

use std::path::PathBuf;

/// Per-user writable cache root: `openlogi_core::paths::data_dir()` plus an
/// `assets/` subdir, keeping the render cache out of the config dir. Falls
/// back to `./assets` only when no home directory can be resolved.
pub(super) fn user_cache_root() -> PathBuf {
    openlogi_core::paths::data_dir()
        .map_or_else(|_| PathBuf::from("./assets"), |d| d.join("assets"))
}
