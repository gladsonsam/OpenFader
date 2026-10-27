//! DL32S-only requested list geometry. Sample k maps to requested address k;
//! pushed `start == 1` is the list ID, never an absolute meter address.
//!
//! Source: installed-native decoding report 2026-10-27 (native client 5.2,
//! build 28602, DLFresh32 selected by DL32S model 10). Native getters give
//! Return IN 258+4r, FX SEND 266+f, FX return PREFADER L/R 270+2f/271+2f,
//! Subgroup OUT 278+4s, LR OUT 302/303, Aux OUT 310+4a. The accompanying
//! 73 passive explicit-116-word frames corroborate list ordering and raw
//! f32 decoding, not physical output calibration or post-fader tap timing.
//! This 62-word grouping is our product list, not a vendor canonical list.

pub const INPUT_COUNT: usize = 32;
pub const RETURN_OFFSET: usize = INPUT_COUNT;
pub const RETURN_COUNT: usize = 2;
pub const FX_SEND_OFFSET: usize = RETURN_OFFSET + RETURN_COUNT;
pub const FX_COUNT: usize = 4;
pub const AUX_OFFSET: usize = FX_SEND_OFFSET + FX_COUNT;
pub const AUX_COUNT: usize = 8;
pub const FX_RETURN_OFFSET: usize = AUX_OFFSET + AUX_COUNT;
pub const STEREO_WIDTH: usize = 2;
pub const SUBGROUP_OFFSET: usize = FX_RETURN_OFFSET + FX_COUNT * STEREO_WIDTH;
pub const SUBGROUP_COUNT: usize = 6;
pub const MAIN_OFFSET: usize = SUBGROUP_OFFSET + SUBGROUP_COUNT;
pub const STRIP_COUNT: usize = MAIN_OFFSET + STEREO_WIDTH;
pub const RTA_OFFSET: usize = STRIP_COUNT;
pub const RTA_BIN_COUNT: usize = 31;
pub const RTA_FRAME_LEN: usize = RTA_OFFSET + RTA_BIN_COUNT;
pub const DYNAMICS_OFFSET: usize = RTA_FRAME_LEN;
pub const DYNAMICS_WIDTH: usize = 6;
pub const DYNAMICS_FRAME_LEN: usize = DYNAMICS_OFFSET + INPUT_COUNT * DYNAMICS_WIDTH;
pub const OUTPUT_GR_OFFSET: usize = DYNAMICS_FRAME_LEN;
pub const SUBGROUP_GR_OFFSET: usize = OUTPUT_GR_OFFSET + AUX_COUNT;
pub const MAIN_GR_OFFSET: usize = SUBGROUP_GR_OFFSET + SUBGROUP_COUNT;
pub const OUTPUT_GR_COUNT: usize = AUX_COUNT + SUBGROUP_COUNT + STEREO_WIDTH;
pub const OUTPUT_GR_FRAME_LEN: usize = OUTPUT_GR_OFFSET + OUTPUT_GR_COUNT;
pub const OUTPUT_GR_ADDRESSES: [i32; OUTPUT_GR_COUNT] = output_gr_addresses();
pub const LEGACY_FRAME_LEN: usize = 60;

/// Exact shapes produced by our explicit lists; legacy only supports input IN.
pub fn is_explicit_shape(len: usize) -> bool {
    matches!(len, STRIP_COUNT | RTA_FRAME_LEN | DYNAMICS_FRAME_LEN | OUTPUT_GR_FRAME_LEN)
}

pub const STRIP_ADDRESSES: [i32; STRIP_COUNT] = strip_addresses();

const fn strip_addresses() -> [i32; STRIP_COUNT] {
    let mut a = [0; STRIP_COUNT];
    let mut i = 0;
    while i < INPUT_COUNT {
        a[i] = 34 + 7 * i as i32;
        i += 1;
    }
    i = 0;
    while i < RETURN_COUNT {
        a[RETURN_OFFSET + i] = 258 + 4 * i as i32;
        i += 1;
    }
    i = 0;
    while i < FX_COUNT {
        a[FX_SEND_OFFSET + i] = 266 + i as i32;
        i += 1;
    }
    i = 0;
    while i < AUX_COUNT {
        a[AUX_OFFSET + i] = 310 + 4 * i as i32;
        i += 1;
    }
    i = 0;
    while i < FX_COUNT * STEREO_WIDTH {
        a[FX_RETURN_OFFSET + i] = 270 + i as i32;
        i += 1;
    }
    i = 0;
    while i < SUBGROUP_COUNT {
        a[SUBGROUP_OFFSET + i] = 278 + 4 * i as i32;
        i += 1;
    }
    a[MAIN_OFFSET] = 302;
    a[MAIN_OFFSET + 1] = 303;
    a
}

// Native COMP_GR getters, in provider order: Aux, Subgroup, Main L/R.
const fn output_gr_addresses() -> [i32; OUTPUT_GR_COUNT] {
    let mut a = [0; OUTPUT_GR_COUNT];
    let mut i = 0;
    while i < AUX_COUNT {
        a[i] = 312 + 4 * i as i32;
        i += 1;
    }
    i = 0;
    while i < SUBGROUP_COUNT {
        a[AUX_COUNT + i] = 280 + 4 * i as i32;
        i += 1;
    }
    a[OUTPUT_GR_COUNT - 2] = 306;
    a[OUTPUT_GR_COUNT - 1] = 307;
    a
}
