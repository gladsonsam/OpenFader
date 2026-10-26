//! Client command arms.

use super::*;

impl Engine {
    pub(super) async fn apply_client(
        &self,
        cmd: Cmd,
        writer: &str,
        out: &mut Vec<Event>,
    ) -> anyhow::Result<Option<Event>> {
        match cmd {
            Cmd::SpectrumTarget { ch } => {
                if ch.is_some() && self.reads_suspended.load(Ordering::Acquire) {
                    anyhow::bail!("RTA unavailable while disconnected");
                }
                if let Some(ch) = ch {
                    anyhow::ensure!(
                        (ch as usize) < self.active_drv().strip_count(),
                        "RTA strip out of range"
                    );
                }
                self.active_drv()
                    .set_spectrum_target(writer, ch.map(StripId))
                    .await?;
                Ok(None)
            }
            Cmd::Focus { ch, chs } => {
                // Poll-priority hint: either `{"cmd":"focus","ch":N}` or
                // `{"cmd":"focus","chs":[...]}`. Entries may be indices or
                // stable strip keys (`ch/01`, …, `main`; `mix/..` accepted).
                // Valid channels move to the front of the poll queue
                // (re-read at the normal budget, then round-robin resumes);
                // unknown/out-of-range channels are ignored with an `error`
                // event, never a disconnect.
                let mut want: Vec<u16> = Vec::new();
                match (ch, chs) {
                    (_, Some(list)) => {
                        want = self.resolve_focus_list(&list)?;
                    }
                    (Some(n), None) => want.push(n),
                    (None, None) => {
                        anyhow::bail!("focus needs \"ch\" (integer) or \"chs\" (array of integers)")
                    }
                }
                let nch = self.driver_strips(&self.active_driver());
                let valid: Vec<u16> = want
                    .iter()
                    .copied()
                    .filter(|c| (*c as usize) < nch)
                    .collect();
                let ignored: Vec<u16> = want
                    .iter()
                    .copied()
                    .filter(|c| (*c as usize) >= nch)
                    .collect();
                if valid.is_empty() {
                    anyhow::bail!("focus channel(s) out of range 0..{nch}");
                }
                self.request_focus(writer, &valid);
                if ignored.is_empty() {
                    Ok(None)
                } else {
                    anyhow::bail!("focus ignored out-of-range channel(s): {ignored:?}")
                }
            }
            Cmd::Lock { ch, key } => {
                let ch = self.resolve_strip(ch, key.as_deref())?;
                if let Some(holder) = self.lock_holder(ch) {
                    if holder != writer {
                        return Ok(Some(Self::locked_error(ch, &holder)));
                    }
                    // Already ours: refresh expiry below.
                }
                {
                    let mut locks = self.locks.write().unwrap();
                    locks.insert(
                        ch,
                        StripLock {
                            holder: writer.to_string(),
                            expires_at: Instant::now() + self.lock_ttl,
                        },
                    );
                }
                self.note_write(writer);
                self.touch_holder_locks(writer);
                out.push(self.locks_message());
                Ok(None)
            }
            Cmd::Unlock { ch, key } => {
                let ch = self.resolve_strip(ch, key.as_deref())?;
                if let Some(holder) = self.lock_holder(ch) {
                    if holder != writer {
                        return Ok(Some(Self::locked_error(ch, &holder)));
                    }
                    {
                        let mut locks = self.locks.write().unwrap();
                        locks.remove(&ch);
                    }
                    self.note_write(writer);
                    self.touch_holder_locks(writer);
                    out.push(self.locks_message());
                }
                // Unlocking an unlocked/expired strip is a no-op success.
                Ok(None)
            }
            // MR1 realtime negotiation: store the per-client binary-meter
            // preference and unicast the current value back (never
            // broadcast). Missing `binary_meters` just re-acks.
            // R2: `taps` works the same way for meter taps.
            Cmd::Hello {
                binary_meters,
                taps,
            } => {
                if let Some(v) = binary_meters {
                    if let Some(entry) = self.clients.write().unwrap().get_mut(writer) {
                        entry.binary_meters = v;
                    }
                }
                if let Some(v) = taps {
                    if let Some(entry) = self.clients.write().unwrap().get_mut(writer) {
                        entry.taps = v;
                    }
                }
                // Slim-caps completion (see `caps_slim_message`): attach
                // queues only the essentials lane so first paint never waits
                // for the ~30 KB catalogue. Every transport sends hello
                // right after subscribing, so the first hello unicasts the
                // full catalogue ahead of any detail UI; later hellos are
                // pure re-acks. Same frame shape as a param-drop caps
                // rebroadcast (idempotent replace for every consumer).
                if self
                    .clients
                    .read()
                    .unwrap()
                    .get(writer)
                    .is_some_and(|e| !e.caps_full)
                {
                    self.unicast(writer, self.caps_message());
                    if let Some(entry) = self.clients.write().unwrap().get_mut(writer) {
                        entry.caps_full = true;
                    }
                }
                Ok(Some(Event::Hello {
                    binary_meters: self.client_binary_meters(writer),
                    taps: self.client_taps(writer),
                }))
            }

            _ => unreachable!("command routed to the wrong family"),
        }
    }
}
