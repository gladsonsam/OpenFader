//! Gate/dynamics gain-reduction (GR) meter decoding for X-Air.
//!
//! There is NO per-channel GR query address: neither the hardware baseline
//! (XR18 baseline notes — no `/ch/XX/gate/gr` or `/ch/XX/dyn/gr` was
//! ever observed) nor protocol reference X32 OSC protocol documentation (the
//! only `gr` under `/ch/` there is `grp`, i.e. mute *groups*) defines one.
//! Per the no-guessing rule, GR reads via OSC query are marked UNSUPPORTED
//! — GR arrives only inside meter blobs, decoded below.
//!
//! Documented sources (Behringer *X AIR Mixer OSC Document*, fw 1.11 —
//! community/Behringer-published; owner XR18 FW 1.20 verified 2026-10-20):
//!
//! * `/meters/6` ALL DYN: one blob of a 4-byte size word + 39 × i16LE at
//!   1/256 dB: 16 gate GR (ch 1..16), 16 dyn GR (ch 1..16), 6 bus dyn GR,
//!   1 LR dyn GR. The 39-slot shape is corroborated by `-stat/keysolo`
//!   (`OFF, G01..G16, D01..D16, DB1..DB6, DLR` = 39) in the xr-meters
//!   `parameters.txt` dump of the XR18 firmware.
//! * `/meters/0` + channel id: 8 per-channel meters (pre-fader l/r, gate +
//!   comp GR, post-fader l/r, gate key, comp key) for ONE channel. Known
//!   layout, no parser here — one `/meters/6` subscribe covers all 16
//!   channels, so that is what the driver requests (see
//!   [`crate::addrs::METER_DYN_SUBSCRIBE`]).
//!
//! Do NOT confuse with X32 `/meters/6 <channel_id>` (protocol reference doc): on X32
//! that id is a per-channel strip meter, on X-Air `/meters/6` is the
//! all-dynamics blob above. Different desks, different namespaces.
//!
//! Owner rig: /meters/6 delivered 160 packets / 8 s (20 Hz), 82 B / 39
//! samples. Input gate reductions observed; comp/bus/LR slots read 0 dB.
//! Nonzero bus/LR reduction remains unexercised.
//!
//! Wire encoding (signed 16 bit, 1/256 dB): each
//! sample is `dB * 256` as i16LE, `<= 0` (reduction). Positive samples, if
//! ever seen, are clamped to 0 dB reduction rather than interpreted.
//!
//! Exposure path (chosen: **param path**, least invention): the shared
//! meter hub (`openfader-transport`) only carries level-only [`openfader_core`]
//! [`MeterFrame`](openfader_core::MeterFrame)s, and extending it is outside
//! this crate's scope — so the driver caches the last decoded [`DynMeters`]
//! and serves `gate_gr` / `comp_gr` (dB reduction `>= 0` as
//! [`ParamValue::Float`](openfader_core::ParamValue)) from
//! [`MixerDriver::get_param`](openfader_core::MixerDriver::get_param). No-data
//! (no blob ingested yet) is an `Err`, so UIs keep the static scale. The
//! ingestion point is [`crate::XAirDriver::ingest_dyn_message`]; the desk
//! starts streaming once subscribed (see `subscribe_meters`).

/// Channels covered by the ALL DYN blob (XR18: ch 1..16, T3).
pub const DYN_CHANNELS: usize = 16;
/// Bus dyn slots in the blob (bus 1..6).
pub const DYN_BUSES: usize = 6;
/// Total GR samples: 16 gate + 16 dyn + 6 bus + 1 LR.
pub const DYN_SAMPLE_COUNT: usize = DYN_CHANNELS + DYN_CHANNELS + DYN_BUSES + 1;
/// Blob layout: 4-byte LE size word + samples.
pub const DYN_BLOB_LEN: usize = 4 + DYN_SAMPLE_COUNT * 2;

/// Gain reduction in dB (`>= 0`; 0 = no reduction), decoded from one
/// `/meters/6` ALL DYN blob.
#[derive(Clone, Debug, PartialEq)]
pub struct DynMeters {
    /// Per-channel gate GR, index 0 = ch 1.
    pub gate: [f32; DYN_CHANNELS],
    /// Per-channel dynamics (comp) GR, index 0 = ch 1.
    pub comp: [f32; DYN_CHANNELS],
    /// Per-bus dynamics GR, index 0 = bus 1.
    pub bus: [f32; DYN_BUSES],
    /// Main LR dynamics GR.
    pub lr: f32,
}

impl DynMeters {
    /// Gate GR for a 0-based strip (`StripId.0`); bails when out of range
    /// instead of wrapping — phantom strips (T3) must never alias ch 1.
    pub fn gate_gr(&self, strip: u16) -> anyhow::Result<f32> {
        self.gate
            .get(strip as usize)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("gate_gr: strip {strip} out of range 0..{DYN_CHANNELS}"))
    }

    /// Dynamics (comp) GR for a 0-based strip; same bounds as [`gate_gr`](Self::gate_gr).
    pub fn comp_gr(&self, strip: u16) -> anyhow::Result<f32> {
        self.comp
            .get(strip as usize)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("comp_gr: strip {strip} out of range 0..{DYN_CHANNELS}"))
    }
}

/// Decode a raw `/meters/6` blob payload into [`DynMeters`].
///
/// Shape: u32LE sample count (must be [`DYN_SAMPLE_COUNT`], else bail — no
/// guessing at unknown layouts) + that many i16LE samples at 1/256 dB
/// (`<= 0`; stored as reduction `>= 0`).
pub fn parse_dyn_blob(blob: &[u8]) -> anyhow::Result<DynMeters> {
    if blob.len() != DYN_BLOB_LEN {
        anyhow::bail!(
            "/meters/6: expected {DYN_BLOB_LEN} bytes (4 + 2*{DYN_SAMPLE_COUNT}), got {}",
            blob.len()
        );
    }
    let count = u32::from_le_bytes([blob[0], blob[1], blob[2], blob[3]]);
    if count as usize != DYN_SAMPLE_COUNT {
        anyhow::bail!("/meters/6: size word {count} != {DYN_SAMPLE_COUNT}, unsupported layout");
    }
    let mut samples = [0f32; DYN_SAMPLE_COUNT];
    for (i, slot) in samples.iter_mut().enumerate() {
        let off = 4 + i * 2;
        let raw = i16::from_le_bytes([blob[off], blob[off + 1]]);
        // Wire dB is <= 0; reduction is its negation clamped at 0.
        *slot = (-(raw as f32) / 256.0).max(0.0);
    }
    let mut gate = [0f32; DYN_CHANNELS];
    let mut comp = [0f32; DYN_CHANNELS];
    let mut bus = [0f32; DYN_BUSES];
    gate.copy_from_slice(&samples[0..DYN_CHANNELS]);
    comp.copy_from_slice(&samples[DYN_CHANNELS..2 * DYN_CHANNELS]);
    bus.copy_from_slice(&samples[2 * DYN_CHANNELS..2 * DYN_CHANNELS + DYN_BUSES]);
    let lr = samples[DYN_SAMPLE_COUNT - 1];
    Ok(DynMeters { gate, comp, bus, lr })
}

/// Encode [`DynMeters`] in desk shape (u32LE count + i16LE @1/256 dB).
/// Test/fake helper: mirrors the documented encoding so loopback tests
/// prove the wire shape survives OSC blob encode/decode.
pub fn build_dyn_blob(m: &DynMeters) -> Vec<u8> {
    let mut out = Vec::with_capacity(DYN_BLOB_LEN);
    out.extend_from_slice(&(DYN_SAMPLE_COUNT as u32).to_le_bytes());
    let encode = |reduction: f32, out: &mut Vec<u8>| {
        // Reduction >= 0 -> wire dB <= 0; i16 range holds ±128 dB.
        let raw = (-reduction.max(0.0) * 256.0).round().clamp(-32768.0, 0.0) as i16;
        out.extend_from_slice(&raw.to_le_bytes());
    };
    for v in m.gate {
        encode(v, &mut out);
    }
    for v in m.comp {
        encode(v, &mut out);
    }
    for v in m.bus {
        encode(v, &mut out);
    }
    encode(m.lr, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> DynMeters {
        let mut m = DynMeters {
            gate: [0.0; DYN_CHANNELS],
            comp: [0.0; DYN_CHANNELS],
            bus: [0.0; DYN_BUSES],
            lr: 0.0,
        };
        m.gate[0] = 6.0;
        m.comp[0] = 12.0;
        m.comp[15] = 3.5;
        m.bus[5] = 1.5;
        m.lr = 2.0;
        m
    }

    #[test]
    fn blob_roundtrip() {
        let back = parse_dyn_blob(&build_dyn_blob(&sample())).unwrap();
        assert_eq!(back, sample());
    }

    #[test]
    fn blob_rejects_wrong_shapes() {
        // Short blob.
        assert!(parse_dyn_blob(&[0u8; 10]).is_err());
        // Right length, wrong size word.
        let mut bad = build_dyn_blob(&sample());
        bad[0] = 0x07;
        assert!(parse_dyn_blob(&bad).is_err());
    }

    #[test]
    fn strip_bounds() {
        let m = sample();
        assert_eq!(m.gate_gr(0).unwrap(), 6.0);
        assert_eq!(m.comp_gr(15).unwrap(), 3.5);
        // Phantom strips (T3) and beyond never alias: hard error.
        assert!(m.gate_gr(16).is_err());
        assert!(m.comp_gr(99).is_err());
    }

    #[test]
    fn positive_wire_clamps_to_zero() {
        // A hypothetical positive sample means "no reduction" here, never
        // negative GR — clamp rather than interpret.
        let mut blob = build_dyn_blob(&sample());
        // First gate sample bytes -> +256 (+1 dB on the wire).
        blob[4] = 0x00;
        blob[5] = 0x01;
        let back = parse_dyn_blob(&blob).unwrap();
        assert_eq!(back.gate[0], 0.0);
    }

    /// Fuzz-ish sweep: every truncation, every overlong extension, wrong
    /// size words, and garbage fills must return `Err`, never panic
    /// (truncated venue datagrams land here via `ingest_dyn_message`).
    #[test]
    fn fuzz_lengths_never_panic() {
        let good = build_dyn_blob(&sample());
        assert_eq!(good.len(), DYN_BLOB_LEN);
        for len in 0..good.len() {
            assert!(
                parse_dyn_blob(&good[..len]).is_err(),
                "truncated len {len} accepted"
            );
        }
        let mut long = good.clone();
        long.extend_from_slice(&[0u8; 16]);
        assert!(parse_dyn_blob(&long).is_err());
        for w in [0u32, 1, 38, 40, 0xFFFF_FFFF] {
            let mut bad = good.clone();
            bad[0..4].copy_from_slice(&w.to_le_bytes());
            assert!(
                parse_dyn_blob(&bad).is_err(),
                "size word {w} accepted"
            );
        }
        // Exact-length garbage fills: size word never matches 39.
        assert!(parse_dyn_blob(&[0xABu8; DYN_BLOB_LEN]).is_err());
        assert!(parse_dyn_blob(&[0u8; DYN_BLOB_LEN]).is_err());
        assert!(parse_dyn_blob(&[]).is_err());
        // The valid blob itself still decodes.
        assert_eq!(parse_dyn_blob(&good).unwrap(), sample());
    }
}
