//! Read-only output levels keyed by provider destination identity, never strip numbers.

#[derive(Clone, Debug, PartialEq)]
pub struct OutputMeterFrame {
    pub target: String,
    /// Linear amplitude, mono or ordered L/R. No post-fader position is implied.
    pub levels: Vec<f32>,
    /// Family/order has structural evidence but still needs physical correlation.
    pub inferred: bool,
    /// None means unsupported. Some with no samples means known but unverified
    /// or unavailable; never substitute zero reduction for missing evidence.
    pub compressor_gr: Option<OutputCompressorGr>,
}

#[derive(Clone, Debug)]
pub struct OutputMeterSnapshot {
    pub generation: u64,
    /// Age of the original sample, so relaying a cache never renews freshness.
    pub age_ms: u64,
    pub frames: Vec<OutputMeterFrame>,
}

/// Provider-normalized positive attenuation in dB, mono or ordered L/R.
/// A provider may only populate samples after proving wire layout AND scale.
#[derive(Clone, Debug, PartialEq)]
pub struct OutputCompressorGr {
    pub reduction_db: Option<Vec<f32>>,
    pub inferred: bool,
}
