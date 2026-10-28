//! Fresh stereo link configuration values.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StereoLinkSettings {
    /// Bus pairs 1–2, 3–4, and 5–6.
    pub bus_links: [bool; 3],
    /// Preamp, EQ, dynamics, and fader/mute policies.
    pub policies: [bool; 4],
}
