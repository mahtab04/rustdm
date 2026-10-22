//! Persistent job queue and attempt history shared by application adapters.
mod model;
mod runner;
mod store;
#[cfg(test)]
#[path = "../../../../tests/core/queue/store.rs"]
mod tests;

pub use model::{HistoryEntry, JobState, QueueEvent, QueueJob, QueueRunSummary};
pub use runner::QueueRunner;
pub use store::DownloadQueue;
