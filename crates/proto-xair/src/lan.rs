//! XR18 stored LAN preferences. Readback does not establish the active IP address.
use super::*;
use std::{sync::atomic::Ordering, time::Duration};

pub use openfader_core::lan::*;

impl XAirDriver {
    fn lan_valid(&self, generation: u64) -> bool {
        self.model == addrs::XAirModel::Xr18
            && self.connected.load(Ordering::Acquire)
            && self.connection_generation.load(Ordering::Acquire) == generation
    }
    async fn lan_read_pinned(
        &self,
        session: &OscSession,
        generation: u64,
    ) -> anyhow::Result<LanSnapshot> {
        let mut previous = None;
        for _ in 0..2 {
            let mut values = Vec::new();
            for addr in [
                "/-prefs/lan/mode",
                "/-prefs/lan/addr",
                "/-prefs/lan/mask",
                "/-prefs/lan/gateway",
                "/-stat/netmode",
            ] {
                anyhow::ensure!(
                    self.lan_valid(generation),
                    "LAN session changed or disconnected"
                );
                let msg = session
                    .query_timeout(addr, Duration::from_millis(350))
                    .await?;
                anyhow::ensure!(
                    self.lan_valid(generation),
                    "LAN session changed or disconnected"
                );
                values.push(msg.args);
            }
            let scalar = |i: usize| -> anyhow::Result<i32> {
                match values[i].as_slice() {
                    [OscType::Int(v)] => Ok(*v),
                    _ => anyhow::bail!("Invalid LAN scalar"),
                }
            };
            let octets = |i: usize| -> anyhow::Result<[u8; 4]> {
                match values[i].as_slice() {
                    [OscType::Int(a), OscType::Int(b), OscType::Int(c), OscType::Int(d)] => Ok([
                        u8::try_from(*a)?,
                        u8::try_from(*b)?,
                        u8::try_from(*c)?,
                        u8::try_from(*d)?,
                    ]),
                    _ => anyhow::bail!("Invalid LAN octets"),
                }
            };
            let mode = match scalar(0)? {
                0 => LanMode::Static,
                1 => LanMode::Dhcp,
                2 => LanMode::DhcpServer,
                _ => anyhow::bail!("Invalid LAN mode"),
            };
            let active_mode = match scalar(4)? {
                0 => ActiveNetworkMode::Ethernet,
                1 => ActiveNetworkMode::WirelessClient,
                2 => ActiveNetworkMode::AccessPoint,
                _ => anyhow::bail!("Invalid active network mode"),
            };
            let snapshot = LanSnapshot {
                generation,
                stored: LanConfiguration {
                    mode,
                    address: octets(1)?,
                    mask: octets(2)?,
                    gateway: octets(3)?,
                },
                active_mode,
            };
            if let Some(old) = previous {
                anyhow::ensure!(old == snapshot, "LAN block changed during read");
            }
            previous = Some(snapshot);
        }
        anyhow::ensure!(
            self.lan_valid(generation),
            "LAN session changed or disconnected"
        );
        Ok(previous.unwrap())
    }
    /// Two independent complete reads of stored preferences and active mode.
    pub async fn read_lan_snapshot(&self) -> anyhow::Result<LanSnapshot> {
        let _lock = self.lan_operations.lock().await;
        let generation = self.connection_generation.load(Ordering::Acquire);
        anyhow::ensure!(
            self.lan_valid(generation),
            "LAN unavailable for this model or connection"
        );
        let session = self.session().await?.as_ref().unwrap().clone();
        self.lan_read_pinned(&session, generation).await
    }
    /// Stage changed fields, verify stored preferences, then optionally dispatch Apply once.
    pub async fn write_lan_checked(
        &self,
        expected: LanSnapshot,
        desired: LanConfiguration,
        authorize: &(dyn Fn() -> bool + Send + Sync),
        apply_requested: bool,
    ) -> Result<LanWriteResult, LanWriteError> {
        let _lock = self.lan_operations.lock().await;
        let mut staged = false;
        let operation = async {
            let generation = expected.generation;
            anyhow::ensure!(
                self.lan_valid(generation) && authorize(),
                "LAN session or authorization changed"
            );
            let session = self.session().await?.as_ref().unwrap().clone();
            anyhow::ensure!(
                self.lan_read_pinned(&session, generation).await? == expected,
                "LAN snapshot is stale"
            );
            let mode = |m| match m {
                LanMode::Static => 0,
                LanMode::Dhcp => 1,
                LanMode::DhcpServer => 2,
            };
            let mut changes = Vec::new();
            if expected.stored.mode != desired.mode {
                changes.push(("/-prefs/lan/mode".to_owned(), mode(desired.mode)));
            }
            for (name, old, new) in [
                ("addr", expected.stored.address, desired.address),
                ("mask", expected.stored.mask, desired.mask),
                ("gateway", expected.stored.gateway, desired.gateway),
            ] {
                for i in 0..4 {
                    if old[i] != new[i] {
                        changes.push((format!("/-prefs/lan/{name}/{i}"), i32::from(new[i])));
                    }
                }
            }
            let check = || {
                anyhow::ensure!(
                    self.lan_valid(generation) && authorize(),
                    "LAN session or authorization changed"
                );
                Ok(())
            };
            for (address, value) in changes {
                staged = true;
                session
                    .send_checked(&address, vec![OscType::Int(value)], &check)
                    .await?;
            }
            let verified = self.lan_read_pinned(&session, generation).await?;
            anyhow::ensure!(
                verified.stored == desired && verified.active_mode == expected.active_mode,
                "LAN verification mismatch; preferences may be partially staged"
            );
            check()?;
            let apply = if apply_requested {
                match session
                    .send_checked("/-action/updnet", vec![OscType::Int(1)], &check)
                    .await
                {
                    Ok(()) => LanApplyStatus::DispatchedUncertain,
                    Err(e) => LanApplyStatus::DispatchUncertain(e.to_string()),
                }
            } else {
                LanApplyStatus::NotRequested
            };
            Ok(LanWriteResult {
                verified_stored: verified.stored,
                apply,
            })
        }
        .await;
        operation.map_err(|e: anyhow::Error| LanWriteError {
            message: format!(
                "{e:#}{}",
                if staged {
                    "; preferences may be partially staged; no rollback attempted"
                } else {
                    ""
                }
            ),
            possibly_staged: staged,
        })
    }
}
