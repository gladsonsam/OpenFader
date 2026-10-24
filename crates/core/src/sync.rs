//! Offline-first op queue: record while disconnected, replay on reconnect.
//!
//! While the link is down, UI layers record [`PendingOp`]s instead of
//! touching the driver. On reconnect [`OpQueue::replay`] drains the queue
//! in order with last-write-wins per target (a later fader move supersedes
//! an earlier one on the same strip) and appends every superseded op to a
//! conflict log for inspection.

use std::collections::{HashMap, VecDeque};

use crate::{validate_fader, MixerDriver, ParamValue, StripId};

/// What a queued op targets. Last-write-wins coalescing is per key.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum OpTarget {
    Fader,
    Mute,
    Param(String),
}

impl OpTarget {
    fn describe(&self, strip: StripId) -> String {
        match self {
            OpTarget::Fader => format!("fader ch{}", strip.0),
            OpTarget::Mute => format!("mute ch{}", strip.0),
            OpTarget::Param(p) => format!("param {p:?} ch{}", strip.0),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct OpKey {
    strip: u16,
    target: OpTarget,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PendingOp {
    /// Monotonic sequence number (enqueue order).
    pub seq: u64,
    pub strip: StripId,
    pub kind: PendingOpKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PendingOpKind {
    Fader { value: f32 },
    Mute { muted: bool },
    Param { param: String, value: ParamValue },
}

impl PendingOp {
    fn key(&self) -> OpKey {
        let target = match &self.kind {
            PendingOpKind::Fader { .. } => OpTarget::Fader,
            PendingOpKind::Mute { .. } => OpTarget::Mute,
            PendingOpKind::Param { param, .. } => OpTarget::Param(param.clone()),
        };
        OpKey {
            strip: self.strip.0,
            target,
        }
    }

    #[cfg(test)]
    fn describe(&self) -> String {
        let what = match &self.kind {
            PendingOpKind::Fader { value } => format!("fader={value}"),
            PendingOpKind::Mute { muted } => format!("muted={muted}"),
            PendingOpKind::Param { param, .. } => format!("param {param:?}"),
        };
        format!("#{} ch{} {what}", self.seq, self.strip.0)
    }

    async fn apply<D: MixerDriver + Sync + ?Sized>(&self, driver: &D) -> anyhow::Result<()> {
        match &self.kind {
            PendingOpKind::Fader { value } => driver.set_fader(self.strip, *value).await,
            PendingOpKind::Mute { muted } => driver.set_mute(self.strip, *muted).await,
            PendingOpKind::Param { param, value } => {
                driver.set_param(self.strip, param, value.clone()).await
            }
        }
    }
}

/// A superseded op, kept for UI conflict display / logs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    pub target: String,
    pub dropped_seq: u64,
    pub kept_seq: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncReport {
    pub applied: usize,
    pub coalesced: usize,
    /// Cumulative conflict log (all replays so far).
    pub conflicts: Vec<Conflict>,
}

#[derive(Clone, Debug, Default)]
pub struct OpQueue {
    ops: VecDeque<PendingOp>,
    next_seq: u64,
    conflicts: Vec<Conflict>,
}

impl OpQueue {
    pub fn new() -> Self {
        Self::default()
    }

    fn enqueue(&mut self, strip: StripId, kind: PendingOpKind) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.ops.push_back(PendingOp { seq, strip, kind });
        seq
    }

    /// Record a fader move. Fails fast on invalid values.
    pub fn push_fader(&mut self, strip: StripId, value: f32) -> anyhow::Result<u64> {
        validate_fader(value)?;
        Ok(self.enqueue(strip, PendingOpKind::Fader { value }))
    }

    pub fn push_mute(&mut self, strip: StripId, muted: bool) -> u64 {
        self.enqueue(strip, PendingOpKind::Mute { muted })
    }

    pub fn push_param(
        &mut self,
        strip: StripId,
        param: impl Into<String>,
        value: ParamValue,
    ) -> u64 {
        self.enqueue(
            strip,
            PendingOpKind::Param {
                param: param.into(),
                value,
            },
        )
    }

    pub fn len(&self) -> usize {
        self.ops.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    pub fn pending(&self) -> &VecDeque<PendingOp> {
        &self.ops
    }

    pub fn conflicts(&self) -> &[Conflict] {
        &self.conflicts
    }

    /// Drain the queue onto `driver` in enqueue order with last-write-wins
    /// per target: only the latest op per (strip, target) is applied, the
    /// rest are logged as [`Conflict`]s. The queue is empty afterwards,
    /// even if an apply fails (applied ops stay applied).
    pub async fn replay<D: MixerDriver + Sync + ?Sized>(
        &mut self,
        driver: &D,
    ) -> anyhow::Result<SyncReport> {
        let mut winners: HashMap<OpKey, PendingOp> = HashMap::new();
        let conflicts_before = self.conflicts.len();
        for op in self.ops.drain(..) {
            let key = op.key();
            if let Some(prev) = winners.insert(key.clone(), op.clone()) {
                self.conflicts.push(Conflict {
                    target: key.target.describe(op.strip),
                    dropped_seq: prev.seq,
                    kept_seq: op.seq,
                });
            }
        }
        let mut final_ops: Vec<PendingOp> = winners.into_values().collect();
        final_ops.sort_by_key(|op| op.seq);
        for op in &final_ops {
            op.apply(driver).await?;
        }
        let applied = final_ops.len();
        let coalesced = self.conflicts.len() - conflicts_before;
        Ok(SyncReport {
            applied,
            coalesced,
            conflicts: self.conflicts.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_fader_rejected_at_enqueue() {
        let mut q = OpQueue::new();
        assert!(q.push_fader(StripId(0), f32::NAN).is_err());
        assert!(q.push_fader(StripId(0), 2.0).is_err());
        assert!(q.is_empty());
    }

    #[test]
    fn op_describe_is_human_readable() {
        let op = PendingOp {
            seq: 7,
            strip: StripId(3),
            kind: PendingOpKind::Mute { muted: true },
        };
        assert!(op.describe().contains("#7"));
    }
}
