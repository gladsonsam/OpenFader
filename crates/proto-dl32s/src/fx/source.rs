//! Captured destination-wide source action, not an assumed master-word relay.
//!
//! Four independent native Pre Fader captures (2026-10-26) change exactly
//! 33 words: CH1/2/4..32, stereo return, and send master. CH3 is unchanged
//! even from a Post Fader baseline; its omission is intentional. The reason
//! is not established, so do not expand this bank without another capture.

pub(super) fn addresses(slot: u16) -> Vec<u32> {
    let offset = 2 * (u32::from(slot) - 1);
    (0..crate::addrs::INPUT_COUNT)
        .filter(|index| *index != 2)
        .map(|index| 76 + crate::addrs::INPUT_STRIDE * index + offset)
        .chain([3456 + offset, 3585 + 12 * (u32::from(slot) - 1)])
        .collect()
}
