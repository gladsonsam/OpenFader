//! Evidence-scoped XR18 capture routing assessment.
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
    /// The aux strip already uses USB 17/18; its live USB signal is not
    /// represented by the physical aux capture tracks.
    pub aux_live_usb_excluded: bool,
}

/// Assess only the observed XR18 firmware and exact raw capture mapping.
/// This does not establish freshness, device identity, or microphone identity.
pub fn assess(
    model: &str,
    firmware: &str,
    sends: &[UsbSend; 18],
    inputs: &[MonoInput; 16],
    aux_usb_pair: i32,
    aux_bank: i32,
) -> Result<Coverage, &'static str> {
    if model != "XR18" || firmware != "1.20" {
        return Err("unverified model or firmware");
    }
    if sends
        .iter()
        .enumerate()
        .any(|(i, s)| s.source != i as i32 || s.tap != 0)
    {
        return Err("unverified USB capture source or tap");
    }
    if aux_usb_pair != 8 || aux_bank != 1 {
        return Err("unverified aux return");
    }
    let mut tracks = [None; 16];
    for (track, input) in tracks.iter_mut().zip(inputs) {
        if !(0..=15).contains(&input.analog)
            || !(0..=17).contains(&input.usb)
            || !(0..=1).contains(&input.bank)
        {
            return Err("unsupported input selector");
        }
        if input.bank == 0 {
            *track = Some(input.analog as u8);
        }
    }
    Ok(Coverage {
        mono_tracks: tracks,
        aux_live_usb_excluded: true,
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
        assert!(assess("XR18", "1.19", &sends, &inputs, 8, 1).is_err());
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
    pub links: Vec<bool>,
    pub keys: Vec<String>,
    pub expected: std::collections::HashMap<u16, InputRouteSelectors>,
}

// The provider has already assessed two matching, generation-guarded readbacks.
pub fn preflight(readback: &Value) -> anyhow::Result<Preflight> {
    anyhow::ensure!(
        readback["model"] == "XR18" && readback["firmware"] == "1.20",
        "unverified model or firmware"
    );
    let sends = readback["sends"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("missing sends"))?;
    anyhow::ensure!(
        sends.len() == 18
            && sends.iter().enumerate().all(
                |(i, s)| s["source"].as_i64() == Some(i as i64) && s["tap"].as_i64() == Some(0)
            ),
        "unverified capture routing"
    );
    anyhow::ensure!(
        readback["aux_usb_pair"].as_i64() == Some(8) && readback["aux_bank"].as_i64() == Some(1),
        "unverified aux return"
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
            if left["bank"] == 0 {
                anyhow::ensure!(
                    left["usb"] == left["analog"] && right["usb"] == right["analog"],
                    "linked return repatch unsupported"
                );
            }
        }
    }
    let mut plan = Vec::new();
    let mut keys: Vec<String> = (0..8)
        .map(|pair| format!("input_pair/{}/link", pair * 2))
        .collect();
    let mut expected = std::collections::HashMap::new();
    let mut output_mask = [false; 18];
    let mut reserved = [false; 18];
    reserved[16] = true;
    reserved[17] = true;
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
            (0..=1).contains(&bank) && (0..=15).contains(&analog) && (0..=17).contains(&usb),
            "unsupported route"
        );
        if bank == 0 {
            output_mask[analog as usize] = true;
            expected.insert(
                channel as u16,
                InputRouteSelectors {
                    analog: analog as i32,
                    usb: usb as i32,
                    bank: bank as i32,
                },
            );
            if links[channel / 2] == true {
                keys.extend(["analog", "usb"].map(|s| format!("input/{channel}/{s}")));
                if channel.is_multiple_of(2) {
                    plan.push((format!("input_pair/{channel}/bank"), 1));
                }
            } else {
                keys.extend(["analog", "usb", "bank"].map(|s| format!("input/{channel}/{s}")));
                plan.push((format!("input/{channel}/usb"), analog as i32));
                plan.push((format!("input/{channel}/bank"), 1));
            }
        } else {
            reserved[usb as usize] = true;
        }
    }
    anyhow::ensure!(
        !output_mask
            .iter()
            .zip(reserved)
            .any(|(enabled, reserved)| *enabled && reserved),
        "covered output conflicts with reserved live USB return"
    );
    anyhow::ensure!(!plan.is_empty(), "no covered analog inputs");
    Ok(Preflight {
        plan,
        output_mask,
        links: links.iter().map(|v| v.as_bool().unwrap()).collect(),
        keys,
        expected,
    })
}
