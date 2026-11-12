//! DL32S static SOLO bank: type2 PID23..84, Bool 0/1.
//! Desk-verified mapping. No SIP/Main mapping.
use crate::Dl32sDriver;
use openfader_core::destinations::{DestinationControl, DestinationValue};
use openfader_transport::mackie::{
    MackieSession, REQUEST_TIMEOUT, VALUE_TYPE_MUTE_GROUP, VALUE_TYPE_PARAM,
};
use std::sync::atomic::Ordering;

fn specs() -> Vec<DestinationControl> {
    openfader_core::solo::controls()
}
fn index(target: &str, id: &str) -> Option<usize> {
    specs()
        .iter()
        .position(|r| r.target == target && r.id == id)
}
fn pair(i: usize) -> Option<(usize, [u32; 2])> {
    let (first, base, stride, offset) = match i {
        0..32 => (0, 1, 106, 11),
        32..34 => (32, 3393, 94, 11),
        42..48 => (42, 4041, 51, 6),
        48..56 => (48, 4434, 90, 7),
        _ => return None,
    };
    let left = first + (i - first) / 2 * 2;
    Some((
        left,
        [
            base + (left - first) as u32 * stride + offset,
            base + (left - first + 1) as u32 * stride + offset,
        ],
    ))
}
#[derive(Debug, Default)]
pub(super) struct SoloState {
    values: Vec<Option<u32>>,
    revisions: Vec<u64>,
    observations: Vec<u64>,
    configuration: [Option<u32>; 2],
    links: std::collections::BTreeMap<u32, Option<u32>>,
}
impl SoloState {
    pub(super) fn seed(&mut self, session: &MackieSession) {
        self.configuration = [
            session.cached_type2_verified(1),
            session.cached_type2_verified(2),
        ];
        self.values = (23..=84)
            .map(|a| session.cached_type2_verified(a).filter(|v| *v <= 1))
            .collect();
        self.revisions = vec![0; 62];
        self.observations = vec![0; 62];
        self.links = (0..62)
            .filter_map(pair)
            .flat_map(|(_, a)| a)
            .map(|a| (a, session.cached_verified(a).filter(|v| *v <= 1)))
            .collect();
    }
    pub(super) fn ingest(&mut self, typ: u8, start: u32, words: &[u32]) {
        if self.values.len() != 62 {
            return;
        }
        self.observations.resize(62, 0);
        if typ == VALUE_TYPE_MUTE_GROUP {
            for i in 0..2 {
                if let Some(word) = (i as u32 + 1)
                    .checked_sub(start)
                    .and_then(|n| words.get(n as usize))
                {
                    if self.configuration[i] != Some(*word) {
                        self.configuration[i] = Some(*word);
                        for revision in &mut self.revisions {
                            *revision = revision.wrapping_add(1);
                        }
                    }
                }
            }
            for i in 0..62 {
                if let Some(word) = (23 + i as u32)
                    .checked_sub(start)
                    .and_then(|n| words.get(n as usize))
                {
                    self.observations[i] = self.observations[i].wrapping_add(1);
                    let value = (*word <= 1).then_some(*word);
                    if self.values[i] != value {
                        self.values[i] = value;
                        self.revisions[i] = self.revisions[i].wrapping_add(1);
                        if let Some((left, _)) = pair(i) {
                            let peer = if i == left { left + 1 } else { left };
                            self.revisions[peer] = self.revisions[peer].wrapping_add(1);
                        }
                    }
                }
            }
        } else if typ == VALUE_TYPE_PARAM {
            for (&address, old) in &mut self.links {
                if let Some(word) = address
                    .checked_sub(start)
                    .and_then(|n| words.get(n as usize))
                {
                    let value = (*word <= 1).then_some(*word);
                    if *old != value {
                        *old = value;
                        for (i, revision) in self.revisions.iter_mut().enumerate() {
                            if pair(i).is_some_and(|(_, a)| a.contains(&address)) {
                                *revision = revision.wrapping_add(1);
                            }
                        }
                    }
                }
            }
        }
    }
    pub(super) fn controls(&self, generation: u64) -> Vec<DestinationControl> {
        let mut rows = specs();
        for i in 0..rows.len() {
            rows[i].generation = Some(generation);
            rows[i].revision = self.revisions.get(i).copied();
            rows[i].current = self
                .values
                .get(i)
                .copied()
                .flatten()
                .map(|v| DestinationValue {
                    value: v as f32,
                    verified: true,
                });
            if let Some((left, addresses)) = pair(i) {
                // Native isLinked ORs both directions. Missing/pending stays
                // conservative; link guards are revisions, never link writes.
                if !addresses
                    .iter()
                    .all(|a| self.links.get(a) == Some(&Some(0)))
                {
                    rows[i].members =
                        vec![rows[left].target.clone(), rows[left + 1].target.clone()];
                }
            }
        }
        rows
    }
}
impl Dl32sDriver {
    pub async fn write_target_solo(
        &self,
        target: &str,
        id: &str,
        value: f32,
        expected: f32,
        snapshot: &[DestinationControl],
        context: &(dyn Fn() -> bool + Send + Sync),
    ) -> anyhow::Result<DestinationControl> {
        let i = index(target, id).ok_or_else(|| anyhow::anyhow!("Solo target unsupported"))?;
        anyhow::ensure!(
            matches!(value, 0. | 1.) && matches!(expected, 0. | 1.),
            "Solo outside Bool domain"
        );
        let _write = self.output_write.lock().await;
        let generation = self.generation.load(Ordering::Acquire);
        let guard = self.session().await?;
        let session = guard.as_ref().expect("checked");
        let validate = |invalidate: bool| {
            if !context()
                || self.generation.load(Ordering::Acquire) != generation
                || !session.is_alive()
            {
                return false;
            }
            let Ok(mut native) = self.native.lock() else {
                return false;
            };
            if native.generation != generation || !native.view.connected {
                return false;
            }
            let rows = native.solos.controls(generation);
            let block: Vec<_> = rows
                .iter()
                .filter(|r| r.target == target)
                .cloned()
                .collect();
            let valid = snapshot == block
                && rows[i]
                    .current
                    .as_ref()
                    .is_some_and(|v| v.verified && v.value == expected)
                && session.cached_type2_verified(23 + i as u32) == Some(expected as u32)
                && native
                    .solos
                    .configuration
                    .iter()
                    .enumerate()
                    .all(|(n, raw)| {
                        raw.is_some_and(|v| v <= 2)
                            && session.cached_type2_verified(n as u32 + 1) == *raw
                    })
                && pair(i).is_none_or(|(left, a)| {
                    a.iter().all(|a| {
                        session.cached_verified(*a).is_some_and(|v| v <= 1)
                            && native.solos.links.get(a) == Some(&session.cached_verified(*a))
                    }) && [left, left + 1].iter().all(|peer| {
                        native.solos.values[*peer].is_some()
                            && session.cached_type2_verified(23 + *peer as u32)
                                == native.solos.values[*peer]
                    })
                });
            if valid && invalidate {
                native.solos.values[i] = None;
                native.solos.revisions[i] = native.solos.revisions[i].wrapping_add(1);
            }
            valid
        };
        anyhow::ensure!(
            validate(false),
            "Solo session, scope or value changed; reopen control"
        );
        // Repeat the exact snapshot/policy/lock/link checks inside socket lane.
        session
            .write_int_guarded(23 + i as u32, value as u32, VALUE_TYPE_MUTE_GROUP, &|| {
                validate(true)
            })
            .await?;
        let mut reads: std::collections::BTreeSet<usize> = snapshot
            .iter()
            .filter_map(|r| index(&r.target, &r.id))
            .collect();
        if let Some((left, _)) = pair(i) {
            reads.extend([left, left + 1]);
        }
        let (readback_revision, observed) = {
            let mut native = self
                .native
                .lock()
                .map_err(|_| anyhow::anyhow!("Solo cache unavailable; result uncertain"))?;
            anyhow::ensure!(
                native.generation == generation,
                "Solo connection changed; result uncertain"
            );
            native.solos.values[i] = None;
            native.solos.revisions[i] = native.solos.revisions[i].wrapping_add(1);
            (
                native.solos.revisions[i].wrapping_add(1),
                reads
                    .iter()
                    .map(|n| (*n, native.solos.observations[*n]))
                    .collect::<Vec<_>>(),
            )
        };
        // Confirm the conservative pair/FX block independently too. Empty
        // ACKs or an old cached peer word cannot establish scope readback.
        for read in &reads {
            session
                .request_global_value(23 + *read as u32)
                .await
                .map_err(|e| {
                    anyhow::anyhow!("Solo write accepted; fresh read failed: {e}; result uncertain")
                })?;
        }
        drop(guard);
        let deadline = tokio::time::Instant::now() + REQUEST_TIMEOUT;
        loop {
            anyhow::ensure!(
                context() && self.generation.load(Ordering::Acquire) == generation,
                "Solo context changed; result uncertain"
            );
            {
                let native = self
                    .native
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Solo cache unavailable"))?;
                anyhow::ensure!(
                    native.generation == generation && native.view.connected,
                    "Solo connection changed; result uncertain"
                );
                let rows = native.solos.controls(generation);
                if let Some(current) = &rows[i].current {
                    anyhow::ensure!(
                        current.verified
                            && current.value == value
                            && rows[i].revision == Some(readback_revision),
                        "Solo readback differs; result uncertain"
                    );
                    anyhow::ensure!(
                        snapshot
                            .iter()
                            .filter(|r| r.id != id)
                            .all(|r| rows.contains(r)),
                        "Solo sibling changed; result uncertain"
                    );
                    // Link membership must still match the captured scope.
                    anyhow::ensure!(
                        snapshot
                            .iter()
                            .find(|r| r.id == id)
                            .is_some_and(|r| r.members == rows[i].members),
                        "Solo scope changed; result uncertain"
                    );
                    if observed.iter().all(|(n, before)| {
                        native.solos.observations[*n] > *before && rows[*n].current.is_some()
                    }) {
                        return Ok(rows[i].clone());
                    }
                }
            }
            anyhow::ensure!(
                tokio::time::Instant::now() < deadline,
                "Solo accepted without fresh mixer readback; result uncertain"
            );
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }
}
