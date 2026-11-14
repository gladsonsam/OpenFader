use super::*;
use openfader_core::console_controls::*;

struct NativeAttempt<'a> {
    engine: &'a Engine,
    driver: &'a str,
    epoch: u64,
    completed: bool,
}

impl Drop for NativeAttempt<'_> {
    fn drop(&mut self) {
        if !self.completed {
            self.engine
                .invalidate_after_native_attempt(self.driver, self.epoch);
        }
        let _sync = self.engine.sync_state.lock().unwrap();
        self.engine
            .native_control_in_flight
            .store(false, Ordering::Release);
    }
}

#[derive(Default)]
pub(crate) struct ConsoleRequests {
    epoch: u64,
    seen: HashSet<(String, String)>,
}

impl Engine {
    fn console_control_access(&self, writer: &str, driver: &str, epoch: u64) -> bool {
        if !self.has_client(writer)
            || self.active_driver() != driver
            || self.connection_gen.load(Ordering::Acquire) != epoch
            || self.reads_suspended.load(Ordering::Acquire)
        {
            return false;
        }
        // Native controls can address global routing and show state. Until a
        // provider supplies narrower resource scopes, require full desk access.
        let policy = self.policy_of(writer);
        policy.allow_recall().is_ok()
            && policy.allow_main(1.).is_ok()
            && policy.allow_upload().is_ok()
            && (0..self.driver_strips(driver) as u16).all(|ch| {
                policy.allow_assign(StripId(ch)).is_ok()
                    && policy.allow_rename(StripId(ch)).is_ok()
                    && policy.allow_fader(StripId(ch), 1.).is_ok()
                    && self.lock_holder(ch).is_none_or(|owner| owner == writer)
            })
    }

    pub(super) async fn apply_console_controls(&self, cmd: Cmd, writer: &str) -> Event {
        let driver = self.active_driver();
        let observed_epoch = self.connection_gen.load(Ordering::Acquire);
        match cmd {
            Cmd::ConsoleControls { request_id, path } => {
                let result = async {
                    anyhow::ensure!(
                        !request_id.is_empty() && request_id.len() <= 128,
                        "Invalid native request ID"
                    );
                    anyhow::ensure!(
                        path.starts_with('/') && path.len() <= 512,
                        "Invalid native node path"
                    );
                    anyhow::ensure!(
                        !self.reads_suspended.load(Ordering::Acquire),
                        "Connection is changing"
                    );
                    let _lane = self.native_lane.lock().await;
                    let drv = self.drv(&driver);
                    let capability = drv.console_controls_capability();
                    if !capability.available {
                        return Ok(ConsoleControlNode {
                            path: path.clone(),
                            label: "Console controls".into(),
                            unavailable_reason: capability.unavailable_reason,
                            ..Default::default()
                        });
                    }
                    let node = drv.console_controls_node(&path).await?;
                    anyhow::ensure!(
                        observed_epoch == self.connection_gen.load(Ordering::Acquire)
                            && driver == self.active_driver(),
                        "Native control session changed"
                    );
                    Ok::<_, anyhow::Error>(node)
                }
                .await;
                let (node, error) = match result {
                    Ok(node) => (node, None),
                    Err(error) => {
                        let error = error.to_string();
                        (
                            ConsoleControlNode {
                                path,
                                label: "Console controls".into(),
                                unavailable_reason: Some(error.clone()),
                                ..Default::default()
                            },
                            Some(error),
                        )
                    }
                };
                Event::ConsoleControls {
                    request_id,
                    driver,
                    epoch: observed_epoch,
                    node,
                    error,
                }
            }
            Cmd::ConsoleControlSet {
                request_id,
                epoch,
                path,
                value,
                revision,
            } => {
                let mut entered = false;
                let result = async {
                    anyhow::ensure!(!request_id.is_empty() && request_id.len() <= 128, "Invalid native request ID");
                    anyhow::ensure!(path.starts_with('/') && path.len() <= 512, "Invalid native control path");
                    let _lane = self.native_lane.lock().await;
                    let check = || self.console_control_access(writer, &driver, epoch);
                    anyhow::ensure!(check(), "Native control session, access or locks changed");
                    {
                        let mut requests = self.console_requests.lock().unwrap();
                        if requests.epoch != epoch {
                            *requests = ConsoleRequests { epoch, ..Default::default() };
                        }
                        if requests.seen.contains(&(writer.into(), request_id.clone())) {
                            return Ok(ConsoleControlResult {
                                status: ConsoleControlStatus::Uncertain, control: None,
                                message: Some("Duplicate request was not replayed. Refresh to inspect current state.".into()),
                            });
                        }
                        anyhow::ensure!(requests.seen.len() < 16384, "Native control request budget exhausted; reconnect before further edits");
                        requests.seen.insert((writer.into(), request_id.clone()));
                    }
                    let drv = self.drv(&driver);
                    anyhow::ensure!(drv.console_controls_capability().available, "Native console controls unavailable");
                    {
                        let _sync = self.sync_state.lock().unwrap();
                        anyhow::ensure!(check(), "Native control session, access or locks changed");
                        self.native_control_generation.fetch_add(1, Ordering::AcqRel);
                        self.native_control_in_flight.store(true, Ordering::Release);
                    }
                    let mut attempt = NativeAttempt { engine: self, driver: &driver, epoch, completed: false };
                    entered = true;
                    let result = drv.console_control_write(&ConsoleControlWrite { path, value, revision }, &check).await;
                    if result.as_ref().is_ok_and(|receipt| matches!(receipt.status, ConsoleControlStatus::Verified | ConsoleControlStatus::Accepted | ConsoleControlStatus::Uncertain)) {
                        self.invalidate_after_native_attempt(&driver, epoch);
                    }
                    attempt.completed = true;
                    drop(attempt);
                    let receipt = match result {
                        Ok(receipt) => receipt,
                        Err(error) => return Ok(ConsoleControlResult {
                            status: ConsoleControlStatus::Rejected, control: None,
                            message: Some(error.to_string()),
                        }),
                    };
                    anyhow::ensure!(check(), "Native control context changed during write; outcome uncertain");
                    if matches!(receipt.status, ConsoleControlStatus::Verified | ConsoleControlStatus::Accepted) {
                        self.note_write(writer);
                        self.touch_holder_locks(writer);
                    }
                    Ok::<_, anyhow::Error>(receipt)
                }.await;
                let result = result.unwrap_or_else(|error| ConsoleControlResult {
                    status: if entered {
                        ConsoleControlStatus::Uncertain
                    } else {
                        ConsoleControlStatus::Rejected
                    },
                    control: None,
                    message: Some(error.to_string()),
                });
                Event::ConsoleControlResult {
                    request_id,
                    driver,
                    epoch,
                    result,
                }
            }
            _ => unreachable!(),
        }
    }
}
