//! Fresh soundcheck preflight. `XAirDriver::soundcheck_readback()` returns JSON
//! with model, firmware, sends (source/tap), inputs (analog/usb/bank),
//! aux_usb_pair, aux_bank, and input_links (eight booleans for zero-based even pairs). It fails for incomplete, unstable or unverified
//! readback. OSC does not provide an atomic snapshot; two matching passes
//! bound the observation but cannot detect changes that revert between reads.
use super::*;
use std::sync::atomic::Ordering;

impl XAirDriver {
    pub async fn soundcheck_readback(&self) -> anyhow::Result<serde_json::Value> {
        let generation = self.connection_generation.load(Ordering::Acquire);
        let session = self.session().await?.as_ref().unwrap().clone();
        let read = || async {
            let info = osc::string_args(&session.query(addrs::INFO).await?);
            anyhow::ensure!(
                info.len() >= 4 && !info[2].is_empty() && !info[3].is_empty(),
                "incomplete mixer info"
            );
            anyhow::ensure!(
                matches!(info[2].as_str(), "XR18" | "MR18"),
                "This model has no configured multichannel USB audio interface"
            );
            let mut sends = Vec::with_capacity(18);
            for n in 1..=18 {
                let source = input::source_selector(
                    &session.query(&format!("/routing/usb/{n:02}/src")).await?,
                    255,
                )?;
                let tap = input::source_selector(
                    &session.query(&format!("/routing/usb/{n:02}/pos")).await?,
                    255,
                )?;
                sends.push(soundcheck_profile::UsbSend { source, tap });
            }
            let mut inputs = Vec::with_capacity(16);
            for n in 1..=16 {
                let analog = input::source_selector(
                    &session.query(&format!("/ch/{n:02}/config/insrc")).await?,
                    18,
                )?;
                let usb = input::source_selector(
                    &session.query(&format!("/ch/{n:02}/config/rtnsrc")).await?,
                    17,
                )?;
                let bank = input::source_selector(
                    &session.query(&format!("/ch/{n:02}/preamp/rtnsw")).await?,
                    1,
                )?;
                inputs.push(soundcheck_profile::MonoInput { analog, usb, bank });
            }
            let mut input_links = Vec::with_capacity(8);
            for left in (0..16).step_by(2) {
                input_links.push(self.read_stereo_link(StripId(left)).await?);
            }
            let aux_usb_pair =
                input::source_selector(&session.query("/rtn/aux/config/rtnsrc").await?, 8)?;
            let aux_bank =
                input::source_selector(&session.query("/rtn/aux/preamp/rtnsw").await?, 1)?;
            let sends: [soundcheck_profile::UsbSend; 18] = sends
                .try_into()
                .map_err(|_| anyhow::anyhow!("incomplete sends"))?;
            let inputs: [soundcheck_profile::MonoInput; 16] = inputs
                .try_into()
                .map_err(|_| anyhow::anyhow!("incomplete inputs"))?;
            Ok::<_, anyhow::Error>(
                serde_json::json!({"model":info[2],"firmware":info[3],"sends":sends.iter().map(|s| serde_json::json!({"source":s.source,"tap":s.tap})).collect::<Vec<_>>(),"inputs":inputs.iter().map(|i| serde_json::json!({"analog":i.analog,"usb":i.usb,"bank":i.bank})).collect::<Vec<_>>(),"aux_usb_pair":aux_usb_pair,"aux_bank":aux_bank,"input_links":input_links}),
            )
        };
        let first = read().await?;
        let second = read().await?;
        anyhow::ensure!(
            first == second && generation == self.connection_generation.load(Ordering::Acquire),
            "xair: soundcheck readback or connection changed"
        );
        Ok(first)
    }
}

impl XAirDriver {
    /// Aux input is one stereo selector pair; keep its physical input unchanged.
    pub async fn soundcheck_aux_route(&self) -> anyhow::Result<[i32; 2]> {
        let generation = self.connection_generation.load(Ordering::Acquire);
        let session = self.session().await?.as_ref().unwrap().clone();
        let read = || async {
            Ok::<_, anyhow::Error>([
                input::source_selector(&session.query("/rtn/aux/config/rtnsrc").await?, 8)?,
                input::source_selector(&session.query("/rtn/aux/preamp/rtnsw").await?, 1)?,
            ])
        };
        let route = read().await?;
        anyhow::ensure!(
            read().await? == route
                && self.connection_generation.load(Ordering::Acquire) == generation,
            "Aux route or connection changed"
        );
        Ok(route)
    }

    pub async fn soundcheck_aux_write(
        &self,
        expected: [i32; 2],
        index: usize,
        desired: i32,
        check: &(dyn Fn(&[StripId]) -> anyhow::Result<()> + Send + Sync),
    ) -> anyhow::Result<[i32; 2]> {
        anyhow::ensure!(
            index < 2 && (0..=if index == 0 { 8 } else { 1 }).contains(&desired),
            "Invalid aux routing selector"
        );
        let _lane = self.preamp_write.lock().await;
        let generation = self.connection_generation.load(Ordering::Acquire);
        let members = [StripId(35)];
        check(&members)?;
        anyhow::ensure!(
            self.soundcheck_aux_route().await? == expected,
            "Aux routing changed"
        );
        let session = self.session().await?.as_ref().unwrap().clone();
        check(&members)?;
        anyhow::ensure!(
            self.connection_generation.load(Ordering::Acquire) == generation,
            "Connection changed"
        );
        let address = if index == 0 {
            "/rtn/aux/config/rtnsrc"
        } else {
            "/rtn/aux/preamp/rtnsw"
        };
        if expected[index] != desired {
            session.send(address, vec![OscType::Int(desired)]).await?;
        }
        let mut next = expected;
        next[index] = desired;
        anyhow::ensure!(
            self.soundcheck_aux_route().await? == next,
            "Aux routing sent without confirmed readback; recovery required"
        );
        check(&members)?;
        anyhow::ensure!(
            self.connection_generation.load(Ordering::Acquire) == generation,
            "Connection changed; recovery required"
        );
        Ok(next)
    }
}

impl XAirDriver {
    pub async fn soundcheck_usb_selector(&self, slot: usize, field: &str) -> anyhow::Result<i32> {
        anyhow::ensure!(slot < 18, "Invalid USB capture slot");
        let suffix = match field {
            "source" => "src",
            "tap" => "pos",
            _ => anyhow::bail!("Invalid USB capture field"),
        };
        let generation = self.connection_generation.load(Ordering::Acquire);
        let session = self.session().await?.as_ref().unwrap().clone();
        let value = input::source_selector(
            &session
                .query(&format!("/routing/usb/{:02}/{suffix}", slot + 1))
                .await?,
            255,
        )?;
        anyhow::ensure!(
            generation == self.connection_generation.load(Ordering::Acquire),
            "Connection changed"
        );
        Ok(value)
    }

    /// Check every routing dependency before switching a bank. OSC is not an
    /// atomic transaction; matching complete fresh passes bound the observation.
    pub async fn guard_soundcheck_dependencies(
        &self,
        state: &serde_json::Value,
    ) -> anyhow::Result<()> {
        if state.is_null() {
            return Ok(());
        }
        let readback = self.soundcheck_readback().await?;
        anyhow::ensure!(
            readback["sends"] == state["sends"]
                && readback["input_links"] == state["links"]
                && serde_json::json!([readback["aux_usb_pair"], readback["aux_bank"]])
                    == state["aux"],
            "Soundcheck USB, aux or linked routing changed"
        );
        for (channel, actual) in readback["inputs"].as_array().unwrap().iter().enumerate() {
            anyhow::ensure!(
                state["expected"][channel.to_string()] == *actual,
                "Input {channel} routing changed"
            );
        }
        Ok(())
    }
}
