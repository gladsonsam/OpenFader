//! FX framework: generic numbered-param slots (X-Air-style), types + validation.
//!
//! Model: 4 FX slots ([`FX_SLOT_MIN`]..=[`FX_SLOT_MAX`]), each a flat array
//! of [`FX_PARAM_COUNT`] normalized params (0-based here; drivers add 1 on
//! 1-based wires, the same rule as [`StripId`](crate::StripId)).
//!
//! This crate only owns the shape and the validation. Per-mixer address
//! mapping (X-Air `/fx/N/par/MM`, DL32S equivalents) comes later in the
//! driver crates; [`MixerDriver`](crate::MixerDriver) already exposes
//! defaulted `set_fx_param`/`get_fx_param` hooks for that.

pub mod dl32s;

/// FX slot count (X-Air: 4 stereo slots).
pub const FX_SLOT_COUNT: usize = 4;
/// First slot number (wire convention is 1-based).
pub const FX_SLOT_MIN: u8 = 1;
pub const FX_SLOT_MAX: u8 = 4;
/// Params per slot, 0-based indices `0..FX_PARAM_COUNT` (wire: +1).
pub const FX_PARAM_COUNT: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FxSlot(u8);

impl FxSlot {
    pub fn new(n: u8) -> anyhow::Result<Self> {
        validate_slot(n)?;
        Ok(Self(n))
    }

    pub fn get(self) -> u8 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FxParamAddr {
    pub slot: FxSlot,
    pub index: u8,
}

impl FxParamAddr {
    pub fn new(slot: u8, index: u8) -> anyhow::Result<Self> {
        Ok(Self {
            slot: FxSlot::new(slot)?,
            index: validate_param_index(index)?,
        })
    }
}

pub fn validate_slot(slot: u8) -> anyhow::Result<u8> {
    if !(FX_SLOT_MIN..=FX_SLOT_MAX).contains(&slot) {
        anyhow::bail!("fx slot {slot} out of range {FX_SLOT_MIN}..={FX_SLOT_MAX}");
    }
    Ok(slot)
}

pub fn validate_param_index(index: u8) -> anyhow::Result<u8> {
    if index as usize >= FX_PARAM_COUNT {
        anyhow::bail!("fx param index {index} out of range 0..{FX_PARAM_COUNT}");
    }
    Ok(index)
}

pub fn validate_param_value(value: f32) -> anyhow::Result<f32> {
    if !value.is_finite() {
        anyhow::bail!("fx param must be finite, got {value}");
    }
    if !(0.0..=1.0).contains(&value) {
        anyhow::bail!("fx param {value} out of range 0.0..=1.0");
    }
    Ok(value)
}

#[derive(Clone, Debug)]
pub struct FxRack {
    values: [[f32; FX_PARAM_COUNT]; FX_SLOT_COUNT],
}
impl FxRack {
    pub fn new() -> Self {
        Self {
            values: [[0.0; FX_PARAM_COUNT]; FX_SLOT_COUNT],
        }
    }

    pub fn set(&mut self, slot: u8, index: u8, value: f32) -> anyhow::Result<()> {
        let slot = validate_slot(slot)?;
        let index = validate_param_index(index)?;
        let value = validate_param_value(value)?;
        self.values[(slot - FX_SLOT_MIN) as usize][index as usize] = value;
        Ok(())
    }

    pub fn get(&self, slot: u8, index: u8) -> anyhow::Result<f32> {
        let slot = validate_slot(slot)?;
        let index = validate_param_index(index)?;
        Ok(self.values[(slot - FX_SLOT_MIN) as usize][index as usize])
    }
}

impl Default for FxRack {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// R2 FX type metadata: the design's six FX types + their param tables.
//
// Source: design logic reference (`buildFx` defaults +
// `fxDefs` labels/formatters). These are UI-facing display tables only:
// values on the wire stay normalized 0.0..=1.0; `min`/`max`/`unit` describe
// the engineering range the UI formats into, `default` is the design's
// seeded normalized value, `scale` hints the formatter law (`lin`/`log`).
//
// Ranges here are display strings from the design, NOT desk documents
// never assert them against hardware. Desk-native algorithms (X-Air 61
// effect types) keep their own table in `crates/proto-xair/src/fx.rs`.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxParamMeta {
    pub id: &'static str,
    pub label: &'static str,
    pub unit: &'static str,
    /// Engineering range minimum (display only).
    pub min: f32,
    /// Engineering range maximum (display only).
    pub max: f32,
    pub default: f32,
    pub scale: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxTypeMeta {
    pub id: &'static str,
    pub label: &'static str,
    pub params: &'static [FxParamMeta],
}

const P_DECAY: FxParamMeta = FxParamMeta {
    id: "decay",
    label: "Decay",
    unit: "s",
    min: 0.2,
    max: 10.0,
    default: 0.42,
    scale: "lin",
};
const P_PREDELAY: FxParamMeta = FxParamMeta {
    id: "predelay",
    label: "Pre-delay",
    unit: "ms",
    min: 0.0,
    max: 200.0,
    default: 0.15,
    scale: "lin",
};
const P_SIZE: FxParamMeta = FxParamMeta {
    id: "size",
    label: "Size",
    unit: "%",
    min: 0.0,
    max: 100.0,
    default: 0.6,
    scale: "lin",
};
const P_HIDAMP: FxParamMeta = FxParamMeta {
    id: "hidamp",
    label: "Hi Damp",
    unit: "Hz",
    min: 1000.0,
    max: 20000.0,
    default: 0.55,
    scale: "log",
};
const P_MIX: FxParamMeta = FxParamMeta {
    id: "mix",
    label: "Mix",
    unit: "%",
    min: 0.0,
    max: 100.0,
    default: 1.0,
    scale: "lin",
};
const P_TIME: FxParamMeta = FxParamMeta {
    id: "time",
    label: "Time",
    unit: "ms",
    min: 20.0,
    max: 2000.0,
    default: 0.55,
    scale: "log",
};
const P_FEEDBACK_DLY: FxParamMeta = FxParamMeta {
    id: "feedback",
    label: "Feedback",
    unit: "%",
    min: 0.0,
    max: 95.0,
    default: 0.35,
    scale: "lin",
};
const P_FEEDBACK_FLG: FxParamMeta = FxParamMeta {
    id: "feedback",
    label: "Feedback",
    unit: "%",
    min: 0.0,
    max: 90.0,
    default: 0.4,
    scale: "lin",
};
const P_HICUT: FxParamMeta = FxParamMeta {
    id: "hicut",
    label: "Hi Cut",
    unit: "Hz",
    min: 1000.0,
    max: 20000.0,
    default: 0.55,
    scale: "log",
};
const P_SPREAD: FxParamMeta = FxParamMeta {
    id: "spread",
    label: "Spread",
    unit: "%",
    min: 0.0,
    max: 100.0,
    default: 0.4,
    scale: "lin",
};
const P_RATE: FxParamMeta = FxParamMeta {
    id: "rate",
    label: "Rate",
    unit: "Hz",
    min: 0.05,
    max: 5.0,
    default: 0.4,
    scale: "log",
};
const P_DEPTH: FxParamMeta = FxParamMeta {
    id: "depth",
    label: "Depth",
    unit: "%",
    min: 0.0,
    max: 100.0,
    default: 0.5,
    scale: "lin",
};

const HALL_PARAMS: &[FxParamMeta] = &[P_DECAY, P_PREDELAY, P_SIZE, P_HIDAMP, P_MIX];
const PLATE_PARAMS: &[FxParamMeta] = &[
    FxParamMeta {
        id: "decay",
        label: "Decay",
        unit: "s",
        min: 0.2,
        max: 10.0,
        default: 0.25,
        scale: "lin",
    },
    FxParamMeta {
        id: "predelay",
        label: "Pre-delay",
        unit: "ms",
        min: 0.0,
        max: 200.0,
        default: 0.08,
        scale: "lin",
    },
    FxParamMeta {
        id: "size",
        label: "Size",
        unit: "%",
        min: 0.0,
        max: 100.0,
        default: 0.5,
        scale: "lin",
    },
    FxParamMeta {
        id: "hidamp",
        label: "Hi Damp",
        unit: "Hz",
        min: 1000.0,
        max: 20000.0,
        default: 0.65,
        scale: "log",
    },
    P_MIX,
];
const ROOM_PARAMS: &[FxParamMeta] = &[
    FxParamMeta {
        id: "decay",
        label: "Decay",
        unit: "s",
        min: 0.2,
        max: 10.0,
        default: 0.1,
        scale: "lin",
    },
    FxParamMeta {
        id: "predelay",
        label: "Pre-delay",
        unit: "ms",
        min: 0.0,
        max: 200.0,
        default: 0.05,
        scale: "lin",
    },
    FxParamMeta {
        id: "size",
        label: "Size",
        unit: "%",
        min: 0.0,
        max: 100.0,
        default: 0.3,
        scale: "lin",
    },
    FxParamMeta {
        id: "hidamp",
        label: "Hi Damp",
        unit: "Hz",
        min: 1000.0,
        max: 20000.0,
        default: 0.6,
        scale: "log",
    },
    P_MIX,
];
const DELAY_PARAMS: &[FxParamMeta] = &[P_TIME, P_FEEDBACK_DLY, P_HICUT, P_SPREAD, P_MIX];
const CHORUS_PARAMS: &[FxParamMeta] = &[
    P_RATE,
    P_DEPTH,
    FxParamMeta {
        id: "spread",
        label: "Spread",
        unit: "%",
        min: 0.0,
        max: 100.0,
        default: 0.6,
        scale: "lin",
    },
    FxParamMeta {
        id: "mix",
        label: "Mix",
        unit: "%",
        min: 0.0,
        max: 100.0,
        default: 1.0,
        scale: "lin",
    },
];
const FLANGER_PARAMS: &[FxParamMeta] = &[
    FxParamMeta {
        id: "rate",
        label: "Rate",
        unit: "Hz",
        min: 0.05,
        max: 5.0,
        default: 0.3,
        scale: "log",
    },
    FxParamMeta {
        id: "depth",
        label: "Depth",
        unit: "%",
        min: 0.0,
        max: 100.0,
        default: 0.6,
        scale: "lin",
    },
    P_FEEDBACK_FLG,
    FxParamMeta {
        id: "mix",
        label: "Mix",
        unit: "%",
        min: 0.0,
        max: 100.0,
        default: 1.0,
        scale: "lin",
    },
];

/// The design's six FX types in UI order.
pub const FX_TYPES: &[FxTypeMeta] = &[
    FxTypeMeta {
        id: "hall",
        label: "Hall",
        params: HALL_PARAMS,
    },
    FxTypeMeta {
        id: "plate",
        label: "Plate",
        params: PLATE_PARAMS,
    },
    FxTypeMeta {
        id: "room",
        label: "Room",
        params: ROOM_PARAMS,
    },
    FxTypeMeta {
        id: "delay",
        label: "Delay",
        params: DELAY_PARAMS,
    },
    FxTypeMeta {
        id: "chorus",
        label: "Chorus",
        params: CHORUS_PARAMS,
    },
    FxTypeMeta {
        id: "flanger",
        label: "Flanger",
        params: FLANGER_PARAMS,
    },
];

/// Look up a design FX type by id or display label (case-insensitive).
pub fn fx_type_meta(id_or_label: &str) -> Option<&'static FxTypeMeta> {
    FX_TYPES.iter().find(|t| {
        t.id.eq_ignore_ascii_case(id_or_label) || t.label.eq_ignore_ascii_case(id_or_label)
    })
}

/// The design's default 8-slot rack types (`buildFx` order: Hall, Plate,
/// Delay, Chorus, Room, Delay, Flanger, Hall).
pub const DESIGN_FX_DEFAULT_TYPES: [&str; 8] = [
    "Hall", "Plate", "Delay", "Chorus", "Room", "Delay", "Flanger", "Hall",
];

/// Normalized default param vector for a design type id/label (falls back
/// to the type table's own defaults when the id is unknown).
pub fn fx_default_params(id_or_label: &str) -> Vec<f32> {
    match fx_type_meta(id_or_label) {
        Some(t) => t.params.iter().map(|p| p.default).collect(),
        None => vec![0.5; 5],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_roundtrip() {
        let mut rack = FxRack::new();
        rack.set(1, 0, 0.75).unwrap();
        rack.set(4, 63, 1.0).unwrap();
        assert_eq!(rack.get(1, 0).unwrap(), 0.75);
        assert_eq!(rack.get(4, 63).unwrap(), 1.0);
        assert_eq!(rack.get(2, 10).unwrap(), 0.0);
    }

    #[test]
    fn bad_slot_rejected() {
        let mut rack = FxRack::new();
        assert!(rack.set(0, 0, 0.5).is_err());
        assert!(rack.set(5, 0, 0.5).is_err());
        assert!(rack.get(0, 0).is_err());
        assert!(FxSlot::new(4).is_ok());
    }

    #[test]
    fn bad_index_rejected() {
        let mut rack = FxRack::new();
        assert!(rack.set(1, 64, 0.5).is_err());
        assert!(rack.get(1, 255).is_err());
        assert!(FxParamAddr::new(2, 63).is_ok());
        assert!(FxParamAddr::new(2, 64).is_err());
    }

    #[test]
    fn bad_value_rejected() {
        let mut rack = FxRack::new();
        assert!(rack.set(1, 0, f32::NAN).is_err());
        assert!(rack.set(1, 0, -0.1).is_err());
        assert!(rack.set(1, 0, 1.1).is_err());
    }

    #[test]
    fn design_type_tables_match_design_logic() {
        // Six types in UI order; labels mirror design-logic fxDefs.
        let labels: Vec<&str> = FX_TYPES.iter().map(|t| t.label).collect();
        assert_eq!(
            labels,
            vec!["Hall", "Plate", "Room", "Delay", "Chorus", "Flanger"]
        );
        for id in ["Hall", "Plate", "Room"] {
            let t = fx_type_meta(id).unwrap();
            let labels: Vec<&str> = t.params.iter().map(|p| p.label).collect();
            assert_eq!(
                labels,
                vec!["Decay", "Pre-delay", "Size", "Hi Damp", "Mix"],
                "{id}"
            );
            assert_eq!(t.params.len(), 5);
        }
        let delay = fx_type_meta("Delay").unwrap();
        let labels: Vec<&str> = delay.params.iter().map(|p| p.label).collect();
        assert_eq!(labels, vec!["Time", "Feedback", "Hi Cut", "Spread", "Mix"]);
        let chorus = fx_type_meta("Chorus").unwrap();
        let labels: Vec<&str> = chorus.params.iter().map(|p| p.label).collect();
        assert_eq!(labels, vec!["Rate", "Depth", "Spread", "Mix"]);
        let flanger = fx_type_meta("Flanger").unwrap();
        let labels: Vec<&str> = flanger.params.iter().map(|p| p.label).collect();
        assert_eq!(labels, vec!["Rate", "Depth", "Feedback", "Mix"]);
        // Seeded defaults mirror buildFx P rows.
        assert_eq!(fx_default_params("Hall"), vec![0.42, 0.15, 0.6, 0.55, 1.0]);
        assert_eq!(fx_default_params("Chorus"), vec![0.4, 0.5, 0.6, 1.0]);
        assert_eq!(DESIGN_FX_DEFAULT_TYPES.len(), 8);
        assert_eq!(fx_type_meta("hall").unwrap().label, "Hall");
        assert!(fx_type_meta("Reverb").is_none());
    }
}
