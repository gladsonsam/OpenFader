//! Expansion-card choices and installed-card domains, protocol 4.09-02 pp.55–56.
use super::specs::{ConsoleControl, ConsoleControlKind};
const CARD_TYPES: &[&str] = &[
    "None",
    "X-UF",
    "X-USB",
    "X-DANTE",
    "X-ADAT",
    "X-MADI",
    "DN32-USB",
    "DN32-DANTE",
    "DN32-ADAT",
    "DN32-MADI",
    "X-LIVE",
    "X-WSG",
];
const UF_MODES: &[&str] = &["32/32", "16/16", "32/8", "8/32"];
const USB_MODES: &[&str] = &["32/32", "16/16", "32/8", "8/32", "8/8", "2/2"];
const SD: &[&str] = &["SD1", "SD2"];
const TRACKS: &[&str] = &["32Ch", "16Ch", "8Ch"];
const PLAYBACK: &[&str] = &["SD", "USB"];
const ROUTING: &[&str] = &["REC", "PLAY", "AUTO"];
#[rustfmt::skip]
pub(super) const CONTROLS: &[ConsoleControl] = &[
    ConsoleControl { id: "card.type", category: "Expansion card", label: "Installed card", path: "/-stat/xcardtype", kind: ConsoleControlKind::ReadOnlyChoice(CARD_TYPES) },
    ConsoleControl { id: "card.uf_mode", category: "Expansion card", label: "X-UF channel mode", path: "/-prefs/card/UFmode", kind: ConsoleControlKind::CardChoice { choices: UF_MODES, card_types: &[1] } },
    ConsoleControl { id: "card.usb_mode", category: "Expansion card", label: "USB channel mode", path: "/-prefs/card/USBmode", kind: ConsoleControlKind::CardChoice { choices: USB_MODES, card_types: &[2, 6] } },
    ConsoleControl { id: "card.live_sd", category: "Expansion card", label: "X-LIVE active SD card", path: "/-prefs/card/URECsdsel", kind: ConsoleControlKind::CardChoice { choices: SD, card_types: &[10] } },
    ConsoleControl { id: "card.live_tracks", category: "Expansion card", label: "X-LIVE recorded tracks", path: "/-prefs/card/URECtracks", kind: ConsoleControlKind::CardChoice { choices: TRACKS, card_types: &[10] } },
    ConsoleControl { id: "card.live_playback", category: "Expansion card", label: "X-LIVE playback source", path: "/-prefs/card/URECplayb", kind: ConsoleControlKind::CardChoice { choices: PLAYBACK, card_types: &[10] } },
    ConsoleControl { id: "card.live_routing", category: "Expansion card", label: "X-LIVE routing", path: "/-prefs/card/URECrout", kind: ConsoleControlKind::CardChoice { choices: ROUTING, card_types: &[10] } },
];

pub(super) async fn observed_type(
    session: &std::sync::Arc<openfader_transport::OscSession>,
) -> anyhow::Result<i32> {
    let reply = session
        .query_timeout("/-stat/xcardtype", crate::QUERY_TIMEOUT)
        .await?;
    crate::mapping::strict_int(&reply)
}

pub(super) fn bind_model(
    mut item: openfader_core::console_controls::ConsoleControl,
    model: i32,
) -> openfader_core::console_controls::ConsoleControl {
    item.model = Some(serde_json::json!(model));
    if let Some(revision) = item.revision.as_object_mut() {
        revision.insert("model".into(), serde_json::json!(model));
    }
    item
}
