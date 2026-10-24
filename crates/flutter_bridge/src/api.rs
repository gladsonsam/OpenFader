//! FRB entry points (`crate::api`): thin Cmd/Event JSON movers over the
//! embedded `openfader-engine` session. No mixer logic lives here.

use crate::{BridgeClient, DRAIN_CAP_DEFAULT, DRAIN_CAP_MAX, clients, engine, rt, set_engine};
use openfader_engine::{Engine, EngineConfig};

/// Boot the embedded engine on `sim` (idempotent) and attach the
/// `app` client. Returns the client id Dart passes to every other
/// call. Repeat calls attach a NEW client on the same session.
#[flutter_rust_bridge::frb(sync)]
pub fn boot() -> String {
    let eng = match engine() {
        Some(e) => e,
        None => {
            let e = Engine::new(EngineConfig::default());
            // Sim boot has no network: a failure means the engine itself is
            // broken (loud-propagate: fail fast).
            rt().block_on(e.connect("sim", None))
                .expect("openfader_flutter_bridge: embedded sim boot");
            set_engine(e.clone());
            e
        }
    };
    let handle = rt().block_on(eng.attach(Some("app".to_string())));
    let id = handle.id.clone();
    clients().lock().unwrap().insert(id.clone(), BridgeClient::new(handle));
    id
}

/// Send one engine Cmd JSON (`{"cmd":...}`, the WS relay spelling).
/// Rejections arrive as unicast `error` Events in the client's mailbox
/// (collect them with [`poll`]); unknown clients are ignored.
#[flutter_rust_bridge::frb]
pub async fn cmd(client_id: String, json: String) {
    if let Some(eng) = engine() {
        eng.handle_json(&client_id, &json).await;
    }
}

/// Drain queued Event JSON frames (broadcast-time serialization shared
/// across clients, byte-identical to the WS relay) for `client_id`, up to
/// `cap` frames (`0` = default). Overflow stays queued for the next call.
/// Unknown clients get `[]`.
///
/// Frames set aside by [`poll_binary`] (non-meter events seen while the
/// Dart side drained the binary meter path) come first, in order, the
/// JSON poll is the lossless fallback for everything binary does not
/// carry.
#[flutter_rust_bridge::frb]
pub async fn poll(client_id: String, cap: u32) -> Vec<String> {
    let cap = if cap == 0 {
        DRAIN_CAP_DEFAULT
    } else {
        (cap as usize).min(DRAIN_CAP_MAX)
    };
    let mut map = clients().lock().unwrap();
    let Some(client) = map.get_mut(&client_id) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    while out.len() < cap {
        if let Some(json) = client.pending_json.pop_front() {
            out.push(json);
        } else {
            break;
        }
    }
    while out.len() < cap {
        match client.handle.events.try_recv() {
            // The FRB boundary needs owned strings; no per-client `serde_json` run.
            Ok(frame) => out.push(frame.json.to_string()),
            Err(_) => break,
        }
    }
    out
}

/// Pack individual `to_binary` frames into one `poll_binary` return blob:
/// `[u32 LE len][frame]...`. Pure (no engine state) so the framing is
/// unit-testable; the Dart splitter mirrors this layout exactly.
pub(crate) fn encode_binary_batch(frames: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    for frame in frames {
        out.extend_from_slice(&(frame.len() as u32).to_le_bytes());
        out.extend_from_slice(frame);
    }
    out
}

/// Inverse of [`encode_binary_batch`] (test + doc mirror of the Dart
/// `splitBinaryBatch`): split a blob into its frames, stopping at a
/// short/truncated tail (never panics on hostile input).
#[cfg(test)]
pub(crate) fn split_binary_batch(blob: &[u8]) -> Vec<Vec<u8>> {
    let mut frames = Vec::new();
    let mut cursor = 0usize;
    while cursor + 4 <= blob.len() {
        let len = u32::from_le_bytes([
            blob[cursor],
            blob[cursor + 1],
            blob[cursor + 2],
            blob[cursor + 3],
        ]) as usize;
        cursor += 4;
        if cursor + len > blob.len() {
            break;
        }
        frames.push(blob[cursor..cursor + len].to_vec());
        cursor += len;
    }
    frames
}

/// Queue a fresh snapshot into an already-attached client's mailbox (the
/// resume path: same frame set as attach, minus `you`). No-op when the id
/// is not attached.
#[flutter_rust_bridge::frb]
pub async fn resync(client_id: String) {
    if let Some(eng) = engine() {
        eng.push_snapshot(&client_id).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openfader_engine::protocol::{
        BIN_HEADER_LEN, BIN_TYPE_GR, BIN_TYPE_METERS, BIN_TYPE_TAPS, BIN_VERSION, Event,
        level_to_u16,
    };

    fn header_of(frame: &[u8]) -> (u8, u8, u16, u32, u32) {
        (
            frame[0],
            frame[1],
            u16::from_le_bytes([frame[2], frame[3]]),
            u32::from_le_bytes([frame[4], frame[5], frame[6], frame[7]]),
            u32::from_le_bytes([frame[8], frame[9], frame[10], frame[11]]),
        )
    }

    #[test]
    fn binary_batch_round_trips_meter_frame_fields() {
        let ev = Event::Meters {
            levels: vec![0.0, 0.5, 1.0, 2.0, -1.0],
        };
        let frame = ev.to_binary(7, 1234).expect("meters encode binary");
        assert!(frame.len() >= BIN_HEADER_LEN);
        let (ty, ver, n, seq, t_ms) = header_of(&frame);
        assert_eq!(ty, BIN_TYPE_METERS);
        assert_eq!(ver, BIN_VERSION);
        assert_eq!(n, 5);
        assert_eq!(seq, 7);
        assert_eq!(t_ms, 1234);
        let payload = &frame[BIN_HEADER_LEN..];
        assert_eq!(payload.len(), 5 * 2);
        for (i, want) in [0.0f32, 0.5, 1.0, 1.0, 0.0].iter().enumerate() {
            let q = u16::from_le_bytes([payload[i * 2], payload[i * 2 + 1]]);
            assert_eq!(q, level_to_u16(*want), "sample {i}");
        }

        let blob = encode_binary_batch(std::slice::from_ref(&frame));
        assert_eq!(&blob[0..4], &((frame.len() as u32).to_le_bytes()));
        let split = split_binary_batch(&blob);
        assert_eq!(split, vec![frame]);
    }

    #[test]
    fn binary_batch_covers_gr_and_taps_layouts() {
        let gr = Event::Gr {
            ch: 3,
            gate: 4.0,
            comp: 8.0,
        };
        let frame = gr.to_binary(0, 99).expect("gr encodes binary");
        let (ty, ver, n, _, _) = header_of(&frame);
        assert_eq!((ty, ver, n), (BIN_TYPE_GR, BIN_VERSION, 1));
        assert_eq!(
            &frame[BIN_HEADER_LEN..],
            &[3u8, (4.0 * 4.0) as u8, (8.0 * 4.0) as u8]
        );

        let taps = Event::Taps {
            tap: "in".to_string(),
            levels: vec![0.25],
        };
        let frame = taps.to_binary(1, 100).expect("taps encode binary");
        let (ty, ver, n, seq, _) = header_of(&frame);
        assert_eq!((ty, ver, n, seq), (BIN_TYPE_TAPS, BIN_VERSION, 1, 1));
        assert_eq!(frame[BIN_HEADER_LEN], 0, "tap id for 'in'");
        let q = u16::from_le_bytes([frame[BIN_HEADER_LEN + 1], frame[BIN_HEADER_LEN + 2]]);
        assert_eq!(q, level_to_u16(0.25));
    }

    #[test]
    fn binary_skips_non_meter_events_for_json_fallback() {
        // Control frames must never enter the binary path.
        let state = Event::State {
            driver: "sim".to_string(),
            strips: vec![],
            main_fader: None,
            by: None,
        };
        assert!(state.to_binary(0, 0).is_none());
        let spectrum = Event::Spectrum {
            ch: 0,
            bins: vec![0.1],
        };
        assert!(spectrum.to_binary(0, 0).is_none());
    }

    #[test]
    fn binary_batch_split_stops_at_truncated_tail() {
        let frame = Event::Meters { levels: vec![0.5] }
            .to_binary(0, 0)
            .expect("meters encode binary");
        let mut blob = encode_binary_batch(&[frame.clone(), frame.clone()]);
        blob.truncate(blob.len() - 3);
        let split = split_binary_batch(&blob);
        assert_eq!(split.len(), 1, "first frame survives, tail dropped");
        assert!(split_binary_batch(&blob[0..2]).is_empty());
        assert!(split_binary_batch(&[]).is_empty());
    }

    #[test]
    fn poll_binary_stashes_control_frames_for_poll_fallback() {
        let id = boot();
        let blob = rt().block_on(poll_binary(id.clone(), 512));
        for frame in split_binary_batch(&blob) {
            assert!(frame.len() >= BIN_HEADER_LEN);
            assert_eq!(frame[1], BIN_VERSION, "binary gate version");
            assert!(
                frame[0] == BIN_TYPE_METERS
                    || frame[0] == BIN_TYPE_GR
                    || frame[0] == BIN_TYPE_TAPS,
                "only meter-class frames go binary"
            );
        }
        let json = rt().block_on(poll(id.clone(), 512));
        assert!(
            !json.is_empty(),
            "stashed attach snapshot must survive poll_binary"
        );
        let has_state = json.iter().any(|f| f.contains("\"state\""));
        assert!(has_state, "state frame falls back to JSON poll");
        for f in &json {
            let v: serde_json::Value =
                serde_json::from_str(f).expect("fallback frames stay valid JSON");
            assert!(v.get("type").is_some());
        }
    }
}

/// Edge stats: queue depth behind the push fan-out for `client_id`
/// (same shape as the engine's `ipc_stats`, minus the counters the
/// collapse layer will own, see the module TODO).
#[flutter_rust_bridge::frb(sync)]
pub fn stats(client_id: String) -> String {
    let depth = clients()
        .lock()
        .unwrap()
        .get(&client_id)
        .map(|c| c.handle.events.len() + c.pending_json.len())
        .unwrap_or(0);
    serde_json::json!({
        "type": "ipc_stats",
        "queue_depth": depth,
    })
    .to_string()
}

/// Drain up to `cap` queued events (`0` = default) for `client_id` and
/// return the meter-class ones as compact binary frames.
///
/// Return layout: concatenated length-prefixed frames, each
/// `[u32 LE frame_len][frame bytes...]`, where each frame is exactly one
/// `Event::to_binary` encoding (protocol.rs: `[u8 type, u8 version=1,
/// u16 n, u32 seq, u32 t_ms]` header + payload, little-endian). `seq` is
/// this client's monotonic binary-frame counter (starts at 0); `t_ms` is
/// `Engine::now_ms` captured once per call. Empty return = no meter
/// frames queued (not an error).
///
/// Framing/version compat: the Dart side splits on the `u32` length
/// prefix and decodes each frame with the existing `parseBinary`
/// counterpart of `to_binary`, same type bytes (`1=meters, 2=gr,
/// 3=taps`), same `BIN_VERSION` gate (unknown versions decode to null
/// and are skipped), same payload quantisation. A layout bump in
/// protocol.rs only needs the Dart gate bumped to match; old frames
/// fail closed (skipped) while JSON meters keep rendering.
///
/// Non-meter events drained by this call are NOT dropped: they are set
/// aside as JSON text in the client's pending queue and come back first
/// from the next [`poll`], the hot meter path never loses control
/// frames. Unknown clients get `[]`.
#[flutter_rust_bridge::frb]
pub async fn poll_binary(client_id: String, cap: u32) -> Vec<u8> {
    let cap = if cap == 0 {
        DRAIN_CAP_DEFAULT
    } else {
        (cap as usize).min(DRAIN_CAP_MAX)
    };
    let Some(eng) = engine() else {
        return Vec::new();
    };
    let t_ms = eng.now_ms();
    let mut map = clients().lock().unwrap();
    let Some(client) = map.get_mut(&client_id) else {
        return Vec::new();
    };
    let mut frames: Vec<Vec<u8>> = Vec::new();
    let mut drained = 0usize;
    while drained < cap {
        match client.handle.events.try_recv() {
            Ok(frame) => {
                drained += 1;
                match frame.event.to_binary(client.bin_seq, t_ms) {
                    Some(bytes) => {
                        client.bin_seq = client.bin_seq.wrapping_add(1);
                        frames.push(bytes);
                    }
                    None => client.pending_json.push_back(frame.json.to_string()),
                }
            }
            Err(_) => break,
        }
    }
    encode_binary_batch(&frames)
}

