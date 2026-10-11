//! ClearScope core: scanning engines, live updates, cache and analysis.
pub mod analysis;
pub mod cache;
pub mod dupes;
pub mod live;
pub mod mft;
pub mod progress;
pub mod safety;
pub mod snapshot;
pub mod tree;
#[cfg(windows)]
pub mod usn;
pub mod walk;

pub use progress::{Phase, Progress};
pub use tree::{ScanMethod, Tree, NONE};
