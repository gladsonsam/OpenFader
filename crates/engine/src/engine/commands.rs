//! Command entry, connection serialization, write guard, and publication barrier.

use super::*;

mod client;
mod connection;
mod destination;
mod input;
mod library;
mod offline;
mod processing;

impl Engine {
    /// Adapter entry: apply one raw JSON command line from `writer` without
    /// a [`ClientHandle`] (the serve inbound task owns none, the outbound
    /// task owns the handle for `events`). Same semantics as
    /// [`ClientHandle::send_json`]: parse failures and domain errors arrive
    /// as unicast `error` events, never closing the connection.
    pub async fn handle_json(&self, writer: &str, text: &str) {
        match crate::protocol::parse_cmd(text) {
            Ok(cmd) => self.apply_cmd(writer, cmd).await,
            Err(message) => {
                if !self.has_client(writer) {
                    return;
                }
                self.unicast(
                    writer,
                    Event::Error {
                        message,
                        locked_by: None,
                        ch: None,
                    },
                );
            }
        }
    }

    /// Shared command dispatch for [`ClientHandle::send`] and
    /// [`handle_json`](Engine::handle_json): run [`apply`](Engine::apply),
    /// unicast the direct reply (or error), then broadcast.
    pub(super) async fn apply_cmd(&self, writer: &str, cmd: Cmd) {
        let connecting = matches!(
            cmd,
            Cmd::Connect { .. }
                | Cmd::ConnectDevice { .. }
                | Cmd::Probe { .. }
                | Cmd::Disconnect
                | Cmd::Offline { .. }
        );
        if connecting {
            let suspension = AtomicU8::new(0);
            self.connection_lifecycle
                .run(
                    async {
                        // Capture only after acquiring the intent lane: a queued
                        // selection must not inherit the older attempt's suspension.
                        suspension.store(
                            1 + u8::from(self.reads_suspended.load(Ordering::Acquire)),
                            Ordering::Release,
                        );
                        self.dispatch_cmd(writer, cmd, true).await;
                    },
                    || {
                        let before = suspension.load(Ordering::Acquire);
                        if before == 0 {
                            return;
                        } // Cancelled before work was polled.
                          // Restore a cancelled pre-readiness attempt's retained read
                          // lane before a newer probe can fail without changing epoch.
                        self.hydrating.store(0, Ordering::Release);
                        if self.reads_suspended.load(Ordering::Acquire) {
                            self.reads_suspended.store(before == 2, Ordering::Release);
                        }
                        self.repair_dirty.store(true, Ordering::Release);
                    },
                )
                .await;
        } else {
            self.dispatch_cmd(writer, cmd, false).await;
        }
    }

    /// Connection work holds its intent lane through final publication, so
    /// an old handshake cannot publish frames after a newer command's epoch.
    pub(super) async fn dispatch_cmd(&self, writer: &str, cmd: Cmd, connecting: bool) {
        let started = Instant::now();
        let generation = self.connection_gen.load(Ordering::Acquire);
        let mut out = Vec::new();
        let result = self.apply(cmd, writer, &mut out).await;
        match result {
            Ok(Some(reply)) => self.unicast(writer, reply),
            Ok(None) => {}
            Err(e) => self.unicast(
                writer,
                Event::Error {
                    message: e.to_string(),
                    locked_by: None,
                    ch: None,
                },
            ),
        }
        let sync = self.sync_state.lock().unwrap();
        if !connecting && generation != self.connection_gen.load(Ordering::Acquire) {
            return;
        }
        for event in out {
            self.broadcast(self.reconcile_event(event, &sync, started));
        }
    }

    /// Apply one client command from `writer` (its client id).
    ///
    /// Returns a direct reply for `list_shows` (unicast `shows`), for
    /// `connect` (unicast `you`), for denials (unicast `error` with
    /// `locked_by`), and for `recall_scene` when it overrode foreign locks
    /// (unicast `recalled`); every other command broadcasts and returns
    /// `None`. Command-triggered broadcasts are collected in `out` (the
    /// caller flushes them after the reply); background tasks (poller,
    /// sweeper, meter ticker) broadcast directly. Mutation broadcasts
    /// carry `"by": writer`.
    pub(super) async fn apply(
        &self,
        cmd: Cmd,
        writer: &str,
        out: &mut Vec<Event>,
    ) -> anyhow::Result<Option<Event>> {
        // MR1 write lane: mark the command in flight so the param poller
        // yields its tick while any command (fader/mute/param/main or
        // otherwise) runs. RAII: every return path below drops the guard.
        let _write_guard = WriteGuard::new(&self.writes_in_flight);
        // Opportunistic expiry: release stale holds before any check so a
        // just-expired strip reads as unlocked, and broadcast when the
        // sweep actually removed something.
        if self.prune_expired_locks() {
            out.push(self.locks_message_inner());
        }
        let routing_context = match &cmd {
            Cmd::RoutePatch { driver, epoch, expected, .. } => {
                Some((driver.clone(), *epoch, expected.clone()))
            }
            _ => None,
        };
        let policy = self.policy_of(writer);
        let (cmd, expected) = match cmd {
            Cmd::ParamGuarded {
                ch,
                field,
                value,
                key,
                expect,
            } => {
                anyhow::ensure!(
                    !expect.is_empty() && expect.len() <= 8,
                    "parameter context needs 1..8 fields"
                );
                let expected = expect
                    .into_iter()
                    .map(|(k, v)| Ok((k, json_to_param_value(&v)?)))
                    .collect::<anyhow::Result<BTreeMap<_, _>>>()?;
                (
                    Cmd::Param {
                        ch,
                        field,
                        value,
                        key,
                    },
                    Some(expected),
                )
            }
            Cmd::RoutePatch { driver, epoch, destination, source, .. } =>
                (self.routing_command(&driver, epoch, &destination, source)?, None),
            other => (other, None),
        };
        match cmd {
            Cmd::RoutePatch { .. } => anyhow::bail!("routing command was not normalized"),
            cmd @ (Cmd::Connect { .. }
            | Cmd::RefreshDevices { .. }
            | Cmd::DiscoverDevices { .. }
            | Cmd::Scan
            | Cmd::ConnectDevice { .. }
            | Cmd::Probe { .. }
            | Cmd::ForgetSavedMixers { .. }
            | Cmd::Disconnect
            | Cmd::Info) => self.apply_connection(cmd, writer, out).await,
            cmd @ (Cmd::Fader { .. }
            | Cmd::Mute { .. }
            | Cmd::MainFader { .. }
            | Cmd::MainMute { .. }
            | Cmd::Solo { .. }
            | Cmd::Param { .. }
            | Cmd::ParamGuarded { .. }
            | Cmd::ClearSolo) => self.apply_input(cmd, writer, out, &policy, expected).await,
            cmd @ (Cmd::SpectrumTarget { .. }
            | Cmd::Focus { .. }
            | Cmd::Lock { .. }
            | Cmd::Unlock { .. }
            | Cmd::Hello { .. }) => self.apply_client(cmd, writer, out).await,
            cmd @ (Cmd::ListShows
            | Cmd::CaptureScene { .. }
            | Cmd::RecallScene { .. }
            | Cmd::DeleteScene { .. }
            | Cmd::SceneSave { .. }
            | Cmd::SceneRecall { .. }
            | Cmd::SceneRename { .. }
            | Cmd::SceneDelete { .. }
            | Cmd::SceneClear
            | Cmd::LocalLibraryStatus
            | Cmd::LocalLibraryMutate { .. }) => {
                self.apply_library(cmd, writer, out, &policy).await
            }
            cmd @ (Cmd::FxSetType { .. }
            | Cmd::FxParam { .. }
            | Cmd::FxBypass { .. }
            | Cmd::FxOn { .. }
            | Cmd::PatchSet { .. }
            | Cmd::PatchClear { .. }
            | Cmd::FileDownload { .. }
            | Cmd::FileUpload { .. }) => self.apply_processing(cmd, writer, out, &policy, routing_context).await,
            cmd @ (Cmd::Offline { .. } | Cmd::OfflinePush { .. } | Cmd::OfflineDiscard) => {
                self.apply_offline(cmd, writer, out, &policy).await
            }
            cmd @ (Cmd::DestinationParam { .. }
            | Cmd::BusFader { .. }
            | Cmd::BusMute { .. }
            | Cmd::SubgroupFader { .. }
            | Cmd::SubgroupMute { .. }
            | Cmd::SubgroupAssign { .. }
            | Cmd::MuteGroup { .. }
            | Cmd::MuteGroupAssign { .. }) => {
                self.apply_destination(cmd, writer, out, &policy).await
            }
        }
    }
}
