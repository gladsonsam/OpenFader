use super::*;

#[derive(Debug, Default)]
pub struct ReceivedState {
    pub values: Vec<(u8, u32, u32, bool)>,
    pub names: Vec<(u32, String, bool)>,
    pub meter_blocks: Vec<ValueBlock>,
    pub info_frames: Vec<Message>,
}

impl MackieSession {
    pub fn received_state(&self) -> ReceivedState {
        let mut snapshot = ReceivedState::default();
        for (kind, store) in [
            (VALUE_TYPE_PARAM, &self.caches.values),
            (VALUE_TYPE_MUTE_GROUP, &self.caches.type2_values),
        ] {
            let store = store.lock().expect("value cache poisoned");
            snapshot
                .values
                .extend(store.observed.iter().map(|(address, (raw, _, _))| {
                    (kind, *address, *raw, store.pending.contains(address))
                }));
        }
        snapshot.values.extend(
            self.caches
                .extra_values
                .lock()
                .expect("value cache poisoned")
                .iter()
                .map(|((kind, address), raw)| (*kind, *address, *raw, false)),
        );
        let names = self.caches.names.lock().expect("name cache poisoned");
        snapshot.names = names
            .received
            .iter()
            .map(|(slot, name)| (*slot, name.clone(), names.pending.contains(slot)))
            .collect();
        drop(names);
        snapshot.meter_blocks = self
            .caches
            .meter_blocks
            .lock()
            .expect("meter cache poisoned")
            .values()
            .cloned()
            .collect();
        snapshot.info_frames = self
            .caches
            .info_frames
            .lock()
            .expect("info cache poisoned")
            .values()
            .cloned()
            .collect();
        snapshot
            .values
            .sort_by_key(|(kind, address, _, _)| (*kind, *address));
        snapshot.names.sort_by_key(|(slot, _, _)| *slot);
        snapshot
    }

    pub fn received_info(&self, subtype: u32) -> Option<Vec<u8>> {
        self.caches
            .info_frames
            .lock()
            .ok()?
            .get(&(
                Command::GeneralInfo as u8,
                MessageType::Response as u8,
                subtype,
            ))
            .map(|message| message.body.clone())
    }
}

pub(super) fn record_values(caches: &Caches, blocks: &[ValueBlock]) {
    for block in blocks {
        match block.value_type {
            VALUE_TYPE_PARAM | VALUE_TYPE_MUTE_GROUP => {}
            VALUE_TYPE_METERS => {
                caches
                    .meter_blocks
                    .lock()
                    .expect("meter cache poisoned")
                    .insert(block.start, block.clone());
            }
            kind => {
                let mut values = caches.extra_values.lock().expect("value cache poisoned");
                values.extend(
                    block
                        .raws
                        .iter()
                        .enumerate()
                        .map(|(index, raw)| ((kind, block.start + index as u32), *raw)),
                );
            }
        }
    }
}

pub(super) fn record_info(caches: &Caches, message: &Message) {
    if message.command == Command::ShowStatus && parse_value_blocks(&message.body).is_some() {
        return;
    }
    if !matches!(
        message.command,
        Command::GeneralInfo
            | Command::FirmwareInfo
            | Command::ChannelInfoControl
            | Command::ChannelNames
            | Command::ShowStatus
    ) {
        return;
    }
    let subtype = if message.command == Command::GeneralInfo {
        message
            .body
            .get(..4)
            .map(|bytes| u32::from_be_bytes(bytes.try_into().unwrap()))
            .unwrap_or(0)
    } else {
        0
    };
    caches
        .info_frames
        .lock()
        .expect("info cache poisoned")
        .insert(
            (message.command as u8, message.msg_type as u8, subtype),
            message.clone(),
        );
}
