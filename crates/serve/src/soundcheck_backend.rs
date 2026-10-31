//! Injectable soundcheck commands. Production has no verified backend yet.
use serde_json::Value;
use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

/// Cancellation is a request; termination is confirmed only after the helper exits.
#[derive(Clone, Default)]
pub struct CancellationHandle {
    pub(crate) requested: Arc<AtomicBool>,
    pub(crate) terminated: Arc<AtomicBool>,
}
impl CancellationHandle {
    pub fn cancel(&self) {
        self.requested.store(true, Ordering::Release);
    }
    pub fn cancellation_requested(&self) -> bool {
        self.requested.load(Ordering::Acquire)
    }
    pub fn output_terminated(&self) -> bool {
        self.terminated.load(Ordering::Acquire)
    }
}

pub struct SoundcheckState {
    pub input_mode: String,
    pub transport: String,
}

/// Implementations must enforce the verified device/profile gate before entering.
pub trait SoundcheckBackend: Send {
    fn state(&self) -> SoundcheckState;
    /// Refresh actual playback telemetry; fake backends may omit it.
    fn telemetry(&mut self) -> Pin<Box<dyn Future<Output = Option<Value>> + Send + '_>> {
        Box::pin(async { None })
    }
    /// Reserve a handle before dispatching an enter operation. Repeated calls
    /// while reserved or active return the same handle. Cancellation of a
    /// reservation applies to that operation; after it finishes, a new call
    /// reserves a fresh epoch. Old handles never affect a later epoch. A
    /// termination report means no helper exists or its process has been reaped,
    /// not merely that cancellation was requested.
    fn cancellation_handle(&self) -> CancellationHandle {
        CancellationHandle::default()
    }
    fn available(&self) -> bool {
        false
    }
    fn unavailable_reason(&self) -> Option<String> {
        None
    }
    fn configure(
        &mut self,
        _helper: Option<std::path::PathBuf>,
        _device: Option<openfader_audio::DeviceInfo>,
    ) {
    }
    fn command<'a>(
        &'a mut self,
        action: &'a str,
        args: &'a Value,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>>;
}
