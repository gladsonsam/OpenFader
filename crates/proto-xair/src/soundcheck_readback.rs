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
            soundcheck_profile::assess(&info[2], &info[3], &sends, &inputs, aux_usb_pair, aux_bank)
                .map_err(|e| anyhow::anyhow!(e))?;
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
