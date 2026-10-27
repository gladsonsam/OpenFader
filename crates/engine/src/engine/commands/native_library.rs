//! Explicit correlated provider libraries; never enter local/offline replay lanes.
use super::*;

#[derive(Default)]
pub(crate) struct NativeRequests {
    epoch: u64,
    blocked: bool,
    seen: HashSet<(String, String)>,
    cancelled: HashSet<(String, String)>,
}

impl Engine {
    fn native_access(
        &self,
        writer: &str,
        driver: &str,
        epoch: u64,
        request: &str,
        write: bool,
    ) -> bool {
        let cancelled = {
            let requests = self.native_requests.lock().unwrap();
            requests.blocked
                || requests
                    .cancelled
                    .contains(&(writer.into(), request.into()))
        };
        if cancelled
            || !self.has_client(writer)
            || self.active_driver() != driver
            || self.connection_gen.load(Ordering::Acquire) != epoch
            || self.reads_suspended.load(Ordering::Acquire)
        {
            return false;
        }
        if !write {
            return true;
        }
        let policy = self.policy_of(writer);
        policy.allow_recall().is_ok()
            && policy.allow_main(1.).is_ok()
            && (0..self.driver_strips(driver) as u16).all(|ch| {
                policy.allow_strip(StripId(ch)).is_ok()
                    && self.lock_holder(ch).is_none_or(|owner| owner == writer)
            })
    }

    pub(super) async fn apply_native_library(&self, cmd: Cmd, writer: &str) -> Event {
        let Cmd::NativeLibrary {
            request_id,
            driver,
            epoch,
            action,
            expected,
        } = cmd
        else {
            unreachable!()
        };
        let operation = action["op"].as_str().unwrap_or("").to_string();
        let mut entered = false;
        let result: anyhow::Result<Value> = async {
            anyhow::ensure!(!request_id.is_empty() && request_id.len() <= 128, "Invalid native request ID");
            {
                let mut requests = self.native_requests.lock().unwrap();
                anyhow::ensure!(epoch == self.connection_gen.load(Ordering::Acquire), "Native session changed");
                if requests.epoch != epoch { *requests = NativeRequests {epoch, ..Default::default()}; }
                anyhow::ensure!(requests.seen.len() < 256, "Native request budget exhausted; reconnect before further requests");
                if !requests.seen.insert((writer.into(), request_id.clone())) {
                    return Ok(openfader_core::native_library::receipt("uncertain",
                        "Duplicate request was not replayed. The original may already have taken effect; inspect again.", Value::Null));
                }
            }
            let _lane = self.native_lane.lock().await;
            let write = !matches!(operation.as_str(), "inspect" | "refresh" | "read_show" | "export_file" | "inspect_file");
            let check = || self.native_access(writer, &driver, epoch, &request_id, write);
            anyhow::ensure!(check(), "Native session/access/locks changed");
            let drv = self.drv(&driver);
            let inventory = drv.native_library().ok_or_else(|| anyhow::anyhow!("Native library unsupported by this provider"))?;
            if operation == "inspect" {
                return Ok(openfader_core::native_library::receipt("confirmed", "Provider inventory inspected; completeness and authority are shown separately.", inventory));
            }
            anyhow::ensure!(inventory["connected"] == true, "Native provider disconnected");
            anyhow::ensure!(inventory["operations"].as_array().is_some_and(|ops| ops.iter().any(|v| v == &operation)), "Native operation unsupported");
            anyhow::ensure!(expected.as_ref() == Some(&inventory), "Native inventory changed; inspect again");
            entered = true;
            // Provider contract: Err is pre-mutation rejection. Once bytes
            // may have left, providers return an explicit uncertain receipt.
            let receipt = match drv.native_library_action(&action, &inventory, &check).await {
                Ok(receipt) => receipt,
                Err(error) => return Ok(openfader_core::native_library::receipt("rejected", error.to_string(), Value::Null)),
            };
            anyhow::ensure!(check(), "Native session/access/locks changed during operation; outcome uncertain");
            if write { self.note_write(writer); }
            Ok(receipt)
        }.await;
        let result = match result {
            Ok(receipt) => receipt,
            Err(e) => openfader_core::native_library::receipt(
                if entered { "uncertain" } else { "rejected" },
                e.to_string(),
                Value::Null,
            ),
        };
        Event::NativeLibraryResult {
            request_id,
            driver,
            epoch,
            operation,
            result,
        }
    }

    pub(super) fn cancel_native_library(&self, writer: &str, request_id: String) {
        let mut requests = self.native_requests.lock().unwrap();
        // Bounded tombstones also reject a cancellation racing command admission.
        let epoch = self.connection_gen.load(Ordering::Acquire);
        if requests.epoch != epoch {
            *requests = NativeRequests {
                epoch,
                ..Default::default()
            };
        }
        if requests.cancelled.len() >= 256 {
            requests.blocked = true;
        } else if request_id.len() <= 128 {
            requests.cancelled.insert((writer.into(), request_id));
        }
    }
}
