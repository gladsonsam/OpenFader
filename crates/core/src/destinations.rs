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
}

#[derive(Clone, Debug, PartialEq)]
pub struct DestinationControl {
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
    pub choices: Vec<DestinationChoice>,
    pub range: Option<DestinationRange>,
    pub current: Option<DestinationValue>,
}

impl DestinationControl {
    pub fn allows_value(&self, value: f32) -> bool {
        value.is_finite() && (self.choices.iter().any(|choice| choice.value == value)
            || self.range.as_ref().is_some_and(|range| {
                range.min.is_finite() && range.max.is_finite() && range.min < range.max
                    && (range.min..=range.max).contains(&value)
            }))
    }
}
