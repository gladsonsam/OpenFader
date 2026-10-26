//! Offline command arms.

use super::*;

impl Engine {
    pub(super) async fn apply_offline(
        &self,
        cmd: Cmd,
        writer: &str,
        out: &mut Vec<Event>,
        policy: &Policy,
    ) -> anyhow::Result<Option<Event>> {
        match cmd {
            Cmd::Offline { model } => {
                if !self.known(&model) {
                    anyhow::bail!(
                        "unknown driver {model:?}; expected {}",
                        self.expected_drivers()
                    );
                }
                {
                    let mut off = self.offline.write().unwrap();
                    off.active = true;
                    off.model = model.clone();
                    off.prev_driver = self.active_driver();
                    off.changes.clear();
                    off.pending_push = false;
                }
                self.invalidate_connection();
                self.reads_suspended.store(false, Ordering::Release);
                *self.active.write().unwrap() = "sim".to_string();
                self.health_strikes.store(0, Ordering::Release);
                {
                    let mut link = self.link.write().unwrap();
                    link.state = "offline".to_string();
                    link.name = format!("Offline ({model})");
                    link.ip = String::new();
                }
                self.post_switch(writer, out, true).await?;
                // post_switch broadcasts the pre-offline link frame; swap
                // it for the offline one.
                if let Some(last) = out
                    .iter_mut()
                    .rev()
                    .find(|e| matches!(e, Event::Link { .. }))
                {
                    *last = self.link_message_cached();
                }
                Ok(None)
            }
            Cmd::OfflinePush { confirm } => {
                if !confirm {
                    anyhow::bail!("offline_push needs confirm:true");
                }
                if !self.offline.read().unwrap().pending_push {
                    anyhow::bail!("no offline diff pending (connect with offline changes first)");
                }
                policy.allow_recall()?;
                self.offline_push_apply(writer, out).await?;
                self.note_write(writer);
                self.touch_holder_locks(writer);
                self.push_state_with_by(writer, out).await;
                Ok(None)
            }
            Cmd::OfflineDiscard => {
                {
                    let mut off = self.offline.write().unwrap();
                    off.active = false;
                    off.changes.clear();
                    off.pending_push = false;
                }
                out.push(self.offline_diff_message());
                Ok(None)
            }

            _ => unreachable!("command routed to the wrong family"),
        }
    }
}
