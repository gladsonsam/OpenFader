//! X-Air channel insert (`{root}/insert/on`, `{root}/insert/sel`).
//!
//! Doc sources (address shapes):
//!
//! - mixer control software protocol map (mixer control software protocol map §7):
//!   `insert ,ii` = **(on, sel)** — the container carries exactly the two
//!   leaves below, no position leaf.
//! - Owner full-plane capture
//!   (mixer control software 1.8.1 live full-plane capture (2026-10-19)):
//!   `/ch/01..16`, `/bus/1..6` and `/lr` answer `insert`, `insert/on` and
//!   `insert/sel`; `/rtn/*`, `/rtn/aux`, `/fxsend/*` and `/dca/*` carry no
//!   insert node (silence, not guessed).
//!
//! Owner-rig verified 2026-10-20 (XR18 FW 1.20, read + same-value/safe
//! writes, desk left sane): `{root}/insert ,ii` = (on, sel);
//! `insert/on` int `0`/`1` (bypass switch — verified write 1→0→1 on muted
//! ch15, restored); `insert/sel` int `0..=8` (writes `9..=12` ignored —
//! readback stays `8`; full 0..8 sweep verified on insert-off ch01,
//! restored to 0). Candidate position leaves (`insert/pos|pre|post|` +
//! `posteq`, `/lr/insert/pos`) are all silent — the desk exposes NO insert
//! position, so there is no `insert_pos` id.
//!
//! The `sel` values are NOT 1..4 FX slots (live ch15 = 7, ch16 = 8), so no
//! slot mapping is asserted: the driver carries the raw `0..=8` int and the
//! catalogue row is a plain enum. Do not invent an FX-slot meaning here.
use rosc::OscType;

pub const INSERT_SEL_MIN: i32 = 0;
/// Highest insert-select value (writes above this are ignored by the desk).
pub const INSERT_SEL_MAX: i32 = 8;

/// ```
/// # use openfader_proto_xair::insert::insert_on;
/// assert_eq!(insert_on("/ch/01"), "/ch/01/insert/on");
/// assert_eq!(insert_on("/bus/1"), "/bus/1/insert/on");
/// assert_eq!(insert_on("/lr"), "/lr/insert/on");
/// ```
pub fn insert_on(root: &str) -> String {
    format!("{root}/insert/on")
}

/// ```
/// # use openfader_proto_xair::insert::insert_sel;
/// assert_eq!(insert_sel("/ch/15"), "/ch/15/insert/sel");
/// assert_eq!(insert_sel("/lr"), "/lr/insert/sel");
/// ```
pub fn insert_sel(root: &str) -> String {
    format!("{root}/insert/sel")
}

/// Validated before any I/O so out-of-range never hits the desk.
pub fn sel_to_arg(v: f32) -> anyhow::Result<OscType> {
    if !v.is_finite() || v.fract() != 0.0 {
        anyhow::bail!("insert sel must be a whole number, got {v}");
    }
    let i = v as i32;
    if !(INSERT_SEL_MIN..=INSERT_SEL_MAX).contains(&i) {
        anyhow::bail!("insert sel {i} out of range {INSERT_SEL_MIN}..={INSERT_SEL_MAX}");
    }
    Ok(OscType::Int(i))
}

pub fn sel_from_msg(msg: &rosc::OscMessage) -> anyhow::Result<i32> {
    match msg.args.first() {
        Some(OscType::Int(i)) => {
            if !(INSERT_SEL_MIN..=INSERT_SEL_MAX).contains(i) {
                anyhow::bail!("{}: insert sel {i} outside {INSERT_SEL_MIN}..={INSERT_SEL_MAX}", msg.addr);
            }
            Ok(*i)
        }
        other => anyhow::bail!("{}: insert sel needs an int arg, got {other:?}", msg.addr),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_shapes() {
        assert_eq!(insert_on("/ch/01"), "/ch/01/insert/on");
        assert_eq!(insert_on("/bus/1"), "/bus/1/insert/on");
        assert_eq!(insert_on("/lr"), "/lr/insert/on");
        assert_eq!(insert_sel("/ch/15"), "/ch/15/insert/sel");
        assert_eq!(insert_sel("/bus/6"), "/bus/6/insert/sel");
        assert_eq!(insert_sel("/lr"), "/lr/insert/sel");
    }

    #[test]
    fn sel_range_matches_desk() {
        // Desk truth 2026-10-20: 0..=8 accepted, 9+ ignored.
        assert_eq!(sel_to_arg(0.0).unwrap(), OscType::Int(0));
        assert_eq!(sel_to_arg(8.0).unwrap(), OscType::Int(8));
        assert!(sel_to_arg(-1.0).is_err());
        assert!(sel_to_arg(9.0).is_err());
        assert!(sel_to_arg(1.5).is_err());
        assert!(sel_to_arg(f32::NAN).is_err());
    }
}
