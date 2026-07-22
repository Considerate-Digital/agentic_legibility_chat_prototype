//! Shared application context threaded through tool handlers.
//!
//! `LIVE_RESOURCES_DIR` gates the 14 spec/memory tools (12 ported from
//! legibility-mcp plus `get_memory`/`add_memory`). This sidecar also serves
//! `fetch`/`report_service_step`/`ui_input`, which must keep working even
//! when the specs directory is absent or broken — so any failure here
//! degrades to `spec_index: None` rather than panicking or exiting.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;

use crate::specs::{spawn_rescan_loop, LoaderHandle, SpecIndex};

/// How often to rescan `LIVE_RESOURCES_DIR` in the background, matching the
/// legibility-mcp default (`SCAN_INTERVAL_SECS=30`).
const DEFAULT_SCAN_INTERVAL: Duration = Duration::from_secs(30);

pub struct AppContext {
    /// `Some` iff `LIVE_RESOURCES_DIR` was set, existed, and the initial
    /// scan succeeded. Shared with the background rescan task.
    pub spec_index: Option<Arc<RwLock<SpecIndex>>>,
    /// The configured specs root, kept alongside `spec_index` since
    /// `search_specs` and the memory tools need the raw path (e.g. to shell
    /// out to `rg`, or to read/append `memory.md`).
    pub live_resources_dir: Option<PathBuf>,
    /// Keeps the background rescan task alive for the process lifetime.
    /// Never read; dropping it would abort the task.
    _rescan_handle: Option<LoaderHandle>,
}

impl AppContext {
    /// Build context from the environment. Never panics or exits: a missing
    /// `LIVE_RESOURCES_DIR`, a nonexistent directory, or a failed initial
    /// scan all degrade to `spec_index: None` (logged to stderr), so the
    /// always-on tools are unaffected.
    pub fn from_env() -> Self {
        let disabled = || Self {
            spec_index: None,
            live_resources_dir: None,
            _rescan_handle: None,
        };

        let raw = match std::env::var("LIVE_RESOURCES_DIR") {
            Ok(v) if !v.is_empty() => v,
            _ => return disabled(),
        };
        let dir = PathBuf::from(raw);

        if !dir.exists() {
            eprintln!(
                "warning: LIVE_RESOURCES_DIR '{}' does not exist; spec/memory tools disabled",
                dir.display()
            );
            return disabled();
        }

        match SpecIndex::scan(&dir) {
            Ok(index) => {
                let state = Arc::new(RwLock::new(index));
                let handle =
                    spawn_rescan_loop(state.clone(), dir.clone(), DEFAULT_SCAN_INTERVAL);
                Self {
                    spec_index: Some(state),
                    live_resources_dir: Some(dir),
                    _rescan_handle: Some(handle),
                }
            }
            Err(err) => {
                eprintln!(
                    "warning: initial specs scan of '{}' failed: {err:#}; spec/memory tools disabled",
                    dir.display()
                );
                disabled()
            }
        }
    }
}

#[cfg(test)]
impl AppContext {
    /// Test-only constructor around an already-scanned index, bypassing
    /// `from_env()` and the background rescan task.
    pub(crate) fn for_test(dir: PathBuf, index: SpecIndex) -> Self {
        Self {
            spec_index: Some(Arc::new(RwLock::new(index))),
            live_resources_dir: Some(dir),
            _rescan_handle: None,
        }
    }

    /// Test-only constructor mimicking an unconfigured/degraded environment
    /// (no `LIVE_RESOURCES_DIR`, or a failed initial scan).
    pub(crate) fn disabled_for_test() -> Self {
        Self {
            spec_index: None,
            live_resources_dir: None,
            _rescan_handle: None,
        }
    }
}
