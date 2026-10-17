//! Deprecated alias: exponential reconnect backoff moved into [`crate::backoff`].
//!
//! Kept so `openmix_transport::reconnect::…` paths keep resolving; new code
//! should import from `crate::backoff` (or the crate root).
pub use super::backoff::*;
