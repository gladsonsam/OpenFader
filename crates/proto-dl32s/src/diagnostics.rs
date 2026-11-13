use crate::{addrs, Dl32sDriver};
use openfader_core::MixerInfo;
use openfader_transport::mackie::{MackieSession, VALUE_TYPE_PARAM};
use serde_json::{json, Value};
use std::{sync::atomic::Ordering, time::Duration};

const INFO_TIMEOUT: Duration = Duration::from_millis(500);

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn value_label(kind: u8, address: u32) -> Option<String> {
    if kind != VALUE_TYPE_PARAM {
        return None;
    }
    let map = addrs::get_map("dl32s").ok()?;
    std::iter::once(&map.inputs)
        .chain(map.blocks.values())
        .find_map(|block| {
            let offset = address.checked_sub(block.base)?;
            if offset >= block.count * block.stride {
                return None;
            }
            let field = block
                .fields
                .iter()
                .find(|field| field.offset == offset % block.stride)?;
            Some(format!(
                "{} {} · {}",
                block.name,
                offset / block.stride + 1,
                field.label
            ))
        })
}

fn row(key: String, label: Option<String>, value: String, pending: bool) -> Value {
    json!({"key": key, "label": label, "value": value,
        "status": if pending { "received; local write pending" } else { "received" }})
}

impl Dl32sDriver {
    pub(super) async fn read_identity(&self) -> anyhow::Result<MixerInfo> {
        let addr = self.addr.lock().await.clone().unwrap_or_default();
        let name = if let Ok(guard) = self.session().await {
            let session = guard.as_ref().expect("checked above");
            if let Some(body) = session.received_info(3) {
                MackieSession::info_string(&body)
            } else {
                session
                    .get_device_name(INFO_TIMEOUT)
                    .await
                    .unwrap_or_default()
            }
        } else {
            String::new()
        };
        Ok(MixerInfo {
            name,
            model: "DL32S".into(),
            firmware: "unparsed".into(),
            addr,
        })
    }

    pub async fn read_diagnostics(&self, offset: usize, limit: usize) -> anyhow::Result<Value> {
        let generation = self.generation.load(Ordering::Acquire);
        let guard = self.session().await?;
        let session = guard.as_ref().expect("checked above").clone();
        drop(guard);
        if offset == 0 {
            let mut information = Vec::new();
            for (label, subtype) in [
                ("Device name", 3),
                ("Subdevice name", 7),
                ("IP address", 4),
                ("Lock status", 5),
                ("Static IP configuration", 6),
                ("Network configuration (raw words)", 18),
                ("Wireless survey (raw words)", 19),
                ("Firmware information (raw bytes)", 0),
            ] {
                let result = match subtype {
                    3 => session.get_device_name(INFO_TIMEOUT).await,
                    7 => session.get_subdevice_name(INFO_TIMEOUT).await,
                    4 => session.get_ip_address(INFO_TIMEOUT).await,
                    5 => session
                        .get_lock_status(INFO_TIMEOUT)
                        .await
                        .map(|raw| raw.to_string()),
                    6 => session.get_static_ip_config(INFO_TIMEOUT).await.map(
                        |(enabled, ip, mask, gateway)| {
                            format!("enabled={enabled}, IP={ip}, mask={mask}, gateway={gateway}")
                        },
                    ),
                    18 => session.get_network_config(INFO_TIMEOUT).await.map(|words| {
                        words
                            .iter()
                            .map(|word| format!("{word:08x}"))
                            .collect::<Vec<_>>()
                            .join(" ")
                    }),
                    19 => session.site_survey(INFO_TIMEOUT).await.map(|rows| {
                        rows.iter()
                            .map(|row| {
                                row.iter()
                                    .map(|word| format!("{word:08x}"))
                                    .collect::<Vec<_>>()
                                    .join(" ")
                            })
                            .collect::<Vec<_>>()
                            .join("; ")
                    }),
                    0 => session
                        .request_firmware_info(INFO_TIMEOUT)
                        .await
                        .map(|message| hex(&message.body)),
                    _ => unreachable!(),
                };
                let (value, status) = match result {
                    Ok(value) => (value, "received"),
                    Err(error) => (error.to_string(), "unavailable"),
                };
                information.push((label.to_string(), value, status.to_string()));
            }
            let mut native = self
                .native
                .lock()
                .map_err(|_| anyhow::anyhow!("diagnostics unavailable"))?;
            anyhow::ensure!(
                native.generation == generation,
                "Diagnostic connection changed"
            );
            native.device_information = information;
        }
        let snapshot = session.received_state();
        let native = self
            .native
            .lock()
            .map_err(|_| anyhow::anyhow!("diagnostics unavailable"))?;
        anyhow::ensure!(
            native.generation == generation && native.view.connected,
            "Diagnostic connection changed"
        );
        let mut rows: Vec<Value> = native
            .device_information
            .iter()
            .map(|(label, value, status)| json!({"key": label, "value": value, "status": status}))
            .collect();
        drop(native);
        rows.extend(
            snapshot
                .names
                .into_iter()
                .map(|(slot, name, pending)| row(format!("name slot {slot}"), None, name, pending)),
        );
        rows.extend(
            snapshot
                .values
                .into_iter()
                .map(|(kind, address, raw, pending)| {
                    row(
                        format!("type {kind} · PID {address}"),
                        value_label(kind, address),
                        format!("0x{raw:08x} · u32={raw} · f32={}", f32::from_bits(raw)),
                        pending,
                    )
                }),
        );
        for block in snapshot.meter_blocks {
            rows.extend(block.raws.iter().enumerate().map(|(index, raw)| {
                row(
                    format!("meter list {} · word {index}", block.start),
                    None,
                    format!("0x{raw:08x} · f32={}", f32::from_bits(*raw)),
                    false,
                )
            }));
        }
        rows.extend(snapshot.info_frames.into_iter().map(|message| {
            row(
                format!("{:?} · {:?}", message.command, message.msg_type),
                None,
                hex(&message.body),
                false,
            )
        }));
        let total = rows.len();
        let offset = offset.min(total.saturating_sub(1) / limit.max(1) * limit.max(1));
        let page: Vec<_> = rows.into_iter().skip(offset).take(limit).collect();
        Ok(
            json!({"supported": true, "offset": offset, "total": total, "rows": page,
            "description": "Latest received state. Raw identifiers and interpretations do not certify control mappings. Refresh to sample again."}),
        )
    }
}
