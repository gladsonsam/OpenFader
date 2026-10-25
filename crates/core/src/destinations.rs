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

#[derive(Clone, Debug, PartialEq)]
pub struct DestinationControl {
    pub target: String,
    pub target_label: String,
    pub target_name: Option<String>,
    /// Corresponding 1-based generic send slot, when one exists.
    pub send_slot: Option<u16>,
    pub id: String,
    pub label: String,
    pub choices: Vec<DestinationChoice>,
    pub current: Option<DestinationValue>,
}
