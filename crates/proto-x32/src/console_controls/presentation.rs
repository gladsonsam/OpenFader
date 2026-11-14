//! Generic console descriptors from X32-owned domains.
use super::codecs::json_value;
use super::specs::{ConsoleControl, ConsoleControlKind, NumberScale};
use openfader_core::ParamValue;

fn presentation_value(row: &ConsoleControl, value: &ParamValue) -> serde_json::Value {
    match (row.kind, value) {
        (ConsoleControlKind::Bitset(_), ParamValue::Float(v)) => serde_json::json!(*v as u32),
        (
            ConsoleControlKind::Choice(_)
            | ConsoleControlKind::ReadOnlyChoice(_)
            | ConsoleControlKind::CardChoice { .. }
            | ConsoleControlKind::IQEq(_)
            | ConsoleControlKind::IQSound { .. },
            ParamValue::Float(v),
        ) => serde_json::json!(*v as i32),
        _ => json_value(value),
    }
}
pub(super) fn descriptor(
    row: &ConsoleControl,
    generation: u64,
    browser_epoch: u64,
    value: Option<ParamValue>,
    error: Option<String>,
) -> openfader_core::console_controls::ConsoleControl {
    use openfader_core::console_controls::{
        ConsoleControl as Item, ConsoleControlChoice as Choice, ConsoleControlKind as Kind,
    };
    let (kind, unit, min, max, step, choices) = match row.kind {
        ConsoleControlKind::Toggle => (Kind::Boolean, "", None, None, None, vec![]),
        ConsoleControlKind::Choice(options)
        | ConsoleControlKind::IQEq(options)
        | ConsoleControlKind::CardChoice {
            choices: options, ..
        } => (
            Kind::Enumeration,
            "",
            None,
            None,
            None,
            options
                .iter()
                .enumerate()
                .map(|(i, label)| Choice {
                    label: (*label).into(),
                    value: serde_json::json!(i),
                })
                .collect(),
        ),
        ConsoleControlKind::Bitset(options) => (
            Kind::Bitset,
            "",
            None,
            None,
            None,
            options
                .iter()
                .map(|(label, value)| Choice {
                    label: (*label).into(),
                    value: serde_json::json!(value),
                })
                .collect(),
        ),
        ConsoleControlKind::ReadOnlyChoice(options) => (
            Kind::Enumeration,
            "",
            None,
            None,
            None,
            options
                .iter()
                .enumerate()
                .map(|(i, label)| Choice {
                    label: (*label).into(),
                    value: serde_json::json!(i),
                })
                .collect(),
        ),
        ConsoleControlKind::ReadOnlyText(_) => (Kind::Text, "", None, None, None, vec![]),
        ConsoleControlKind::UserAssignment(_) => (Kind::Text, "", None, None, None, vec![]),
        ConsoleControlKind::IQSound { .. } => (Kind::Enumeration, "", None, None, None, vec![]),
        ConsoleControlKind::Text(_) => (Kind::Text, "", None, None, None, vec![]),
        ConsoleControlKind::Number {
            min,
            max,
            unit,
            scale,
        } => (
            Kind::Number,
            unit,
            Some(min as f64),
            Some(max as f64),
            match scale {
                NumberScale::Linear { step } => Some(step as f64),
                _ => None,
            },
            vec![],
        ),
    };
    let revision = value
        .as_ref()
        .map(|v| serde_json::json!({"session": generation, "browser": browser_epoch, "id": row.id, "value": presentation_value(row, v)}))
        .unwrap_or(serde_json::Value::Null);
    Item {
        path: format!("/{}", row.id),
        label: row.label.into(),
        kind,
        value: value.as_ref().map(|v| presentation_value(row, v)),
        read_only: error.is_some()
            || matches!(
                row.kind,
                ConsoleControlKind::ReadOnlyChoice(_) | ConsoleControlKind::ReadOnlyText(_)
            ),
        unavailable_reason: error,
        unit: unit.into(),
        min,
        max,
        step,
        choices,
        revision,
        model: None,
    }
}

pub(super) fn receipt(
    status: openfader_core::console_controls::ConsoleControlStatus,
    control: Option<openfader_core::console_controls::ConsoleControl>,
    message: impl Into<Option<String>>,
) -> openfader_core::console_controls::ConsoleControlResult {
    openfader_core::console_controls::ConsoleControlResult {
        status,
        control,
        message: message.into(),
    }
}
