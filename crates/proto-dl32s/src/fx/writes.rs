//! Serialized FX writes with dispatch-time authority and complete bank witnesses.
use super::*;
use openfader_transport::mackie::{REQUEST_TIMEOUT, VALUE_TYPE_PARAM};

impl Dl32sDriver {
    pub(crate) async fn write_fx_guarded(
        &self,
        target: &str,
        control: &str,
        value: f32,
        expected: f32,
        snapshot: &[DestinationControl],
        context: &(dyn Fn() -> bool + Send + Sync),
    ) -> anyhow::Result<DestinationControl> {
        let slot = (1..=4)
            .find(|slot| target == format!("fx/{slot:02}"))
            .ok_or_else(|| anyhow::anyhow!("FX target unavailable"))?;
        let generation = self.generation.load(Ordering::Acquire);
        let _write = self.output_write.lock().await;
        let guard = self.session().await?;
        let session = guard.as_ref().expect("session checked");
        let addresses = banks::addresses(slot);
        let (before, operation, pairs) = {
            let mut native = self
                .native
                .lock()
                .map_err(|_| anyhow::anyhow!("FX cache unavailable"))?;
            anyhow::ensure!(
                context()
                    && session.is_alive()
                    && native.generation == generation
                    && native.view.connected
                    && self.generation.load(Ordering::Acquire) == generation,
                "FX authority or session changed"
            );
            let rows = native.fx.rows(slot, generation);
            anyhow::ensure!(
                rows == snapshot,
                "FX configuration/revision changed; reopen the control"
            );
            let row = rows
                .iter()
                .find(|r| r.id == control)
                .ok_or_else(|| anyhow::anyhow!("FX field unavailable"))?;
            anyhow::ensure!(
                row.allows_value(value)
                    && row
                        .current
                        .as_ref()
                        .is_some_and(|v| v.verified && v.value == expected),
                "FX value unavailable, unconfirmed or changed"
            );
            let before = native
                .fx
                .observed(slot)
                .ok_or_else(|| anyhow::anyhow!("FX complete value space unavailable"))?;
            let bank = banks::BANKS[slot as usize - 1];
            let algorithm = before[addresses
                .iter()
                .position(|a| *a == bank[0])
                .expect("selector")]
            .0;
            let pairs = if control == "algorithm" {
                anyhow::ensure!(
                    value.fract() == 0.0 && dl32s::algorithm(value as u32).is_some(),
                    "FX algorithm unavailable in this slot"
                );
                vec![(bank[0], value as u32)]
            } else if let Some(spec) = banks::common(slot).find(|s| s.id == control) {
                anyhow::ensure!(spec.allows(value), "FX value outside domain");
                if control == "send_source" {
                    source::addresses(slot)
                        .into_iter()
                        .map(|a| (a, value as u32))
                        .collect()
                } else {
                    vec![(spec.address, spec.raw_value(value))]
                }
            } else {
                let field = dl32s::algorithm(algorithm)
                    .and_then(|a| a.fields.iter().find(|f| f.id == control))
                    .ok_or_else(|| anyhow::anyhow!("FX algorithm field changed"))?;
                vec![(
                    bank[field.word],
                    field
                        .encode(value)
                        .ok_or_else(|| anyhow::anyhow!("FX encoding unavailable"))?,
                )]
            };
            let state = &mut native.fx.slots[slot as usize - 1];
            state.operation = state.operation.wrapping_add(1);
            state.after = addresses
                .iter()
                .zip(&before)
                .map(|(a, v)| (*a, v.2))
                .collect();
            (before, state.operation, pairs)
        };
        // This closure runs INSIDE the socket write lane, after any await for
        // competing writers. It includes complete raw/revision witnesses,
        // including hidden bank words and all 33 source-action members.
        let authorize = || {
            if !context()
                || !session.is_alive()
                || self.generation.load(Ordering::Acquire) != generation
            {
                return false;
            }
            self.native.lock().is_ok_and(|mut n| {
                if n.generation != generation
                    || !n.view.connected
                    || n.fx.slots[slot as usize - 1].operation != operation
                {
                    return false;
                }
                let Some(now) = n.fx.observed(slot) else {
                    return false;
                };
                if !now
                    .iter()
                    .zip(&before)
                    .all(|(a, b)| a.0 == b.0 && a.1 == b.1)
                {
                    return false;
                }
                // Equal-value pushes can arrive while queued for the socket.
                // They preserve the comparison guard but must NOT satisfy the
                // freshness barrier of a future that is cancelled after send.
                n.fx.slots[slot as usize - 1].after =
                    addresses.iter().zip(now).map(|(a, v)| (*a, v.2)).collect();
                true
            })
        };
        session
            .write_multi_guarded(&pairs, VALUE_TYPE_PARAM, &authorize)
            .await?;
        {
            let mut n = self
                .native
                .lock()
                .map_err(|_| anyhow::anyhow!("FX cache unavailable; result uncertain"))?;
            anyhow::ensure!(
                context()
                    && n.generation == generation
                    && n.view.connected
                    && self.generation.load(Ordering::Acquire) == generation,
                "FX session changed; result uncertain"
            );
            // Demand evidence after ACK, including unchanged siblings. ACKs
            // do not advance these sequences and never restore editability.
            n.fx.slots[slot as usize - 1].after = addresses
                .iter()
                .map(|a| (*a, session.observed_value_sequence(*a).unwrap_or(0)))
                .collect();
        }
        session.request_value_snapshot().await.map_err(|e| {
            anyhow::anyhow!("FX accepted; fresh read failed: {e}; result uncertain")
        })?;
        drop(guard);
        let deadline = tokio::time::Instant::now() + REQUEST_TIMEOUT;
        loop {
            anyhow::ensure!(
                context() && self.generation.load(Ordering::Acquire) == generation,
                "FX authority/session changed; result uncertain"
            );
            {
                let n = self
                    .native
                    .lock()
                    .map_err(|_| anyhow::anyhow!("FX cache unavailable"))?;
                anyhow::ensure!(
                    n.generation == generation
                        && n.view.connected
                        && n.fx.slots[slot as usize - 1].operation == operation,
                    "FX reservation changed; result uncertain"
                );
                if n.fx.ready(slot) {
                    let now =
                        n.fx.observed(slot)
                            .ok_or_else(|| anyhow::anyhow!("FX readback incomplete"))?;
                    for ((a, old), new) in addresses.iter().zip(&before).zip(&now) {
                        if let Some((_, raw)) = pairs.iter().find(|(p, _)| p == a) {
                            anyhow::ensure!(
                                new.0 == *raw
                                    && new.1 == old.1.wrapping_add(u64::from(old.0 != *raw)),
                                "FX readback differs or changed again; result uncertain"
                            );
                        } else if control != "algorithm"
                            || !banks::BANKS[slot as usize - 1][1..].contains(a)
                        {
                            anyhow::ensure!(
                                new.0 == old.0 && new.1 == old.1,
                                "FX sibling changed; result uncertain"
                            );
                        }
                    }
                    return n
                        .fx
                        .rows(slot, generation)
                        .into_iter()
                        .find(|r| {
                            r.id == control
                                && r.current
                                    .as_ref()
                                    .is_some_and(|v| v.verified && v.value == value)
                        })
                        .ok_or_else(|| {
                            anyhow::anyhow!("FX readback encoding unavailable; result uncertain")
                        });
                }
            }
            anyhow::ensure!(
                tokio::time::Instant::now() < deadline,
                "FX accepted without fresh complete mixer readback; result uncertain"
            );
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }
}
