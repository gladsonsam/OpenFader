//! X-Air strip look (`/…/config/color`) plus retained `icon`/`link` arms.
//!
//! Doc sources (address shapes + wire codecs):
//!
//! - Behringer XR18 firmware OSC map, FW 1.14 (xr-meters `parameters.txt`,
//!   extracted from Behringer-provided firmware): the strip `config` block
//!   carries `name` plus a `color [0,15]` leaf. The map has NO `icon` leaf
//!   and NO per-channel `link` leaf (stereo link is the global
//!   `/config/chlink/1-2…` switch instead).
//! - xair-api-python `shared.py` (MIT, onyx-and-iris, dev branch, fetched
//!   2026-10-18): strip `Config` at `{root}/config` with int leaves.
//! - Patrick-Gilles Maillot's unofficial X32 OSC doc (`/tmp/x32.pdf`): backs
//!   the `config/color` leaf vocabulary on X32 channels.
//!
//! Owner-rig verified (XR18 FW 1.20: `/ch/01`, `/ch/16`, `/lr` `color` all
//! answer): `color` is in caps. `icon`/`link` are live-silent and out of
//! caps; their driver arms are retained (loopback-tested) only so direct
//! misuse errors locally instead of hitting silent addresses.

use rosc::OscType;

/// Desk channel-colour bounds (enum `0..=15`, name-bar colour).
pub const COLOR_MIN: i32 = 0;
pub const COLOR_MAX: i32 = 15;
/// Desk channel-icon bounds (enum `0..=31`).
pub const ICON_MIN: i32 = 0;
pub const ICON_MAX: i32 = 31;

/// ```
/// # use openfader_proto_xair::look::strip_color;
/// assert_eq!(strip_color("/ch/01"), "/ch/01/config/color");
/// assert_eq!(strip_color("/bus/1"), "/bus/1/config/color");
/// ```
pub fn strip_color(root: &str) -> String {
    format!("{root}/config/color")
}

/// ```
/// # use openfader_proto_xair::look::strip_icon;
/// assert_eq!(strip_icon("/ch/01"), "/ch/01/config/icon");
/// ```
pub fn strip_icon(root: &str) -> String {
    format!("{root}/config/icon")
}

/// ```
/// # use openfader_proto_xair::look::strip_link;
/// assert_eq!(strip_link("/ch/01"), "/ch/01/config/link");
/// ```
pub fn strip_link(root: &str) -> String {
    format!("{root}/config/link")
}

/// Whole-number floats `0..=15` become the wire int.
pub fn color_to_arg(v: f32) -> anyhow::Result<OscType> {
    crate::codec::int_to_arg(v, COLOR_MIN, COLOR_MAX, "look", "color")
}

/// Whole-number floats `0..=31` become the wire int.
pub fn icon_to_arg(v: f32) -> anyhow::Result<OscType> {
    crate::codec::int_to_arg(v, ICON_MIN, ICON_MAX, "look", "icon")
}

/// Whole-number floats accepted. Link switches use [`crate::codec::bool_arg`] instead.
pub fn int_from_msg(msg: &rosc::OscMessage, min: i32, max: i32, what: &str) -> anyhow::Result<i32> {
    crate::codec::int_from_msg(msg, min, max, "look", what)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_shapes() {
        assert_eq!(strip_color("/ch/01"), "/ch/01/config/color");
        assert_eq!(strip_color("/lr"), "/lr/config/color");
        assert_eq!(strip_icon("/ch/16"), "/ch/16/config/icon");
        assert_eq!(strip_link("/ch/02"), "/ch/02/config/link");
    }

    #[test]
    fn codec_shapes() {
        assert_eq!(color_to_arg(0.0).unwrap(), OscType::Int(0));
        assert_eq!(color_to_arg(15.0).unwrap(), OscType::Int(15));
        assert!(color_to_arg(16.0).is_err());
        assert!(color_to_arg(2.5).is_err());
        assert_eq!(icon_to_arg(31.0).unwrap(), OscType::Int(31));
        assert!(icon_to_arg(32.0).is_err());
    }
}
