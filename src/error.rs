//! Error type for the library API.
//!
//! The binary turns these into a stderr message and a non-zero exit code; a
//! library consumer can match on the variants. `NoGpu` is deliberately distinct
//! so callers can tell "this machine has no usable GPU" apart from a real fault.

/// Anything that can go wrong while running a probe.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No usable (non-CPU) GPU adapter, or the device could not be created.
    /// The message mirrors the `no_gpu:` text the CLI has always printed.
    #[error("no_gpu: {0}")]
    NoGpu(String),

    /// A GPU device operation failed after setup (e.g. buffer mapping).
    #[error("device error: {0}")]
    Device(String),

    /// The SQLite store failed to open or insert.
    #[cfg(feature = "sqlite")]
    #[error(transparent)]
    Db(#[from] rusqlite::Error),
}
