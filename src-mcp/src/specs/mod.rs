//! Spec loading, indexing, and search.

pub mod loader;
pub mod model;
pub mod search;

pub use loader::{spawn_rescan_loop, LoaderHandle, SpecIndex};
pub use model::{Doc, DocKind};
pub use search::search;
