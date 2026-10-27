//! Controls shared by every send feeding a destination, separate from
//! strip-local send parameters. Providers own target IDs and value domains.

#[derive(Clone, Debug, PartialEq)]
pub struct DestinationChoice {
    pub value: f32,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DestinationValue {
    pub value: f32,
    /// An accepted write is display continuity, never mixer readback.
    pub verified: bool,
}

/// Continuous engineering-unit domain; never an implicit normalized law.
#[derive(Clone, Debug, PartialEq)]
pub struct DestinationRange {
    pub min: f32,
    pub max: f32,
    pub unit: String,
    pub decimals: u8,
    /// Optional quantization step in engineering units, relative to min.
    pub step: Option<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DestinationControl {
    /// Monotonic observed-state revision, required for guarded processor recall.
    pub revision: Option<u64>,
    /// Provider connection generation; prevents revision reuse after reconnect.
    pub generation: Option<u64>,
    /// Provider opt-in: local processor snapshots may store this control.
    pub preset: bool,
    /// Conservative affected stable targets; never synthetic input strip IDs.
    pub members: Vec<String>,
    pub target: String,
    pub target_label: String,
    pub target_name: Option<String>,
    /// Corresponding 1-based generic send slot, when one exists.
    pub send_slot: Option<u16>,
    /// Generic presentation surface, e.g. `send` or `fx`.
    pub surface: String,
    /// Provider describes the scope/impact of this shared control.
    pub description: String,
    pub id: String,
    pub label: String,
    /// This numeric control accepts a tapped period in ms/s, or a tempo
    /// in BPM. Providers opt in only where tapping is supported.
    pub tap_tempo: bool,
    /// Explicit provider opt-in to local press/release semantics. `talkback`
    /// uses binary 0/1 and requires a verified guarded-write receipt.
    pub gesture: Option<&'static str>,
    pub choices: Vec<DestinationChoice>,
    pub range: Option<DestinationRange>,
    pub current: Option<DestinationValue>,
}

impl DestinationControl {
    pub fn allows_value(&self, value: f32) -> bool {
        value.is_finite()
            && (self.choices.iter().any(|choice| choice.value == value)
                || self.range.as_ref().is_some_and(|range| {
                    range.min.is_finite()
                        && range.max.is_finite()
                        && range.min < range.max
                        && (range.min..=range.max).contains(&value)
                        && range.step.is_none_or(|step| {
                            step.is_finite()
                                && step > 0.0
                                && (((value - range.min) / step).round()
                                    - (value - range.min) / step)
                                    .abs()
                                    < 1e-4
                        })
                }))
    }
}
