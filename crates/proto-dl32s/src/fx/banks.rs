//! Explicit DL32S snapshot descriptors, not DL32R geometry or inferred stride.
//! Each word resolves independently in gDLF32SnapshotParameterTable.
pub(super) const BANKS: [[u32; 31]; 4] = [
    [
        3629, 3630, 3631, 3632, 3633, 3634, 3635, 3636, 3637, 3638, 3639, 3640, 3641, 3642, 3643,
        3644, 3645, 3646, 3647, 3648, 3649, 3650, 3651, 3652, 3653, 3654, 3655, 3656, 3657, 3658,
        3659,
    ],
    [
        3660, 3661, 3662, 3663, 3664, 3665, 3666, 3667, 3668, 3669, 3670, 3671, 3672, 3673, 3674,
        3675, 3676, 3677, 3678, 3679, 3680, 3681, 3682, 3683, 3684, 3685, 3686, 3687, 3688, 3689,
        3690,
    ],
    [
        3691, 3692, 3693, 3694, 3695, 3696, 3697, 3698, 3699, 3700, 3701, 3702, 3703, 3704, 3705,
        3706, 3707, 3708, 3709, 3710, 3711, 3712, 3713, 3714, 3715, 3716, 3717, 3718, 3719, 3720,
        3721,
    ],
    [
        3722, 3723, 3724, 3725, 3726, 3727, 3728, 3729, 3730, 3731, 3732, 3733, 3734, 3735, 3736,
        3737, 3738, 3739, 3740, 3741, 3742, 3743, 3744, 3745, 3746, 3747, 3748, 3749, 3750, 3751,
        3752,
    ],
];
pub(super) fn common(slot: u16) -> impl Iterator<Item = &'static super::specs::FxParameterSpec> {
    super::specs::FX_PARAMETERS.iter().filter(move |s| {
        s.slot == slot
            && matches!(
                s.id,
                "mute" | "main_return" | "send_master" | "send_mute" | "send_source"
            )
    })
}
pub(super) fn addresses(slot: u16) -> Vec<u32> {
    let mut addresses = BANKS[slot as usize - 1].to_vec();
    addresses.extend(common(slot).map(|s| s.address));
    addresses.extend(super::source::addresses(slot));
    addresses.sort_unstable();
    addresses.dedup();
    addresses
}
