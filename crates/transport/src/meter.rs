//! Deprecated alias: meter parsing + ring buffer moved into [`crate::meters`].
//!
//! Kept so `openmix_transport::meter::…` and `crate::meter::…` paths keep
//! resolving; new code should import from `crate::meters` (or the crate root).
pub use super::meters::*;
