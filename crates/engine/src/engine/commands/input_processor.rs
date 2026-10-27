//! Correlated input processor capture/write results, separate from preamp guards.
use super::*;
impl Engine {
    fn input_processor_context(
        &self,
        ch: u16,
        processor: &str,
        epoch: u64,
    ) -> anyhow::Result<Value> {
        let Event::Caps {
            driver,
            params,
            topology,
            stereo_pairs,
            input_processor_guard,
            ..
        } = self.caps_message()
        else {
            unreachable!()
        };
        anyhow::ensure!(
            input_processor_guard,
            "Guarded input processor recall unavailable for this provider"
        );
        let params: Vec<_> = params
            .into_iter()
            .filter(|p| openfader_core::input_processors::member(processor, &p.id))
            .collect();
        anyhow::ensure!(!params.is_empty(), "Input processor catalogue unavailable");
        Ok(
            json!({"driver":driver,"epoch":epoch,"key":self.active_strip_key(ch),"params":params,"topology":topology,"stereo_pairs":stereo_pairs}),
        )
    }
    fn input_processor_access(&self, ch: u16, epoch: u64, writer: &str) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.reads_suspended.load(Ordering::Acquire)
                && epoch == self.connection_gen.load(Ordering::Acquire),
            "Input connection changed; result uncertain"
        );
        let policy = self.policy_of(writer);
        for member in self
            .active_drv()
            .stereo_pairs()
            .into_iter()
            .find(|p| p.contains(&ch))
            .map(Vec::from)
            .unwrap_or_else(|| vec![ch])
        {
            policy.allow_strip(StripId(member))?;
            anyhow::ensure!(
                self.lock_holder(member)
                    .is_none_or(|holder| holder == writer),
                "Channel or stereo partner is locked"
            );
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn apply_input_processor(
        &self,
        request_id: String,
        epoch: u64,
        ch: u16,
        key: String,
        processor: String,
        field: Option<String>,
        value: Option<Value>,
        guard: Option<Value>,
        writer: &str,
        out: &mut Vec<Event>,
    ) -> Event {
        let result: anyhow::Result<Value> = async {
            anyhow::ensure!(
                !request_id.is_empty() && request_id.len() <= 128,
                "Invalid processor request ID"
            );
            let ch = self.resolve_strip(ch, Some(&key))?;
            self.input_processor_access(ch, epoch, writer)?;
            let context = self.input_processor_context(ch, &processor, epoch)?;
            let mut provider_guard = guard;
            if let Some(g) = provider_guard.as_mut() {
                anyhow::ensure!(
                    g["engine_context"] == context && g.to_string().len() <= 65536,
                    "Input descriptor, provider or session changed"
                );
                g.as_object_mut()
                    .ok_or_else(|| anyhow::anyhow!("Invalid processor guard"))?
                    .remove("engine_context");
            }
            let write = match (&field, &value) {
                (Some(f), Some(v)) => {
                    let pv = json_to_param_value(v)?;
                    self.registry(&self.active_driver()).validate(f, &pv)?;
                    Some((f.as_str(), pv))
                }
                (None, None) => None,
                _ => anyhow::bail!("Incomplete processor write"),
            };
            anyhow::ensure!(
                write.is_none() || provider_guard.is_some(),
                "Processor write requires complete captured block"
            );
            self.input_processor_access(ch, epoch, writer)?;
            let preflight = || {
                self.input_processor_access(ch, epoch, writer)?;
                anyhow::ensure!(
                    self.input_processor_context(ch, &processor, epoch)? == context,
                    "Input descriptor changed before dispatch or during readback; result uncertain"
                );
                Ok(())
            };
            let mut snapshot = self
                .active_drv()
                .input_processor(
                    StripId(ch),
                    &processor,
                    write,
                    provider_guard.as_ref(),
                    &preflight,
                )
                .await?;
            self.input_processor_access(ch, epoch, writer)?;
            anyhow::ensure!(
                self.input_processor_context(ch, &processor, epoch)? == context,
                "Input descriptor changed during readback; result uncertain"
            );
            snapshot
                .as_object_mut()
                .ok_or_else(|| anyhow::anyhow!("Invalid provider snapshot"))?
                .insert("engine_context".into(), context);
            if let (Some(field), Some(value)) = (field, value) {
                // Provider success requires fresh readback. Publish that state
                // through the existing parameter path for ordinary screens.
                let verified = snapshot["rows"]
                    .as_array()
                    .and_then(|rows| rows.iter().find(|r| r["id"] == field))
                    .map(|r| r["value"].clone());
                anyhow::ensure!(
                    verified.as_ref().is_some_and(|v| refresh_agrees(v, &value)),
                    "Input command accepted without matching readback; result uncertain"
                );
                let read = verified.expect("checked readback");
                self.apply_inbound(ch, &field, &json_to_param_value(&read)?);
                out.push(Event::Param {
                    ch,
                    field,
                    value: read,
                    by: Some(writer.into()),
                    key: self.active_strip_key(ch),
                    authority: Some("verified".into()),
                });
                self.note_write(writer);
                self.touch_holder_locks(writer);
            }
            Ok(snapshot)
        }
        .await;
        let (snapshot, message) = match result {
            Ok(s) => (Some(s), None),
            Err(e) => (None, Some(e.to_string())),
        };
        Event::InputProcessorResult {
            request_id,
            epoch,
            snapshot,
            message,
        }
    }
}
