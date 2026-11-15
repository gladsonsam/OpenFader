//! Explicit XR18/MR18 multichannel USB routing assessment.
//!
//! This validates a supplied readback only; callers must acquire fresh values
//! under a pinned engine session before using it to plan any routing changes.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsbSend {
    pub source: i32,
    pub tap: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MonoInput {
    pub analog: i32,
    pub usb: i32,
    pub bank: i32,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Coverage {
    /// For each mono strip, the zero-based physical capture track, or None
    /// when its live source is USB and therefore absent from analog capture.
    pub mono_tracks: [Option<u8>; 16],
    /// An aux strip already using USB is excluded from physical capture replay.
    pub aux_live_usb_excluded: bool,
}

/// Assess explicitly supported multichannel models and complete native routing.
/// This does not establish freshness, device identity, or microphone identity.
pub fn assess(
    model: &str,
    firmware: &str,
    sends: &[UsbSend; 18],
    inputs: &[MonoInput; 16],
    aux_usb_pair: i32,
    aux_bank: i32,
) -> Result<Coverage, &'static str> {
    if !matches!(model, "XR18" | "MR18") || firmware.trim().is_empty() {
        return Err("model has no configured multichannel USB profile or missing firmware");
    }
    if sends
        .iter()
        .any(|s| !(0..=17).contains(&s.source) || s.tap != 0)
    {
        return Err("unverified USB capture source or tap");
    }
    if !(0..=8).contains(&aux_usb_pair) || !(0..=1).contains(&aux_bank) {
        return Err("unsupported aux return");
    }
    let mut tracks = [None; 16];
    for (track, input) in tracks.iter_mut().zip(inputs) {
        if !(0..=18).contains(&input.analog)
            || !(0..=17).contains(&input.usb)
            || !(0..=1).contains(&input.bank)
        {
            return Err("unsupported input selector");
        }
        if input.bank == 0 && input.analog != 18 {
            *track = Some(
                sends
                    .iter()
                    .position(|send| send.source == input.analog)
                    .ok_or("live input is absent from USB capture")? as u8,
            );
        }
    }
    Ok(Coverage {
        mono_tracks: tracks,
        aux_live_usb_excluded: aux_bank == 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn evidence_scope_and_repatching() {
        let sends = std::array::from_fn(|i| UsbSend {
            source: i as i32,
            tap: 0,
        });
        let mut inputs = std::array::from_fn(|i| MonoInput {
            analog: i as i32,
            usb: i as i32,
            bank: 0,
        });
        inputs[0].analog = 4;
        inputs[14].bank = 1;
        let coverage = assess("XR18", "1.20", &sends, &inputs, 8, 1).unwrap();
        assert_eq!(coverage.mono_tracks[0], Some(4));
        assert_eq!(coverage.mono_tracks[14], None);
        assert!(coverage.aux_live_usb_excluded);
        assert!(assess("XR18", "1.19", &sends, &inputs, 8, 1).is_ok());
        let mut wrong = sends;
        wrong[4].tap = 1;
        assert!(assess("XR18", "1.20", &wrong, &inputs, 8, 1).is_err());
        wrong = sends;
        wrong[4].source = 3;
        assert!(assess("XR18", "1.20", &wrong, &inputs, 8, 1).is_err());
    }
}

/// Selector values required for a guarded route change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputRouteSelectors {
    pub analog: i32,
    pub usb: i32,
    pub bank: i32,
}

use serde_json::Value;

pub struct Preflight {
    pub plan: Vec<(String, i32)>,
    pub output_mask: [bool; 18],
    pub output_mapping: Vec<Option<usize>>,
    pub links: Vec<bool>,
    pub keys: Vec<String>,
    pub expected: std::collections::HashMap<u16, InputRouteSelectors>,
}

// The provider has already assessed two matching, generation-guarded readbacks.
pub fn preflight(readback: &Value) -> anyhow::Result<Preflight> {
    anyhow::ensure!(
        matches!(readback["model"].as_str(), Some("XR18" | "MR18"))
            && readback["firmware"]
                .as_str()
                .is_some_and(|f| !f.trim().is_empty()),
        "model has no configured multichannel USB profile or missing firmware"
    );
    let sends = readback["sends"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("missing sends"))?;
    anyhow::ensure!(
        sends.len() == 18
            && sends.iter().all(|s| s["source"]
                .as_i64()
                .is_some_and(|source| (0..=17).contains(&source))
                && s["tap"].as_i64() == Some(0)),
        "unverified capture routing"
    );
    anyhow::ensure!(
        readback["aux_usb_pair"]
            .as_i64()
            .is_some_and(|v| (0..=8).contains(&v))
            && readback["aux_bank"]
                .as_i64()
                .is_some_and(|v| (0..=1).contains(&v)),
        "unsupported aux return"
    );
    let inputs = readback["inputs"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("missing inputs"))?;
    anyhow::ensure!(inputs.len() == 16, "incomplete inputs");
    // input_links contains eight booleans indexed by the zero-based even pair / 2.
    let links = readback["input_links"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("missing input links"))?;
    anyhow::ensure!(
        links.len() == 8 && links.iter().all(Value::is_boolean),
        "invalid input links"
    );
    for pair in 0..8 {
        if links[pair] == true {
            let left = &inputs[pair * 2];
            let right = &inputs[pair * 2 + 1];
            anyhow::ensure!(
                left["bank"].as_i64().is_some() && left["bank"] == right["bank"],
                "unequal linked banks"
            );
        }
    }
    let mut plan = Vec::new();
    let mut keys: Vec<String> = (0..8)
        .map(|pair| format!("input_pair/{}/link", pair * 2))
        .collect();
    let mut expected = std::collections::HashMap::new();
    let mut output_mask = [false; 18];
    let mut reserved = [false; 18];
    keys.extend(["aux/usb_pair".to_owned(), "aux/bank".to_owned()]);
    let mut source_outputs = std::collections::BTreeMap::<usize, usize>::new();
    // Linked routing can share bank selection. Preserve its existing USB
    // selectors and map host output to them instead of repatching either peer.
    for (channel, input) in inputs.iter().enumerate() {
        let bank = input["bank"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Missing input bank"))?;
        let analog = input["analog"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Missing analog source"))? as usize;
        let usb = input["usb"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Missing USB source"))? as usize;
        anyhow::ensure!(
            bank <= 1 && analog <= 18 && usb < 18,
            "Unsupported input selector"
        );
        if bank == 1 {
            reserved[usb] = true;
        } else if links[channel / 2] == true {
            anyhow::ensure!(analog < 18, "Linked pair has an unavailable physical input");
            if let Some(previous) = source_outputs.insert(analog, usb) {
                anyhow::ensure!(
                    previous == usb,
                    "Linked inputs require multiple returns for one capture lane"
                );
            }
        }
    }
    if readback["aux_bank"] == 1 {
        let pair = readback["aux_usb_pair"].as_u64().unwrap() as usize * 2;
        reserved[pair] = true;
        reserved[pair + 1] = true;
    } else {
        // Select a free stereo pair, preferring the desk's current assignment.
        let preferred = readback["aux_usb_pair"].as_u64().unwrap() as usize;
        let pair = std::iter::once(preferred)
            .chain(0..9)
            .find(|pair| {
                let left = pair * 2;
                !reserved[left]
                    && !reserved[left + 1]
                    && source_outputs.iter().all(|(source, output)| {
                        (*output != left || *source == 16) && (*output != left + 1 || *source == 17)
                    })
                    && source_outputs.get(&16).is_none_or(|output| *output == left)
                    && source_outputs
                        .get(&17)
                        .is_none_or(|output| *output == left + 1)
            })
            .ok_or_else(|| anyhow::anyhow!("No free stereo USB return for aux inputs"))?;
        source_outputs.insert(16, pair * 2);
        source_outputs.insert(17, pair * 2 + 1);
        if preferred != pair {
            plan.push(("aux/usb_pair".into(), pair as i32));
        }
        plan.push(("aux/bank".into(), 1));
    }
    for (source, output) in &source_outputs {
        anyhow::ensure!(
            !reserved[*output]
                && source_outputs
                    .iter()
                    .all(|(other_source, other_output)| other_source == source
                        || other_output != output),
            "Linked USB return conflicts with another source"
        );
        output_mask[*output] = true;
    }
    for (channel, input) in inputs.iter().enumerate() {
        let bank = input["bank"]
            .as_i64()
            .ok_or_else(|| anyhow::anyhow!("missing bank"))?;
        let analog = input["analog"]
            .as_i64()
            .ok_or_else(|| anyhow::anyhow!("missing analog"))?;
        let usb = input["usb"]
            .as_i64()
            .ok_or_else(|| anyhow::anyhow!("missing usb"))?;
        anyhow::ensure!(
            (0..=1).contains(&bank) && (0..=18).contains(&analog) && (0..=17).contains(&usb),
            "unsupported route"
        );
        expected.insert(
            channel as u16,
            InputRouteSelectors {
                analog: analog as i32,
                usb: usb as i32,
                bank: bank as i32,
            },
        );
        if bank == 0 && analog == 18 {
            keys.extend(["analog", "usb", "bank"].map(|s| format!("input/{channel}/{s}")));
            continue;
        }
        if bank == 0 {
            let analog = analog as usize;
            let output = if let Some(output) = source_outputs.get(&analog) {
                *output
            } else {
                let output = std::iter::once(analog)
                    .chain(0..18)
                    .find(|output| !reserved[*output] && !output_mask[*output])
                    .ok_or_else(|| {
                        anyhow::anyhow!("No free USB return for physical input {}", analog + 1)
                    })?;
                source_outputs.insert(analog, output);
                output_mask[output] = true;
                output
            };
            if links[channel / 2] == true {
                keys.extend(["analog", "usb"].map(|s| format!("input/{channel}/{s}")));
                if channel.is_multiple_of(2) {
                    plan.push((format!("input_pair/{channel}/bank"), 1));
                }
            } else {
                keys.extend(["analog", "usb", "bank"].map(|s| format!("input/{channel}/{s}")));
                plan.push((format!("input/{channel}/usb"), output as i32));
                plan.push((format!("input/{channel}/bank"), 1));
            }
        } else {
            keys.extend(["analog", "usb", "bank"].map(|s| format!("input/{channel}/{s}")));
            reserved[usb as usize] = true;
        }
    }
    for slot in 0..18 {
        keys.extend(["source", "tap"].map(|s| format!("usb_send/{slot}/{s}")));
    }
    anyhow::ensure!(
        !output_mask
            .iter()
            .zip(reserved)
            .any(|(enabled, reserved)| *enabled && reserved),
        "covered output conflicts with reserved live USB return"
    );
    let mut output_mapping = vec![None; 18];
    for (source, output) in source_outputs {
        let track = sends
            .iter()
            .position(|s| s["source"].as_u64() == Some(source as u64))
            .ok_or_else(|| {
                anyhow::anyhow!("Physical input {} is absent from USB capture", source + 1)
            })?;
        output_mapping[track] = Some(output);
    }
    anyhow::ensure!(!plan.is_empty(), "no covered analog inputs");
    Ok(Preflight {
        plan,
        output_mask,
        output_mapping,
        links: links.iter().map(|v| v.as_bool().unwrap()).collect(),
        keys,
        expected,
    })
}

#[cfg(test)]
mod preflight_routing_tests {
    use super::preflight;
    use serde_json::Value;
    fn readback() -> Value {
        serde_json::json!({"model":"XR18", "firmware":"1.20", "aux_usb_pair":8, "aux_bank":1, "input_links":(vec![false; 8]),
            "sends": (0..18).map(|i| serde_json::json!({"source":i,"tap":0})).collect::<Vec<_>>(),
            "inputs": (0..16).map(|i| serde_json::json!({"analog":i,"usb":i,"bank":0})).collect::<Vec<_>>()})
    }
    #[test]
    fn linked_pair_is_one_guarded_change_and_existing_returns_are_mapped() {
        let mut data = readback();
        data["input_links"][3] = true.into();
        let plan = preflight(&data).unwrap();
        assert_eq!(
            plan.plan
                .iter()
                .filter(|(key, _)| key == "input_pair/6/bank")
                .count(),
            1
        );
        assert!(!plan
            .plan
            .iter()
            .any(|(key, _)| key == "input/6/bank" || key == "input/7/bank"));
        data["inputs"][7]["usb"] = 3.into();
        assert_eq!(preflight(&data).unwrap().output_mapping[7], Some(3));
        data["inputs"][7]["usb"] = 7.into();
        data["inputs"][7]["bank"] = 1.into();
        assert!(preflight(&data).is_err());
    }
    #[test]
    fn assessed_plan_excludes_live_usb_and_uses_analog_patch() {
        let mut data = readback();
        data["inputs"][0]["analog"] = 4.into();
        data["inputs"][1]["bank"] = 1.into();
        let p = preflight(&data).unwrap();
        assert!(p.plan.contains(&("input/0/usb".into(), 4)));
        assert!(p.keys.iter().any(|k| k.starts_with("input/1/")));
        assert_eq!(p.expected[&0].analog, 4);
        assert!(!p.output_mask[1]);
        assert!(p.output_mask[4]);
        data["inputs"][1]["usb"] = 4.into();
        let remapped = preflight(&data).unwrap();
        assert_ne!(remapped.output_mapping[4], Some(4));
        data["inputs"][1]["usb"] = 1.into();
        data["sends"][4]["tap"] = 1.into();
        assert!(preflight(&data).is_err());
        data = readback();
        data["firmware"] = "1.19".into();
        assert!(preflight(&data).is_ok());
    }
}
