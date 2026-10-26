//! Session lifecycle, correlated reads, discovery identity validation and meters.
use crate::{
    capabilities,
    mapping::{mapping, root, Mapping, STRIPS, TOPOLOGY},
    WingDriver, QUERY_TIMEOUT,
};
use anyhow::{ensure, Context};
use openfader_core::{MixerInfo, ParamUpdate, ParamValue, StripId};
use openfader_transport::{discovery::parse_info_reply, OscSession};
use rosc::OscType;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{sync::broadcast, task::JoinHandle};
pub(crate) struct Link {
    pub(crate) session: Arc<OscSession>,
    pub(crate) info: MixerInfo,
    pub(crate) updates: JoinHandle<()>,
    pub(crate) meters: Option<openfader_transport::wing::WingMeterSession>,
}
impl Drop for Link {
    fn drop(&mut self) {
        self.updates.abort();
    }
}

impl WingDriver {
    pub fn meter_sample(&self) -> Option<openfader_transport::wing::WingMeterSample> {
        self.link
            .read()
            .unwrap()
            .as_ref()?
            .meters
            .as_ref()?
            .latest()
            .filter(|s| s.received.elapsed() < Duration::from_secs(1))
    }
    pub(crate) fn session(&self) -> anyhow::Result<Arc<OscSession>> {
        self.link
            .read()
            .unwrap()
            .as_ref()
            .map(|l| l.session.clone())
            .context("WING: not connected")
    }
    pub(crate) fn ensure_current(&self, session: &Arc<OscSession>) -> anyhow::Result<()> {
        ensure!(
            Arc::ptr_eq(session, &self.session()?),
            "WING: connection changed during operation"
        );
        Ok(())
    }
    pub(crate) async fn read(&self, m: &Mapping) -> anyhow::Result<ParamValue> {
        let session = self.session()?;
        let msg = session
            .query_with_args_timeout(&m.path, vec![], QUERY_TIMEOUT, |msg| {
                !matches!(msg.args.as_slice(), [OscType::Blob(_)])
            })
            .await?;
        self.ensure_current(&session)?;
        let value = m.decode(&msg)?;
        *self.last_control.lock().unwrap() = Some(Instant::now());
        Ok(value)
    }
    pub(crate) async fn connect_session(&self, addr: &str) -> anyhow::Result<()> {
        let _guard = self.lifecycle.lock().await;
        self.link.write().unwrap().take();
        *self.last_control.lock().unwrap() = None;
        let peer = openfader_transport::resolve_peer(addr, 2223)?;
        // WING allows ONE global OSC event subscriber, so use query polling
        // by default: connecting must not steal another client's subscription.
        let session = Arc::new(OscSession::connect(peer, None).await?);
        let response = session.query_timeout("/?", Duration::from_secs(2)).await?;
        let [OscType::String(raw)] = response.args.as_slice() else {
            anyhow::bail!("WING: invalid identity response")
        };
        let (model, _) =
            parse_info_reply(raw.as_bytes(), "WING").context("WING: invalid identity")?;
        let fields: Vec<_> = raw.split(',').collect();
        let info = MixerInfo {
            name: fields[2].into(),
            model,
            firmware: fields[5].into(),
            addr: peer.to_string(),
        };
        let mut received = session.subscribe_received_updates();
        let tx = self.updates.clone();
        let last_control = self.last_control.clone();
        let updates = tokio::spawn(async move {
            loop {
                let (at, msg) = match received.recv().await {
                    Ok(v) => v,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                };
                // The foreground query path performs model checks. Only
                // unambiguous, model-independent events enter the push plane.
                for strip in 0..STRIPS as u16 {
                    let Ok((r, _)) = root(StripId(strip)) else {
                        continue;
                    };
                    if !msg.addr.starts_with(&format!("{r}/")) {
                        continue;
                    }
                    for &id in capabilities() {
                        if let Ok(m) = mapping(StripId(strip), id) {
                            if m.model.is_none() && m.path == msg.addr {
                                if let Ok(value) = m.decode(&msg) {
                                    *last_control.lock().unwrap() = Some(at);
                                    let _ = tx.send(ParamUpdate {
                                        received_at: at,
                                        strip: StripId(strip),
                                        param: id.into(),
                                        value,
                                    });
                                }
                            }
                        }
                    }
                    break;
                }
            }
        });
        let specs = TOPOLOGY
            .iter()
            .enumerate()
            .flat_map(|(family, f)| {
                (0..f.count).map(move |index| openfader_transport::wing::WingMeterSpec {
                    token: 0xa0 + family as u8,
                    index: index as u8,
                })
            })
            .collect();
        let meters = match openfader_transport::wing::WingMeterSession::connect(
            std::net::SocketAddr::new(peer.ip(), 2222),
            specs,
        )
        .await
        {
            Ok(meters) => Some(meters),
            Err(error) => {
                eprintln!("WING: native meters unavailable: {error}");
                None
            }
        };
        *self.link.write().unwrap() = Some(Link {
            session,
            info,
            updates,
            meters,
        });
        *self.last_control.lock().unwrap() = Some(Instant::now());
        Ok(())
    }
    pub(crate) async fn refresh_meter_streams(&self) -> anyhow::Result<()> {
        let _guard = self.lifecycle.lock().await;
        let addr = {
            let mut link = self.link.write().unwrap();
            let link = link.as_mut().context("WING: not connected")?;
            link.meters.take();
            openfader_transport::resolve_peer(&link.info.addr, 2223)?
        };
        let specs = TOPOLOGY
            .iter()
            .enumerate()
            .flat_map(|(family, f)| {
                (0..f.count).map(move |index| openfader_transport::wing::WingMeterSpec {
                    token: 0xa0 + family as u8,
                    index: index as u8,
                })
            })
            .collect();
        let meters = openfader_transport::wing::WingMeterSession::connect(
            std::net::SocketAddr::new(addr.ip(), 2222),
            specs,
        )
        .await?;
        self.link
            .write()
            .unwrap()
            .as_mut()
            .context("WING: not connected")?
            .meters = Some(meters);
        Ok(())
    }
    pub(crate) async fn disconnect_session(&self) -> anyhow::Result<()> {
        let _guard = self.lifecycle.lock().await;
        self.link.write().unwrap().take();
        *self.last_control.lock().unwrap() = None;
        Ok(())
    }
}
