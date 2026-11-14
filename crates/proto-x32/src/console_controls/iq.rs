//! X32 iQ speaker types and EQ sets, protocol 4.09-02 p.57.
use super::specs::{ConsoleControl, ConsoleControlKind};

const MODELS: &[&str] = &["none", "iQ8", "iQ10", "iQ12", "iQ15", "iQ15B", "iQ18B"];
const EQ: &[&str] = &["Linear", "Live", "Speech", "Playback", "User"];

#[rustfmt::skip]
pub(super) const CONTROLS: &[ConsoleControl] = &[
    ConsoleControl { id: "iq.model.01", category: "iQ speakers", label: "Speaker 01 Model", path: "/-prefs/iQ/01/iQmodel", kind: ConsoleControlKind::Choice(MODELS) },
    ConsoleControl { id: "iq.eq.01", category: "iQ speakers", label: "Speaker 01 EQ set", path: "/-prefs/iQ/01/iQeqset", kind: ConsoleControlKind::IQEq(EQ) },
    ConsoleControl { id: "iq.model.02", category: "iQ speakers", label: "Speaker 02 Model", path: "/-prefs/iQ/02/iQmodel", kind: ConsoleControlKind::Choice(MODELS) },
    ConsoleControl { id: "iq.eq.02", category: "iQ speakers", label: "Speaker 02 EQ set", path: "/-prefs/iQ/02/iQeqset", kind: ConsoleControlKind::IQEq(EQ) },
    ConsoleControl { id: "iq.model.03", category: "iQ speakers", label: "Speaker 03 Model", path: "/-prefs/iQ/03/iQmodel", kind: ConsoleControlKind::Choice(MODELS) },
    ConsoleControl { id: "iq.eq.03", category: "iQ speakers", label: "Speaker 03 EQ set", path: "/-prefs/iQ/03/iQeqset", kind: ConsoleControlKind::IQEq(EQ) },
    ConsoleControl { id: "iq.model.04", category: "iQ speakers", label: "Speaker 04 Model", path: "/-prefs/iQ/04/iQmodel", kind: ConsoleControlKind::Choice(MODELS) },
    ConsoleControl { id: "iq.eq.04", category: "iQ speakers", label: "Speaker 04 EQ set", path: "/-prefs/iQ/04/iQeqset", kind: ConsoleControlKind::IQEq(EQ) },
    ConsoleControl { id: "iq.model.05", category: "iQ speakers", label: "Speaker 05 Model", path: "/-prefs/iQ/05/iQmodel", kind: ConsoleControlKind::Choice(MODELS) },
    ConsoleControl { id: "iq.eq.05", category: "iQ speakers", label: "Speaker 05 EQ set", path: "/-prefs/iQ/05/iQeqset", kind: ConsoleControlKind::IQEq(EQ) },
    ConsoleControl { id: "iq.model.06", category: "iQ speakers", label: "Speaker 06 Model", path: "/-prefs/iQ/06/iQmodel", kind: ConsoleControlKind::Choice(MODELS) },
    ConsoleControl { id: "iq.eq.06", category: "iQ speakers", label: "Speaker 06 EQ set", path: "/-prefs/iQ/06/iQeqset", kind: ConsoleControlKind::IQEq(EQ) },
    ConsoleControl { id: "iq.model.07", category: "iQ speakers", label: "Speaker 07 Model", path: "/-prefs/iQ/07/iQmodel", kind: ConsoleControlKind::Choice(MODELS) },
    ConsoleControl { id: "iq.eq.07", category: "iQ speakers", label: "Speaker 07 EQ set", path: "/-prefs/iQ/07/iQeqset", kind: ConsoleControlKind::IQEq(EQ) },
    ConsoleControl { id: "iq.model.08", category: "iQ speakers", label: "Speaker 08 Model", path: "/-prefs/iQ/08/iQmodel", kind: ConsoleControlKind::Choice(MODELS) },
    ConsoleControl { id: "iq.eq.08", category: "iQ speakers", label: "Speaker 08 EQ set", path: "/-prefs/iQ/08/iQeqset", kind: ConsoleControlKind::IQEq(EQ) },
    ConsoleControl { id: "iq.model.09", category: "iQ speakers", label: "Speaker 09 Model", path: "/-prefs/iQ/09/iQmodel", kind: ConsoleControlKind::Choice(MODELS) },
    ConsoleControl { id: "iq.eq.09", category: "iQ speakers", label: "Speaker 09 EQ set", path: "/-prefs/iQ/09/iQeqset", kind: ConsoleControlKind::IQEq(EQ) },
    ConsoleControl { id: "iq.model.10", category: "iQ speakers", label: "Speaker 10 Model", path: "/-prefs/iQ/10/iQmodel", kind: ConsoleControlKind::Choice(MODELS) },
    ConsoleControl { id: "iq.eq.10", category: "iQ speakers", label: "Speaker 10 EQ set", path: "/-prefs/iQ/10/iQeqset", kind: ConsoleControlKind::IQEq(EQ) },
    ConsoleControl { id: "iq.model.11", category: "iQ speakers", label: "Speaker 11 Model", path: "/-prefs/iQ/11/iQmodel", kind: ConsoleControlKind::Choice(MODELS) },
    ConsoleControl { id: "iq.eq.11", category: "iQ speakers", label: "Speaker 11 EQ set", path: "/-prefs/iQ/11/iQeqset", kind: ConsoleControlKind::IQEq(EQ) },
    ConsoleControl { id: "iq.model.12", category: "iQ speakers", label: "Speaker 12 Model", path: "/-prefs/iQ/12/iQmodel", kind: ConsoleControlKind::Choice(MODELS) },
    ConsoleControl { id: "iq.eq.12", category: "iQ speakers", label: "Speaker 12 EQ set", path: "/-prefs/iQ/12/iQeqset", kind: ConsoleControlKind::IQEq(EQ) },
    ConsoleControl { id: "iq.model.13", category: "iQ speakers", label: "Speaker 13 Model", path: "/-prefs/iQ/13/iQmodel", kind: ConsoleControlKind::Choice(MODELS) },
    ConsoleControl { id: "iq.eq.13", category: "iQ speakers", label: "Speaker 13 EQ set", path: "/-prefs/iQ/13/iQeqset", kind: ConsoleControlKind::IQEq(EQ) },
    ConsoleControl { id: "iq.model.14", category: "iQ speakers", label: "Speaker 14 Model", path: "/-prefs/iQ/14/iQmodel", kind: ConsoleControlKind::Choice(MODELS) },
    ConsoleControl { id: "iq.eq.14", category: "iQ speakers", label: "Speaker 14 EQ set", path: "/-prefs/iQ/14/iQeqset", kind: ConsoleControlKind::IQEq(EQ) },
    ConsoleControl { id: "iq.model.15", category: "iQ speakers", label: "Speaker 15 Model", path: "/-prefs/iQ/15/iQmodel", kind: ConsoleControlKind::Choice(MODELS) },
    ConsoleControl { id: "iq.eq.15", category: "iQ speakers", label: "Speaker 15 EQ set", path: "/-prefs/iQ/15/iQeqset", kind: ConsoleControlKind::IQEq(EQ) },
    ConsoleControl { id: "iq.model.16", category: "iQ speakers", label: "Speaker 16 Model", path: "/-prefs/iQ/16/iQmodel", kind: ConsoleControlKind::Choice(MODELS) },
    ConsoleControl { id: "iq.eq.16", category: "iQ speakers", label: "Speaker 16 EQ set", path: "/-prefs/iQ/16/iQeqset", kind: ConsoleControlKind::IQEq(EQ) },
    ConsoleControl { id: "iq.sound.01", category: "iQ speakers", label: "Speaker 01 sound profile", path: "/-prefs/iQ/01/iQsound", kind: ConsoleControlKind::IQSound { model_path: "/-prefs/iQ/01/iQmodel" } },
    ConsoleControl { id: "iq.sound.02", category: "iQ speakers", label: "Speaker 02 sound profile", path: "/-prefs/iQ/02/iQsound", kind: ConsoleControlKind::IQSound { model_path: "/-prefs/iQ/02/iQmodel" } },
    ConsoleControl { id: "iq.sound.03", category: "iQ speakers", label: "Speaker 03 sound profile", path: "/-prefs/iQ/03/iQsound", kind: ConsoleControlKind::IQSound { model_path: "/-prefs/iQ/03/iQmodel" } },
    ConsoleControl { id: "iq.sound.04", category: "iQ speakers", label: "Speaker 04 sound profile", path: "/-prefs/iQ/04/iQsound", kind: ConsoleControlKind::IQSound { model_path: "/-prefs/iQ/04/iQmodel" } },
    ConsoleControl { id: "iq.sound.05", category: "iQ speakers", label: "Speaker 05 sound profile", path: "/-prefs/iQ/05/iQsound", kind: ConsoleControlKind::IQSound { model_path: "/-prefs/iQ/05/iQmodel" } },
    ConsoleControl { id: "iq.sound.06", category: "iQ speakers", label: "Speaker 06 sound profile", path: "/-prefs/iQ/06/iQsound", kind: ConsoleControlKind::IQSound { model_path: "/-prefs/iQ/06/iQmodel" } },
    ConsoleControl { id: "iq.sound.07", category: "iQ speakers", label: "Speaker 07 sound profile", path: "/-prefs/iQ/07/iQsound", kind: ConsoleControlKind::IQSound { model_path: "/-prefs/iQ/07/iQmodel" } },
    ConsoleControl { id: "iq.sound.08", category: "iQ speakers", label: "Speaker 08 sound profile", path: "/-prefs/iQ/08/iQsound", kind: ConsoleControlKind::IQSound { model_path: "/-prefs/iQ/08/iQmodel" } },
    ConsoleControl { id: "iq.sound.09", category: "iQ speakers", label: "Speaker 09 sound profile", path: "/-prefs/iQ/09/iQsound", kind: ConsoleControlKind::IQSound { model_path: "/-prefs/iQ/09/iQmodel" } },
    ConsoleControl { id: "iq.sound.10", category: "iQ speakers", label: "Speaker 10 sound profile", path: "/-prefs/iQ/10/iQsound", kind: ConsoleControlKind::IQSound { model_path: "/-prefs/iQ/10/iQmodel" } },
    ConsoleControl { id: "iq.sound.11", category: "iQ speakers", label: "Speaker 11 sound profile", path: "/-prefs/iQ/11/iQsound", kind: ConsoleControlKind::IQSound { model_path: "/-prefs/iQ/11/iQmodel" } },
    ConsoleControl { id: "iq.sound.12", category: "iQ speakers", label: "Speaker 12 sound profile", path: "/-prefs/iQ/12/iQsound", kind: ConsoleControlKind::IQSound { model_path: "/-prefs/iQ/12/iQmodel" } },
    ConsoleControl { id: "iq.sound.13", category: "iQ speakers", label: "Speaker 13 sound profile", path: "/-prefs/iQ/13/iQsound", kind: ConsoleControlKind::IQSound { model_path: "/-prefs/iQ/13/iQmodel" } },
    ConsoleControl { id: "iq.sound.14", category: "iQ speakers", label: "Speaker 14 sound profile", path: "/-prefs/iQ/14/iQsound", kind: ConsoleControlKind::IQSound { model_path: "/-prefs/iQ/14/iQmodel" } },
    ConsoleControl { id: "iq.sound.15", category: "iQ speakers", label: "Speaker 15 sound profile", path: "/-prefs/iQ/15/iQsound", kind: ConsoleControlKind::IQSound { model_path: "/-prefs/iQ/15/iQmodel" } },
    ConsoleControl { id: "iq.sound.16", category: "iQ speakers", label: "Speaker 16 sound profile", path: "/-prefs/iQ/16/iQsound", kind: ConsoleControlKind::IQSound { model_path: "/-prefs/iQ/16/iQmodel" } },
];

/// Profile choices are meaningful only for the independently observed model.
pub(super) fn profiles(model: i32) -> Option<&'static [&'static str]> {
    Some(match model {
        1 => &["iQ8", "E8", "F8+", "UPJunior", "PS8", "NuQ8-DP"],
        2 => &["iQ10", "F10+", "UPJ-1P", "PS10-R2", "NuQ10-DP"],
        3 => &[
            "iQ12", "E12", "JF29NT", "ELX112P", "PRX612M", "F12+", "UPA-1P", "NuQ12-DP",
        ],
        4 => &[
            "iQ15", "JF59NT", "ELX115P", "PRX615M", "F15+", "UPQ-1P", "PS15-R2", "NuQ15-DP",
        ],
        5 => &["iQ15B", "E15X", "S15+", "B-15DP"],
        6 => &["iQ18B", "ELX18P", "PRX618S", "S18+", "B-18DP"],
        _ => return None,
    })
}

pub(super) async fn observed_model(
    session: &std::sync::Arc<openfader_transport::OscSession>,
    path: &str,
) -> anyhow::Result<i32> {
    let reply = session.query_timeout(path, crate::QUERY_TIMEOUT).await?;
    crate::mapping::strict_int(&reply)
}

pub(super) fn bind_model(
    mut item: openfader_core::console_controls::ConsoleControl,
    model: i32,
) -> openfader_core::console_controls::ConsoleControl {
    use openfader_core::console_controls::ConsoleControlChoice;
    item.model = Some(serde_json::json!(model));
    if let Some(revision) = item.revision.as_object_mut() {
        revision.insert("model".into(), serde_json::json!(model));
    }
    item.choices = profiles(model)
        .unwrap_or(&[])
        .iter()
        .enumerate()
        .map(|(i, label)| ConsoleControlChoice {
            label: (*label).into(),
            value: serde_json::json!(i),
        })
        .collect();
    item
}

/// Resolve the documented capitalization conflict by observing the live
/// provider. A single answering alternative is selected; zero or two stay
/// unavailable. Queries are read-only and never guess a native path.
pub(super) async fn resolve_eq_path(
    session: &std::sync::Arc<openfader_transport::OscSession>,
    row: &ConsoleControl,
) -> anyhow::Result<(String, openfader_core::ParamValue)> {
    let upper = row.path.replace("/iQeqset", "/IQeqset");
    let mut found = Vec::new();
    for path in [row.path, upper.as_str()] {
        let mut value = None;
        for _ in 0..2 {
            let mapping = crate::mapping::Mapping {
                path: path.into(),
                kind: crate::mapping::WireKind::Enum(EQ),
            };
            if let Ok(result) = session
                .query_timeout(path, crate::QUERY_TIMEOUT)
                .await
                .and_then(|msg| mapping.decode(&msg))
            {
                value = Some(result);
                break;
            }
        }
        if let Some(value) = value {
            found.push((path.to_owned(), value));
        }
    }
    anyhow::ensure!(
        found.len() == 1,
        "iQ EQ address is unavailable or both capitalization variants answer"
    );
    Ok(found.remove(0))
}

pub(super) fn bind_eq(
    mut item: openfader_core::console_controls::ConsoleControl,
    native_path: &str,
) -> openfader_core::console_controls::ConsoleControl {
    if let Some(revision) = item.revision.as_object_mut() {
        revision.insert("native_path".into(), serde_json::json!(native_path));
    }
    item
}
