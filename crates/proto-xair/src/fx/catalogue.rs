//! Algorithm metadata and parameter lookup.
use super::*;
/// Split a single-block base table into A/B halves: pars `1..=len` bank
/// Split a single-block base table into A/B halves: pars `1..=len` bank
/// `"A"`, `len+1..=2*len` bank `"B"`. Anything else is padding (`None`).
fn dual(base: &'static [FxParamDef], par: u8) -> Option<FxParamDef> {
    let half = base.len() as u8;
    if half == 0 || par == 0 {
        return None;
    }
    let (idx, bank) = if par <= half {
        (par - 1, "A")
    } else if par <= half * 2 {
        (par - 1 - half, "B")
    } else {
        return None;
    };
    base.get(idx as usize).map(|d| FxParamDef {
        par,
        bank: Some(bank),
        ..*d
    })
}

/// Graphic-EQ def (GEQ/TEQ): band centres `1..=31`, master `Lvl` at 32.
fn geq_def(par: u8, bank: Option<&'static str>) -> Option<FxParamDef> {
    if (1..=31).contains(&par) {
        Some(FxParamDef {
            par,
            label: GEQ_BANDS[(par - 1) as usize],
            unit: "Hz",
            min: "-15",
            max: "+15",
            options: None,
            inferred: false,
            bank,
        })
    } else if par == 32 {
        Some(FxParamDef {
            par,
            label: "Lvl",
            unit: "dB",
            min: "-15",
            max: "+15",
            options: None,
            inferred: false,
            bank,
        })
    } else {
        None
    }
}

/// Dual graphic-EQ def (GEQ2/TEQ2): A block `1..=32`, B block `33..=64`.
fn geq2_def(par: u8) -> Option<FxParamDef> {
    if (1..=32).contains(&par) {
        geq_def(par, Some("A"))
    } else if (33..=64).contains(&par) {
        geq_def(par - 32, Some("B")).map(|d| FxParamDef { par, ..d })
    } else {
        None
    }
}

/// Short display name for an effect-type index (`None` = undocumented
/// index — index 15 is a gap in the wire order; treat like unknown).
/// This is the desk code (`HALL`, `DLY`, …): stable wire key, used as the
/// `caps.fx` type `id`. Human-facing UI shows [`fx_effect_full_name`].
///
/// ```
/// # use openfader_proto_xair::fx::fx_effect_name;
/// assert_eq!(fx_effect_name(0), Some("HALL"));
/// assert_eq!(fx_effect_name(60), Some("PIT"));
/// assert_eq!(fx_effect_name(15), None);
/// assert_eq!(fx_effect_name(61), None);
/// ```
pub fn fx_effect_name(index: u8) -> Option<&'static str> {
    let (name, _, _) = fx_effect_meta(index)?;
    Some(name)
}

/// Full display name for an effect-type index (`None` = undocumented
/// index, same gap rule as [`fx_effect_name`]).
///
/// Provenance: 58 of the 60 names are the X-AIR-Edit 1.8.1 rack-select
/// strings (see X-Air Edit 1.8.1 app structure capture §4 —
/// our own prior strings inventory, names only, no artwork): matched to
/// wire indices by family/param semantics (the rack-select order is
/// grouped by category, NOT wire order). Two are clean-room inferences,
/// marked below: `PLAT` has no entry in the captured strings
/// ("Plate Reverb" from the code + plate param family) and `ULC2` takes
/// "Dual Ultimo Compressor" (the captured "Stereo Ultimo Compressor"
/// has no corresponding wire index in the 0..=60 order — possibly a
/// single-channel variant outside this table; never asserted).
///
/// ```
/// # use openfader_proto_xair::fx::fx_effect_full_name;
/// assert_eq!(fx_effect_full_name(0), Some("Hall Reverb"));
/// assert_eq!(fx_effect_full_name(10), Some("Stereo Delay"));
/// assert_eq!(fx_effect_full_name(60), Some("Stereo Pitch Shifter"));
/// assert_eq!(fx_effect_full_name(15), None);
/// ```
pub fn fx_effect_full_name(index: u8) -> Option<&'static str> {
    let (_, full, _) = fx_effect_meta(index)?;
    Some(full)
}

/// ```
/// # use openfader_proto_xair::fx::fx_param_count;
/// assert_eq!(fx_param_count(0), Some(12));
/// assert_eq!(fx_param_count(28), Some(64));
/// assert_eq!(fx_param_count(15), None);
/// ```
pub fn fx_param_count(index: u8) -> Option<u8> {
    let (_, _, used) = fx_effect_meta(index)?;
    Some(used)
}

fn fx_effect_meta(index: u8) -> Option<(&'static str, &'static str, u8)> {
    match index {
        0 => Some(("HALL", "Hall Reverb", 12)),
        1 => Some(("AMBI", "Ambience", 10)),
        2 => Some(("RPLT", "Rich Plate Reverb", 16)),
        3 => Some(("ROOM", "Room Reverb", 16)),
        4 => Some(("CHAM", "Chamber Reverb", 16)),
        // Inferred: no "Plate Reverb" in the captured rack-select strings.
        5 => Some(("PLAT", "Plate Reverb", 12)),
        6 => Some(("VREV", "Vintage Reverb", 10)),
        7 => Some(("VRM", "Vintage Room", 13)),
        8 => Some(("GATE", "Gated Reverb", 10)),
        9 => Some(("RVRS", "Reverse Reverb", 9)),
        10 => Some(("DLY", "Stereo Delay", 12)),
        11 => Some(("3TAP", "3-Tap Delay", 15)),
        12 => Some(("4TAP", "Rhythm Delay", 15)),
        13 => Some(("CRS", "Stereo Chorus", 11)),
        14 => Some(("FLNG", "Stereo Flanger", 12)),
        // 15: undocumented gap (FLNG 14 → PHAS 16) — generic fallback.
        16 => Some(("PHAS", "Stereo Phaser", 12)),
        17 => Some(("DIMC", "Dimensional Chorus", 7)),
        18 => Some(("FILT", "Mood Filter", 14)),
        19 => Some(("ROTA", "Rotary Speaker", 8)),
        20 => Some(("PAN", "Tremolo / Panner", 9)),
        21 => Some(("SUB", "Suboctaver", 10)),
        22 => Some(("D/RV", "Delay + Chamber", 12)),
        23 => Some(("CR/R", "Chorus + Chamber", 12)),
        24 => Some(("FL/R", "Flanger + Chamber", 12)),
        25 => Some(("D/CR", "Delay + Chorus", 12)),
        26 => Some(("D/FL", "Delay + Flanger", 12)),
        27 => Some(("MODD", "Modulation Delay", 13)),
        28 => Some(("GEQ2", "Dual Graphic EQ", 64)),
        29 => Some(("GEQ", "Stereo Graphic EQ", 32)),
        30 => Some(("TEQ2", "Dual TruEQ", 64)),
        31 => Some(("TEQ", "Stereo TruEQ", 32)),
        32 => Some(("DES2", "Dual DeEsser", 6)),
        33 => Some(("DES", "Stereo DeEsser", 6)),
        34 => Some(("P1A", "Stereo Xtec EQ1", 11)),
        35 => Some(("P1A2", "Dual Xtec EQ1", 22)),
        36 => Some(("PQ5", "Stereo Xtec EQ5", 9)),
        37 => Some(("PQ5S", "Dual Xtec EQ5", 18)),
        38 => Some(("WAVD", "Wave Designer", 6)),
        39 => Some(("LIM", "Precision Limiter", 8)),
        40 => Some(("CMB", "Stereo Combinator", 29)),
        41 => Some(("CMB2", "Dual Combinator", 58)),
        42 => Some(("FAC", "Stereo Fair Compressor", 7)),
        43 => Some(("FAC1M", "M/S Fair Compressor", 14)),
        44 => Some(("FAC2", "Dual Fair Compressor", 14)),
        45 => Some(("LEC", "Stereo Leisure Compressor", 5)),
        46 => Some(("LEC2", "Dual Leisure Compressor", 10)),
        // 47/48 are contested in the notes: the wire-order grid (stated
        // doubly-sourced, count-checked) says 47=ULC2/48=ENH2, while the
        // section headers claim ULC(47), ULC2(48) AND ENH2(48) — 48 is
        // double-claimed, so at least one section number is a typo. The
        // grid wins for names; every row of 47 is forced inferred (see
        // fx_param_def) and 48 is wholly inferred already (flagged layout).
        // "Dual Ultimo Compressor": the captured strings also list a
        // "Stereo Ultimo Compressor" with no wire index here (see
        // fx_effect_full_name docs) — never asserted.
        47 => Some(("ULC2", "Dual Ultimo Compressor", 12)),
        48 => Some(("ENH2", "Dual Enhancer", 16)),
        49 => Some(("ENH", "Stereo Enhancer", 9)),
        50 => Some(("EXC2", "Dual Exciter", 14)),
        51 => Some(("EXC", "Stereo Exciter", 7)),
        52 => Some(("IMG", "Stereo Imager", 7)),
        53 => Some(("EDI", "Edison EX1", 8)),
        54 => Some(("SON", "Sound Maxer", 8)),
        55 => Some(("AMP2", "Dual Guitar Amp", 18)),
        56 => Some(("AMP", "Stereo Guitar Amp", 9)),
        57 => Some(("DRV2", "Dual Tube Stage", 20)),
        58 => Some(("DRV", "Stereo Tube Stage", 10)),
        59 => Some(("PIT2", "Dual Pitch Shifter", 12)),
        60 => Some(("PIT", "Stereo Pitch Shifter", 6)),
        _ => None,
    }
}

/// Labeled def for 1-based `par` of effect-type `index` (`None` = padding
/// par or unknown effect — the UI hides padding and falls back to generic
/// numbered sliders for unknown types).
///
/// ```
/// # use openfader_proto_xair::fx::fx_param_def;
/// let d = fx_param_def(0, 1).unwrap();
/// assert_eq!((d.label, d.unit), ("PreDelay", "ms"));
/// assert!(fx_param_def(0, 13).is_none()); // HALL padding
/// assert!(fx_param_def(99, 1).is_none()); // unknown effect
/// ```
pub fn fx_param_def(index: u8, par: u8) -> Option<FxParamDef> {
    if par == 0 || par > PARAM_COUNT as u8 {
        return None;
    }
    // NOTE: indices 47–60 follow the wire-order grid (doubly-sourced),
    // NOT the research file's section order — e.g. 47 is ULC2 (not ULC),
    // 48 is ENH2, 50 is EXC2, 59 is PIT2 and 60 is PIT. The meta table
    // above is the check.
    match index {
        0 => HALL.get((par - 1) as usize).copied(),
        1 => AMBI.get((par - 1) as usize).copied(),
        2 => RPLT.get((par - 1) as usize).copied(),
        3 => ROOM.get((par - 1) as usize).copied(),
        4 => CHAM.get((par - 1) as usize).copied(),
        5 => PLAT.get((par - 1) as usize).copied(),
        6 => VREV.get((par - 1) as usize).copied(),
        7 => VRM.get((par - 1) as usize).copied(),
        8 => GATE.get((par - 1) as usize).copied(),
        9 => RVRS.get((par - 1) as usize).copied(),
        10 => DLY.get((par - 1) as usize).copied(),
        11 => TAP3.get((par - 1) as usize).copied(),
        12 => TAP4.get((par - 1) as usize).copied(),
        13 => CRS.get((par - 1) as usize).copied(),
        14 => FLNG.get((par - 1) as usize).copied(),
        16 => PHAS.get((par - 1) as usize).copied(),
        17 => DIMC.get((par - 1) as usize).copied(),
        18 => FILT.get((par - 1) as usize).copied(),
        19 => ROTA.get((par - 1) as usize).copied(),
        20 => PAN.get((par - 1) as usize).copied(),
        21 => dual(SUB5, par),
        22 => DRV_COMBO.get((par - 1) as usize).copied(),
        23 => CRR.get((par - 1) as usize).copied(),
        24 => FLR.get((par - 1) as usize).copied(),
        25 => DCR.get((par - 1) as usize).copied(),
        26 => DFL.get((par - 1) as usize).copied(),
        27 => MODD.get((par - 1) as usize).copied(),
        28 => geq2_def(par),
        29 => geq_def(par, None),
        30 => geq2_def(par),
        31 => geq_def(par, None),
        32 => DES2.get((par - 1) as usize).copied(),
        33 => DES.get((par - 1) as usize).copied(),
        34 => P1A.get((par - 1) as usize).copied(),
        35 => dual(P1A, par),
        36 => PQ5.get((par - 1) as usize).copied(),
        37 => dual(PQ5, par),
        38 => WAVD.get((par - 1) as usize).copied(),
        39 => LIM.get((par - 1) as usize).copied(),
        40 => CMB.get((par - 1) as usize).copied(),
        41 => dual(CMB, par),
        42 => FAC.get((par - 1) as usize).copied(),
        // FAC1M header is inferred from block position (notes); same layout
        // as FAC2 but every row low-confidence.
        43 => dual(FAC, par).map(|d| FxParamDef {
            inferred: true,
            ..d
        }),
        44 => dual(FAC, par),
        45 => LEC.get((par - 1) as usize).copied(),
        46 => dual(LEC, par),
        // Contested identity (see meta): grid says ULC2, one section header
        // says ULC. The ULC2 A-block coincides with the ULC table either
        // way, so pars 1–6 are safe; every row is still forced inferred.
        47 => dual(ULC, par).map(|d| FxParamDef {
            inferred: true,
            ..d
        }),
        48 => ENH2.get((par - 1) as usize).copied(),
        49 => ENH.get((par - 1) as usize).copied(),
        50 => dual(EXC, par),
        51 => EXC.get((par - 1) as usize).copied(),
        52 => IMG.get((par - 1) as usize).copied(),
        53 => EDI.get((par - 1) as usize).copied(),
        54 => dual(SON4, par),
        55 => dual(AMP, par),
        56 => AMP.get((par - 1) as usize).copied(),
        57 => dual(DRV, par),
        58 => DRV.get((par - 1) as usize).copied(),
        59 => PIT2.get((par - 1) as usize).copied(),
        60 => PIT.get((par - 1) as usize).copied(),
        _ => None,
    }
}
