use super::*;
use openfader_core::link_settings::StereoLinkSettings;
use std::sync::atomic::Ordering;
use std::time::Duration;

impl XAirDriver {
    pub(super) async fn read_link_settings_snapshot(&self) -> anyhow::Result<StereoLinkSettings> {
        const LEAVES: [&str; 7] = [
            "/config/buslink/1-2",
            "/config/buslink/3-4",
            "/config/buslink/5-6",
            "/config/linkcfg/preamp",
            "/config/linkcfg/eq",
            "/config/linkcfg/dyn",
            "/config/linkcfg/fdrmute",
        ];
        let _dispatch = self.bus_dispatch.read().await;
        let generation = self.connection_generation.load(Ordering::Acquire);
        anyhow::ensure!(
            self.connected.load(Ordering::Acquire),
            "Link settings: not connected"
        );
        // Query replies follow the normal observation path so actual coupling
        // changes can invalidate stale admission. This reader never seeds the
        // separate explicit-OFF cache used to grant independent bus writes.
        let session = self.session().await?.as_ref().unwrap().clone();
        let mut values = [false; 7];
        for pass in 0..2 {
            for (index, address) in LEAVES.iter().enumerate() {
                anyhow::ensure!(
                    self.connected.load(Ordering::Acquire)
                        && self.connection_generation.load(Ordering::Acquire) == generation,
                    "Link settings session changed"
                );
                let reply = session
                    .query_timeout(address, Duration::from_millis(350))
                    .await?;
                let value = match reply.args.as_slice() {
                    [OscType::Int(value @ 0..=1)] => *value == 1,
                    _ => anyhow::bail!("Invalid link settings reply at {address}"),
                };
                if pass == 0 {
                    values[index] = value;
                } else {
                    anyhow::ensure!(values[index] == value, "Link settings changed at {address}");
                }
            }
        }
        anyhow::ensure!(
            self.connected.load(Ordering::Acquire)
                && self.connection_generation.load(Ordering::Acquire) == generation,
            "Link settings session changed"
        );
        Ok(StereoLinkSettings {
            bus_links: [values[0], values[1], values[2]],
            policies: [values[3], values[4], values[5], values[6]],
        })
    }
}
