//! Behringer WING family provider. OSC control on UDP 2223; native meter
//! requests on TCP 2222. Reference: WING Remote Protocols V3.0.5-1 and
//! <https://github.com/dannyfiresnake/libwing> (protocol cross-check only).
//! No successful UDP send is treated as a confirmed desk value.
mod catalogue;
mod control;
pub mod delay;
mod discovery;
mod driver;
pub mod filters;
pub mod fx;
pub mod groups;
mod lifecycle;
pub mod mapping;
pub mod native;
mod presentation;
mod routing;
pub mod sends;
pub mod source;
pub(crate) use catalogue::capabilities;
pub use routing::ROUTING_GROUPS;

use lifecycle::Link;
use openfader_core::ParamUpdate;
use std::{
    sync::{Arc, Mutex, RwLock},
    time::{Duration, Instant},
};
use tokio::sync::broadcast;
const QUERY_TIMEOUT: Duration = Duration::from_millis(700);

pub struct WingDriver {
    link: RwLock<Option<Link>>,
    lifecycle: tokio::sync::Mutex<()>,
    writes: tokio::sync::Mutex<()>,
    updates: broadcast::Sender<ParamUpdate>,
    last_control: Arc<Mutex<Option<Instant>>>,
}
impl Default for WingDriver {
    fn default() -> Self {
        Self::new()
    }
}
impl WingDriver {
    pub fn new() -> Self {
        Self {
            link: RwLock::new(None),
            lifecycle: tokio::sync::Mutex::new(()),
            writes: tokio::sync::Mutex::new(()),
            updates: broadcast::channel(2048).0,
            last_control: Arc::new(Mutex::new(None)),
        }
    }
    pub fn param_scopes() -> Vec<(&'static str, &'static [openfader_core::StripKind])> {
        presentation::scopes()
    }
}
