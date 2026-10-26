//! Read-only output levels keyed by provider destination identity, never strip numbers.

#[derive(Clone, Debug, PartialEq)]
pub struct OutputMeterFrame {
    pub target: String,
    /// Linear amplitude, mono or ordered L/R. No post-fader position is implied.
    pub levels: Vec<f32>,
    /// Family/order has structural evidence but still needs physical correlation.
    pub inferred: bool,
}

#[derive(Clone, Debug)]
pub struct OutputMeterSnapshot {
    pub generation: u64,
    /// Age of the original sample, so relaying a cache never renews freshness.
    pub age_ms: u64,
    pub frames: Vec<OutputMeterFrame>,
}
