//! Provider-described solo scope and honest partial clear semantics.
use super::*;
impl Engine {
    fn solo_real_strips(&self, target: &str, control: &str) -> Vec<u16> {
        self.active_drv()
            .topology()
            .into_iter()
            .flat_map(|f| {
                let (key, id) = match f.key {
                    "mix" => ("bus", "solo"),
                    "ret" => ("return", "solo"),
                    "dca" => ("vca", "solo"),
                    "fxsnd" => ("fx", "send_solo"),
                    "fxret" => ("fx", "return_solo"),
                    key => (key, "solo"),
                };
                (0..f.count).filter_map(move |i| {
                    (id == control && openfader_core::strip_key(key, i as u16 + 1) == target)
                        .then_some(f.first as u16 + i as u16)
                })
            })
            .collect()
    }
    pub(super) fn check_solo_scope(&self, targets: &[String], writer: &str) -> anyhow::Result<()> {
        anyhow::ensure!(!targets.is_empty(), "Solo scope unavailable");
        let drv = self.active_drv();
        let topology = drv.topology();
        let policy = self.policy_of(writer);
        for target in targets {
            let strips = topology
                .iter()
                .flat_map(|f| {
                    (0..f.count).map(move |i| {
                        (
                            openfader_core::strip_key(
                                match f.key {
                                    "mix" => "bus",
                                    "ret" => "return",
                                    "fxsnd" | "fxret" => "fx",
                                    "dca" => "vca",
                                    key => key,
                                },
                                i as u16 + 1,
                            ),
                            f.first as u16 + i as u16,
                            f.kind,
                        )
                    })
                })
                .filter(|(key, _, _)| key == target)
                .map(|(_, ch, kind)| (ch, kind))
                .collect::<Vec<_>>();
            if !strips.is_empty() {
                if strips
                    .iter()
                    .any(|(_, kind)| *kind != openfader_core::StripKind::In)
                {
                    policy.allow_master()?;
                }
                for (ch, _) in strips {
                    policy.allow_strip(StripId(ch))?;
                    anyhow::ensure!(
                        self.lock_holder(ch).is_none_or(|h| h == writer),
                        "Solo scope locked"
                    );
                }
            } else {
                policy.allow_master()?;
                // Legacy locks have no non-input identity on this topology.
                // Protect the shared bank conservatively rather than aliasing IDs.
                for ch in 0..drv.strip_count() as u16 {
                    anyhow::ensure!(
                        self.lock_holder(ch).is_none_or(|h| h == writer),
                        "Solo shared scope locked"
                    );
                }
            }
        }
        Ok(())
    }
    pub(super) async fn clear_advertised_solos(
        &self,
        writer: &str,
        out: &mut Vec<Event>,
    ) -> anyhow::Result<Option<Event>> {
        let drv = self.active_drv();
        let driver = self.active_driver();
        let epoch = self.connection_gen.load(Ordering::Acquire);
        let mut rows: Vec<_> = drv
            .destination_controls()
            .into_iter()
            .filter(|r| r.surface == "solo")
            .collect();
        anyhow::ensure!(!rows.is_empty(), "Solo clear coverage unavailable");
        for row in &rows {
            self.check_solo_scope(&row.members, writer)?;
            anyhow::ensure!(
                row.current
                    .as_ref()
                    .is_some_and(|v| v.verified && matches!(v.value, 0. | 1.)),
                "Solo bank incomplete; clear not sent"
            );
        }
        let mut written = 0;
        let result: anyhow::Result<()> = async {
            for i in 0..rows.len() {
                if rows[i].current.as_ref().is_none_or(|v| v.value == 0.) {
                    continue;
                }
                let row = rows[i].clone();
                let current: Vec<_> = drv
                    .destination_controls()
                    .into_iter()
                    .filter(|r| r.surface == "solo")
                    .collect();
                anyhow::ensure!(current == rows, "Solo bank changed during clear");
                let block: Vec<_> = rows
                    .iter()
                    .filter(|r| r.target == row.target)
                    .cloned()
                    .collect();
                let context = || {
                    driver == self.active_driver()
                        && epoch == self.connection_gen.load(Ordering::Acquire)
                        && !self.reads_suspended.load(Ordering::Acquire)
                        && rows
                            .iter()
                            .all(|r| self.check_solo_scope(&r.members, writer).is_ok())
                };
                // A failed write/readback may have landed. Invalidate legacy
                // input cache before attempting; never replay a stale ON/OFF.
                for ch in self.solo_real_strips(&row.target, &row.id) {
                    self.param_cache
                        .write()
                        .unwrap()
                        .entry(ch)
                        .or_default()
                        .remove("solo");
                    self.sync_state
                        .lock()
                        .unwrap()
                        .typed
                        .remove(&format!("{ch}/solo"));
                }
                drv.set_target_solo(&row.target, &row.id, 0., 1., &block, &context)
                    .await?;
                written += 1;
                for ch in self.solo_real_strips(&row.target, &row.id) {
                    self.apply_inbound(ch, "solo", &ParamValue::Bool(false));
                    out.push(Event::Solo {
                        ch,
                        soloed: false,
                        key: self.active_strip_key(ch),
                    });
                }
                let next: Vec<_> = drv
                    .destination_controls()
                    .into_iter()
                    .filter(|r| r.surface == "solo")
                    .collect();
                anyhow::ensure!(
                    next.len() == rows.len()
                        && next.iter().zip(&rows).all(|(a, b)| a.target == b.target
                            && a.id == b.id
                            && a.members == b.members
                            && a.generation == b.generation
                            && (a.target == row.target && a.id == row.id
                                || a.current == b.current)),
                    "Solo bank changed during clear"
                );
                rows = next;
            }
            let current: Vec<_> = drv
                .destination_controls()
                .into_iter()
                .filter(|r| r.surface == "solo")
                .collect();
            anyhow::ensure!(
                current == rows
                    && rows.iter().all(|r| r
                        .current
                        .as_ref()
                        .is_some_and(|v| v.verified && v.value == 0.)),
                "Solo bank changed during clear"
            );
            anyhow::ensure!(
                driver == self.active_driver()
                    && epoch == self.connection_gen.load(Ordering::Acquire),
                "Solo connection changed"
            );
            Ok(())
        }
        .await;
        out.extend(self.destination_controls_message());
        if let Err(e) = result {
            anyhow::bail!("Clear solo partial/uncertain after {written} verified writes: {e}");
        }
        self.note_write(writer);
        Ok(None)
    }
}
