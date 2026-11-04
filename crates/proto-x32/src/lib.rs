//! Behringer X32 / Midas M32 family provider. OSC control on UDP 10023.
//!
//!
//! Layout mirrors `proto-wing`: this file owns the session struct,
//! `addrs` the topology + wire roots, `codec` the engineering-unit
//! conversions, `mapping` the param plane, `lifecycle` the session +
//! identity validation, and the remaining modules one domain each.
pub mod addrs;
mod codec;
mod discovery;
mod driver;
pub mod fx;
mod groups;
mod inbound;
mod lifecycle;
mod link;
mod mapping;
pub mod native;
mod preamp;
mod presentation;
pub mod routing;
pub mod sends;
mod solo;
pub(crate) mod telemetry;

pub use addrs::{X32_STRIPS, X32_TOPOLOGY};
pub use link::STEREO_PAIRS;

use lifecycle::Link;
use openfader_core::ParamUpdate;
use std::{
    sync::{Arc, Mutex, RwLock},
    time::{Duration, Instant},
};
use tokio::sync::broadcast;

const QUERY_TIMEOUT: Duration = Duration::from_millis(700);

pub struct X32Driver {
    link: RwLock<Option<Link>>,
    lifecycle: tokio::sync::Mutex<()>,
    writes: tokio::sync::Mutex<()>,
    updates: broadcast::Sender<ParamUpdate>,
    last_control: Arc<Mutex<Option<Instant>>>,
    pub(crate) telemetry: Arc<telemetry::Telemetry>,
    pub(crate) native: std::sync::Mutex<native::NativeState>,
}

impl Default for X32Driver {
    fn default() -> Self {
        Self::new()
    }
}

impl X32Driver {
    pub fn new() -> Self {
        Self {
            link: RwLock::new(None),
            lifecycle: tokio::sync::Mutex::new(()),
            writes: tokio::sync::Mutex::new(()),
            updates: broadcast::channel(2048).0,
            last_control: Arc::new(Mutex::new(None)),
            telemetry: Arc::new(telemetry::Telemetry::default()),
            native: std::sync::Mutex::new(native::NativeState::default()),
        }
    }

    pub fn param_scopes() -> Vec<(&'static str, &'static [openfader_core::StripKind])> {
        presentation::scopes()
    }
}
