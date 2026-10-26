//! Captured destination-wide source action, not an assumed master-word relay.
//!
//! Four independent native Pre Fader captures (2026-10-26) change exactly
//! 33 words: CH1/2/4..32, stereo return, and send master. CH3 is unchanged
//! even from a Post Fader baseline; its omission is intentional. The reason
//! is not established, so do not expand this bank without another capture.
use openfader_core::destinations::DestinationValue;
use openfader_transport::mackie::MackieSession;
use std::collections::BTreeMap;

pub(super) fn addresses(slot: u16) -> Vec<u32> {
    let offset = 2 * (u32::from(slot) - 1);
    (0..crate::addrs::INPUT_COUNT)
        .filter(|index| *index != 2)
        .map(|index| 76 + crate::addrs::INPUT_STRIDE * index + offset)
        .chain([3456 + offset, 3585 + 12 * (u32::from(slot) - 1)])
        .collect()
}

#[derive(Debug, Default)]
pub(super) struct SourceState {
    // Acknowledged intent and genuine desk proof remain distinct per word.
    words: BTreeMap<u32, (u32, bool)>,
    revision: u64,
    writing: bool,
}

impl SourceState {
    pub(super) fn seed(slot: u16, session: &MackieSession) -> Self {
        Self {
            words: addresses(slot)
                .into_iter()
                .filter_map(|address| {
                    session
                        .cached_verified(address)
                        .map(|raw| (address, (raw, true)))
                })
                .collect(),
            ..Self::default()
        }
    }

    pub(super) fn ingest(&mut self, slot: u16, start: u32, raws: &[u32]) {
        for address in addresses(slot) {
            if let Some(raw) = address
                .checked_sub(start)
                .and_then(|offset| raws.get(offset as usize))
            {
                self.words.insert(address, (*raw, true));
                self.revision = self.revision.wrapping_add(1);
            }
        }
    }

    pub(super) fn current(&self, slot: u16) -> Option<DestinationValue> {
        if self.writing {
            return None;
        }
        let addresses = addresses(slot);
        let (raw, _) = self.words.get(addresses.last()?)?;
        if *raw > 1 {
            return None;
        }
        let mut verified = true;
        for address in addresses {
            let (word, proof) = self.words.get(&address)?;
            if word != raw {
                return None;
            }
            verified &= proof;
        }
        Some(DestinationValue {
            value: *raw as f32,
            verified,
        })
    }

    pub(super) fn begin(
        &mut self,
        slot: u16,
        expected: u32,
        session: &MackieSession,
    ) -> anyhow::Result<u64> {
        anyhow::ensure!(!self.writing, "FX source write already pending.");
        for address in addresses(slot) {
            let (raw, _) = self.words.get(&address).ok_or_else(|| {
                anyhow::anyhow!("FX source bank incomplete; refresh the connection.")
            })?;
            // cached() permits immediate reversal of a fully acknowledged own
            // action. It is a comparison guard, never a claim of desk proof.
            anyhow::ensure!(
                *raw == expected && session.cached(address) == Some(*raw),
                "FX source bank changed; reopen the control."
            );
        }
        self.writing = true;
        for (_, proof) in self.words.values_mut() {
            *proof = false;
        }
        Ok(self.revision)
    }

    pub(super) fn finish(&mut self, slot: u16, revision: u64, value: u32, accepted: bool) {
        self.writing = false;
        if accepted && self.revision == revision {
            self.words = addresses(slot)
                .into_iter()
                .map(|address| (address, (value, false)))
                .collect();
        } else {
            // Timeout, cancellation/reconnect or a racing desk action cannot
            // leave a partially updated bank looking accepted or verified.
            // Withhold until fresh pushes cover the complete action again.
            self.words.clear();
        }
    }
}
