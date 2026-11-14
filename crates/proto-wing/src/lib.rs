//! Behringer WING family provider. OSC control on UDP 2223; native meter
//! requests on TCP 2222. Reference: WING Remote Protocols V3.0.5-1 and
//! <https://github.com/dannyfiresnake/libwing> (protocol cross-check only).
//! No successful UDP send is treated as a confirmed desk value.
mod catalogue;
mod console_controls;
mod console_metadata;
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
mod processors;
mod routing;
pub mod sends;
mod show_library;
pub mod source;
pub(crate) use catalogue::capabilities;
pub use processors::{
    validate_complete_native_processor_snapshot, validate_native_processor_snapshot,
};
pub use routing::ROUTING_GROUPS;

use lifecycle::Link;
use openfader_core::ParamUpdate;
use std::{
    sync::{atomic::AtomicU64, Arc, Mutex, RwLock},
    time::{Duration, Instant},
};
use tokio::sync::broadcast;
const QUERY_TIMEOUT: Duration = Duration::from_millis(700);

pub struct WingDriver {
    link: RwLock<Option<Link>>,
    generation: AtomicU64,
    native_revision: AtomicU64,
    show_library: Mutex<show_library::State>,
    color_domains: Mutex<std::collections::HashMap<String, (i32, i32)>>,
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
            generation: AtomicU64::new(0),
            native_revision: AtomicU64::new(0),
            show_library: Mutex::new(show_library::State::default()),
            color_domains: Mutex::new(std::collections::HashMap::new()),
            lifecycle: tokio::sync::Mutex::new(()),
            writes: tokio::sync::Mutex::new(()),
            updates: broadcast::channel(2048).0,
            last_control: Arc::new(Mutex::new(None)),
        }
    }
    /// The legacy selector table comes from a 3.0 schema; later or
    /// unrecognized firmware must use the live native descriptor instead.
    pub(crate) fn has_legacy_dynamics_selectors(&self) -> bool {
        let link = self.link.read().unwrap();
        let Some(link) = link.as_ref() else {
            return false;
        };
        link.info
            .firmware
            .strip_prefix("3.0")
            .is_some_and(|suffix| suffix.is_empty() || suffix.starts_with(['.', '-', '+', ' ']))
    }
    pub fn param_scopes() -> Vec<(&'static str, &'static [openfader_core::StripKind])> {
        presentation::scopes()
    }
}
