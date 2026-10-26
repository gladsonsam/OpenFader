//! Output edits and guarded recalls need independent originating-session evidence.
use super::map::Spec;
use crate::Dl32sDriver;
use openfader_transport::mackie::REQUEST_TIMEOUT;
use std::sync::atomic::Ordering;

impl Dl32sDriver {
    pub(super) async fn confirm_output_write(
        &self,
        spec: &Spec,
        generation: u64,
        readback_revision: u64,
        value: f32,
    ) -> anyhow::Result<()> {
        // The caller requested the initialization value-space push after ACK
        // and released the session lock. Only native desk-push ingestion can
        // turn the accepted value into verified evidence.
        let deadline = tokio::time::Instant::now() + REQUEST_TIMEOUT;
        loop {
            anyhow::ensure!(
                self.generation.load(Ordering::Acquire) == generation,
                "Output connection changed after acceptance; result uncertain"
            );
            {
                let native = self
                    .native
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Output cache unavailable; result uncertain"))?;
                anyhow::ensure!(
                    native.generation == generation && native.view.connected,
                    "Output connection changed after acceptance; result uncertain"
                );
                let row = native
                    .output_processing
                    .controls(generation)
                    .into_iter()
                    .find(|row| row.target == spec.target && row.id == spec.id);
                if let Some(row) =
                    row.filter(|row| row.current.as_ref().is_some_and(|v| v.verified))
                {
                    // The caller captured the exact next verified revision
                    // after ACK (or the already verified revision if a push
                    // beat the ACK). Extra transitions include peer ABA edits.
                    anyhow::ensure!(
                        row.revision == Some(readback_revision),
                        "Output changed again during readback; result uncertain"
                    );
                    let current = row.current.expect("verified above");
                    anyhow::ensure!(
                        current.value == value,
                        "Output write accepted, but mixer readback differs; result uncertain"
                    );
                    return Ok(());
                }
            }
            anyhow::ensure!(
                tokio::time::Instant::now() < deadline,
                "Output write accepted without independent mixer readback; result uncertain"
            );
            // Provider-side bounded wait; the sole frontend poll loop remains
            // responsible for delivery to the UI. The native ingest task owns
            // cache updates, avoiding duplicate or reordered ingestion here.
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }
}
