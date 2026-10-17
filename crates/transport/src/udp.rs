//! Deprecated alias: [`UdpPeer`] moved into [`crate::session`].
//!
//! Kept so `openmix_transport::udp::…` and `crate::udp::…` paths keep
//! resolving; new code should import from `crate::session` (or the crate root).
pub use super::session::UdpPeer;
