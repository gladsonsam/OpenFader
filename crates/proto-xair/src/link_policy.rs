use super::*;
use openfader_core::destinations::{DestinationChoice, DestinationControl, DestinationValue};
use std::sync::atomic::Ordering;

pub(super) const IDS: [&str; 4] = ["preamp", "eq", "dyn", "fdrmute"];
pub(super) type Cache =
    std::sync::Arc<std::sync::Mutex<[(u64, Option<(std::time::Instant, bool)>); 4]>>;

pub(super) fn parse(msg: &OscMessage) -> Option<Vec<(usize, bool)>> {
    if msg.addr == "/config/linkcfg" {
        match msg.args.as_slice() {
            [OscType::Int(a @ 0..=1), OscType::Int(b @ 0..=1), OscType::Int(c @ 0..=1), OscType::Int(d @ 0..=1)] => {
                Some(vec![(0, *a == 1), (1, *b == 1), (2, *c == 1), (3, *d == 1)])
            }
            _ => None,
        }
    } else {
        let id = msg.addr.strip_prefix("/config/linkcfg/")?;
        let index = IDS.iter().position(|candidate| *candidate == id)?;
        match msg.args.as_slice() {
            [OscType::Int(value @ 0..=1)] => Some(vec![(index, *value == 1)]),
            _ => None,
        }
    }
}

pub(super) fn record(
    cache: &Cache,
    msg: &OscMessage,
    at: std::time::Instant,
    generation: &std::sync::atomic::AtomicU64,
    expected: u64,
) {
    if let Some(values) = parse(msg) {
        let mut state = cache.lock().unwrap();
        if generation.load(Ordering::Acquire) != expected {
            return;
        }
        for (index, value) in values {
            if state[index].1.is_none_or(|(_, old)| old != value) {
                state[index].0 = state[index].0.wrapping_add(1);
            }
            state[index].1 = Some((at, value));
        }
    }
}

impl XAirDriver {
    pub(super) fn refresh_link_policies(
        &self,
        session: std::sync::Arc<OscSession>,
        generation: u64,
    ) {
        let cache = self.link_policy_cache.clone();
        let current = self.connection_generation.clone();
        let task = tokio::spawn(async move {
            for id in IDS {
                let index = IDS.iter().position(|candidate| *candidate == id).unwrap();
                let revision = cache.lock().unwrap()[index].0;
                if let Ok(msg) = session.query(&format!("/config/linkcfg/{id}")).await {
                    if let Some(values) = parse(&msg) {
                        let mut state = cache.lock().unwrap();
                        if current.load(Ordering::Acquire) == generation
                            && state[index].0 == revision
                            && values.len() == 1
                            && values[0].0 == index
                        {
                            state[index].0 = state[index].0.wrapping_add(1);
                            state[index].1 = Some((std::time::Instant::now(), values[0].1));
                        }
                    }
                }
            }
        });
        self.tasks.lock().unwrap().push(task);
    }

    pub(super) fn link_policy_controls(&self) -> Vec<DestinationControl> {
        if !self.connected.load(Ordering::Acquire) {
            return vec![];
        }
        let generation = self.connection_generation.load(Ordering::Acquire);
        let cache = self.link_policy_cache.lock().unwrap();
        let labels = ["Preamp", "EQ", "Dynamics", "Fader/Mute"];
        (0..4)
            .map(|i| DestinationControl {
                revision: Some(cache[i].0),
                generation: Some(generation),
                preset: false,
                members: vec![],
                target: "link_policies".into(),
                target_label: "Link policies".into(),
                target_name: None,
                send_slot: None,
                surface: "link_policy_state".into(),
                description: "Read-only global link preference.".into(),
                id: IDS[i].into(),
                label: labels[i].into(),
                tap_tempo: false,
                gesture: None,
                choices: vec![
                    DestinationChoice {
                        value: 0.0,
                        label: "Off".into(),
                    },
                    DestinationChoice {
                        value: 1.0,
                        label: "On".into(),
                    },
                ],
                range: None,
                current: cache[i]
                    .1
                    .filter(|(at, _)| at.elapsed() <= std::time::Duration::from_secs(5))
                    .map(|(_, value)| DestinationValue {
                        value: u8::from(value) as f32,
                        verified: true,
                    }),
            })
            .collect()
    }
}
