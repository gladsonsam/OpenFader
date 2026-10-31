//! XR18 input selectors. These primitives remain unadvertised until the engine
//! has a guarded routing transaction and snapshot integration.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputRoute {
    pub analog: i32,
    pub usb: i32,
    pub bank: i32,
}

impl InputRoute {
    pub fn active_label(self) -> String {
        input::source_label(
            self.bank,
            if self.bank == 0 {
                self.analog
            } else {
                self.usb
            },
        )
    }
}

impl XAirDriver {
    /// Read all three selectors as a coherent sample. A second read rejects a
    /// route that changed during the sample; OSC itself has no atomic read.
    pub async fn input_route(&self, strip: StripId) -> anyhow::Result<InputRoute> {
        let ch = self.check_channel(strip.0)?;
        let generation = self
            .connection_generation
            .load(std::sync::atomic::Ordering::Acquire);
        let session = self.session().await?.as_ref().unwrap().clone();
        let analog = format!("/ch/{ch:02}/config/insrc");
        let usb = input::config_rtnsrc(ch);
        let bank = format!("/ch/{ch:02}/preamp/rtnsw");
        let read = || async {
            Ok::<_, anyhow::Error>(InputRoute {
                analog: input::source_selector(&session.query(&analog).await?, 18)?,
                usb: input::source_selector(&session.query(&usb).await?, 17)?,
                bank: input::source_selector(&session.query(&bank).await?, 1)?,
            })
        };
        let state = read().await?;
        anyhow::ensure!(
            read().await? == state
                && self
                    .connection_generation
                    .load(std::sync::atomic::Ordering::Acquire)
                    == generation,
            "xair: input route or connection changed while reading"
        );
        Ok(state)
    }
}

impl XAirDriver {
    /// Dispatch a single selector only after the entire prior route and the
    /// currently shared physical sources have been rechecked in the same lane
    /// as gain and phantom writes. The caller owns authorization/session policy.
    pub async fn set_input_route_guarded(
        &self,
        strip: StripId,
        expected: InputRoute,
        desired: InputRoute,
        check: &(dyn for<'a> Fn(&'a [StripId]) -> anyhow::Result<()> + Send + Sync),
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            (0..=18).contains(&desired.analog)
                && (0..=17).contains(&desired.usb)
                && (0..=1).contains(&desired.bank),
            "xair: invalid input route"
        );
        let changes = usize::from(expected.analog != desired.analog)
            + usize::from(expected.usb != desired.usb)
            + usize::from(expected.bank != desired.bank);
        anyhow::ensure!(
            changes <= 1,
            "xair: change only one input selector at a time"
        );
        let _lane = self.preamp_write.lock().await;
        let generation = self
            .connection_generation
            .load(std::sync::atomic::Ordering::Acquire);
        anyhow::ensure!(
            self.input_route(strip).await? == expected,
            "xair: input route changed; reopen the control"
        );
        let ch = self.check_channel(strip.0)?;
        let affected = [expected.analog, desired.analog];
        let mut members = self.stereo_control_members(strip, "gain")?;
        let mut sources = Vec::with_capacity(16);
        for index in 0..16_u16 {
            let source = self.active_source(StripId(index)).await?;
            if source.mode == 0
                && affected.contains(&source.index)
                && source.index < 18
                && !members.contains(&StripId(index))
            {
                members.push(StripId(index));
            }
            sources.push(source);
        }
        if affected.iter().any(|v| (16..=17).contains(v)) && !members.contains(&StripId(35)) {
            members.push(StripId(35));
        }
        check(&members)?;
        anyhow::ensure!(
            self.input_route(strip).await? == expected,
            "xair: input route changed; reopen the control"
        );
        for (index, source) in sources.iter().enumerate() {
            anyhow::ensure!(
                self.active_source(StripId(index as u16)).await? == *source,
                "xair: shared input routes changed; reopen the control"
            );
        }
        anyhow::ensure!(
            self.connection_generation
                .load(std::sync::atomic::Ordering::Acquire)
                == generation,
            "xair: connection changed; reopen the control"
        );
        check(&members)?;
        if changes == 0 {
            return Ok(());
        }
        // A linked channel can change its peer as a side effect. Until the
        // transaction journals and validates both members, refuse the write.
        // Query the desk here rather than relying on the expiring link cache.
        anyhow::ensure!(
            !self.read_stereo_link(strip).await?,
            "xair: linked input routing requires coupled transaction support"
        );
        check(&members)?;
        let (addr, value) = if expected.analog != desired.analog {
            (format!("/ch/{ch:02}/config/insrc"), desired.analog)
        } else if expected.usb != desired.usb {
            (input::config_rtnsrc(ch), desired.usb)
        } else {
            (format!("/ch/{ch:02}/preamp/rtnsw"), desired.bank)
        };
        let session = self.session().await?.as_ref().unwrap().clone();
        anyhow::ensure!(
            self.connection_generation
                .load(std::sync::atomic::Ordering::Acquire)
                == generation,
            "xair: connection changed; reopen the control"
        );
        session.send(&addr, vec![OscType::Int(value)]).await?;
        anyhow::ensure!(
            self.input_route(strip).await? == desired,
            "xair: input route sent but not confirmed; mixer may have changed"
        );
        Ok(())
    }
}

impl XAirDriver {
    /// Switch a linked pair's bank with a single physical write. Both complete
    /// routes and the live link state are checked in the preamp write lane.
    pub async fn set_linked_input_bank_guarded(
        &self,
        left: StripId,
        expected: [InputRoute; 2],
        bank: i32,
        check: &(dyn for<'a> Fn(&'a [StripId]) -> anyhow::Result<()> + Send + Sync),
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            left.0 < 16 && left.0.is_multiple_of(2),
            "xair: invalid input pair"
        );
        anyhow::ensure!((0..=1).contains(&bank), "xair: invalid bank");
        anyhow::ensure!(
            expected[0].bank == expected[1].bank,
            "xair: unequal pair banks"
        );
        let members = [left, StripId(left.0 + 1)];
        let _lane = self.preamp_write.lock().await;
        let generation = self
            .connection_generation
            .load(std::sync::atomic::Ordering::Acquire);
        check(&members)?;
        anyhow::ensure!(
            self.read_stereo_link(left).await?,
            "xair: input pair is not linked"
        );
        for (member, route) in members.iter().zip(expected) {
            anyhow::ensure!(
                self.input_route(*member).await? == route,
                "xair: input pair route changed"
            );
        }
        anyhow::ensure!(
            self.read_stereo_link(left).await?,
            "xair: input pair link changed"
        );
        anyhow::ensure!(
            self.connection_generation
                .load(std::sync::atomic::Ordering::Acquire)
                == generation,
            "xair: connection changed"
        );
        check(&members)?;
        if expected[0].bank != bank {
            let ch = self.check_channel(left.0)?;
            let session = self.session().await?.as_ref().unwrap().clone();
            session
                .send(
                    &format!("/ch/{ch:02}/preamp/rtnsw"),
                    vec![OscType::Int(bank)],
                )
                .await?;
        }
        anyhow::ensure!(
            self.read_stereo_link(left).await?,
            "xair: input pair link changed"
        );
        for (member, mut route) in members.iter().zip(expected) {
            route.bank = bank;
            anyhow::ensure!(
                self.input_route(*member).await? == route,
                "xair: linked input bank not confirmed"
            );
        }
        check(&members)?;
        anyhow::ensure!(
            self.connection_generation
                .load(std::sync::atomic::Ordering::Acquire)
                == generation,
            "xair: connection changed"
        );
        Ok(())
    }
}
