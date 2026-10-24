//! Generic OSC encode/decode over `rosc`. All drivers use this; none hand-roll OSC.
use rosc::{OscMessage, OscPacket, OscType, decoder, encoder};

/// Encode `addr` with `args` to wire bytes.
pub fn encode_message(addr: &str, args: Vec<OscType>) -> anyhow::Result<Vec<u8>> {
    let packet = OscPacket::Message(OscMessage {
        addr: addr.to_string(),
        args,
    });
    encoder::encode(&packet).map_err(|e| anyhow::anyhow!("osc encode {addr}: {e:?}"))
}

pub fn query_message(addr: &str) -> anyhow::Result<Vec<u8>> {
    encode_message(addr, vec![])
}

/// Decode one UDP datagram into a packet.
///
/// Slashless fallback (rig-verified 2026-10-19, XR18 FW 1.20, see
/// X-Air Edit protocol map §4): the desk addresses
/// `/batchsubscribe` meter blobs as `meters/N` with NO leading slash,
/// which the strict decoder rejects. When strict decode fails AND the
/// datagram's leading address string is a slashless `meters` addr, retry
/// with a `/` prepended (re-padded); the retried bytes must themselves
/// strictly decode or the datagram still errors. All other malformed
/// input returns `Err` unchanged.
pub fn decode_packet(buf: &[u8]) -> anyhow::Result<OscPacket> {
    match decoder::decode_udp(buf) {
        Ok((_, p)) => Ok(p),
        Err(e) => decode_slashless_meters(buf)
            .ok_or_else(|| anyhow::anyhow!("osc decode: {e:?}")),
    }
}

/// Repair a slashless `meters/N` datagram (`None` for anything else).
fn decode_slashless_meters(buf: &[u8]) -> Option<OscPacket> {
    let end = buf.iter().position(|&b| b == 0)?;
    let addr = std::str::from_utf8(&buf[..end]).ok()?;
    if !(addr == "meters" || addr.starts_with("meters/")) {
        return None;
    }
    // Splice a leading slash, preserving 4-byte OSC-string padding: the
    // original address field occupies `(end + 4) & !3` bytes.
    let mut fixed = Vec::with_capacity(buf.len() + 4);
    fixed.push(b'/');
    fixed.extend_from_slice(&buf[..end]);
    fixed.push(0);
    while fixed.len() % 4 != 0 {
        fixed.push(0);
    }
    let rest = (end + 4) & !3;
    fixed.extend_from_slice(buf.get(rest..)?);
    decoder::decode_udp(&fixed).ok().map(|(_, p)| p)
}

pub fn flatten(packet: OscPacket) -> Vec<OscMessage> {
    match packet {
        OscPacket::Message(m) => vec![m],
        OscPacket::Bundle(b) => b.content.into_iter().flat_map(flatten).collect(),
    }
}

pub fn clamp01(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

pub fn fader_arg(value: f32) -> OscType {
    OscType::Float(clamp01(value))
}

/// Build a mute message value for X-Air `/mix/on` semantics: 1 = on (unmuted).
pub fn on_arg(muted: bool) -> OscType {
    OscType::Int(if muted { 0 } else { 1 })
}

pub fn fader_value(msg: &OscMessage) -> anyhow::Result<f32> {
    match msg.args.first() {
        Some(OscType::Float(f)) => Ok(*f),
        Some(OscType::Double(d)) => Ok(*d as f32),
        Some(OscType::Int(i)) => Ok(*i as f32),
        other => anyhow::bail!("{}: expected float arg, got {:?}", msg.addr, other),
    }
}

pub fn int_arg(msg: &OscMessage) -> anyhow::Result<i32> {
    match msg.args.first() {
        Some(OscType::Int(i)) => Ok(*i),
        Some(OscType::Float(f)) => Ok(*f as i32),
        other => anyhow::bail!("{}: expected int arg, got {:?}", msg.addr, other),
    }
}

pub fn is_on(msg: &OscMessage) -> anyhow::Result<bool> {
    Ok(int_arg(msg)? != 0)
}

pub fn mute_is_muted(msg: &OscMessage) -> anyhow::Result<bool> {
    Ok(!is_on(msg)?)
}

/// All string args of a message (used for `/info` discovery replies).
pub fn string_args(msg: &OscMessage) -> Vec<String> {
    msg.args
        .iter()
        .filter_map(|a| match a {
            OscType::String(s) => Some(s.clone()),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fader_roundtrip() {
        let bytes = encode_message("/ch/01/mix/fader", vec![OscType::Float(0.75)]).unwrap();
        let msgs = flatten(decode_packet(&bytes).unwrap());
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].addr, "/ch/01/mix/fader");
        assert!((fader_value(&msgs[0]).unwrap() - 0.75).abs() < 1e-6);
    }

    #[test]
    fn keepalive_shape() {
        // /xremote is an empty message: address + ",\0" tag section, no args.
        let bytes = query_message("/xremote").unwrap();
        let msgs = flatten(decode_packet(&bytes).unwrap());
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].addr, "/xremote");
        assert!(msgs[0].args.is_empty());
        assert!(bytes.len() >= 12);
    }

    #[test]
    fn on_arg_semantics() {
        assert!(matches!(on_arg(true), OscType::Int(0)));
        assert!(matches!(on_arg(false), OscType::Int(1)));
        let msg = OscMessage {
            addr: "/ch/01/mix/on".into(),
            args: vec![OscType::Int(0)],
        };
        assert!(mute_is_muted(&msg).unwrap());
    }

    /// Gig wifi feeds garbage: every truncation of a valid datagram must
    /// decode-or-error, never panic. (The full datagram still decodes.)
    #[test]
    fn truncated_datagrams_never_panic() {
        let valid =
            encode_message("/ch/01/mix/fader", vec![OscType::Float(0.75)]).unwrap();
        for len in 0..valid.len() {
            let _ = decode_packet(&valid[..len]);
        }
        let bundle = {
            let inner = encode_message("/a", vec![]).unwrap();
            let mut b = b"#bundle\0".to_vec();
            b.extend_from_slice(&[0u8; 8]);
            b.extend_from_slice(&(inner.len() as u32).to_be_bytes());
            b.extend_from_slice(&inner);
            b
        };
        for len in 0..bundle.len() {
            let _ = decode_packet(&bundle[..len]);
        }
        assert!(decode_packet(&valid).is_ok());
        assert_eq!(flatten(decode_packet(&bundle).unwrap()).len(), 1);
    }

    /// Targeted malformed inputs: empty, short, zero-filled, unknown type
    /// tags, tag/arg count mismatches, and non-OSC garbage all return
    /// `Err` (never panic, never `Ok` on junk).
    #[test]
    fn malformed_datagrams_are_errors() {
        assert!(decode_packet(&[]).is_err());
        for n in 1..16usize {
            assert!(
                decode_packet(&vec![0u8; n]).is_err(),
                "{n} zero bytes decoded"
            );
        }
        let mut bad_tag = encode_message("/t", vec![OscType::Int(1)]).unwrap();
        let pos = bad_tag.iter().position(|&b| b == b'i').unwrap();
        bad_tag[pos] = b'X';
        assert!(decode_packet(&bad_tag).is_err());
        assert!(decode_packet(b"/t\0\0,i\0\0").is_err());
        assert!(decode_packet(b"/ab\0").is_err());
        // NOTE: rosc parses bundles with `many0`, so corrupt bundle
        // *content* (lying element sizes, trailing junk) decodes Ok with
        // zero elements rather than Err, lenient, but safe: it flattens
        // to nothing and the recv loop ignores it. Assert exactly that
        // (and no panic) instead of Err here.
        for tail in [u32::MAX.to_be_bytes().to_vec(), b"junk".to_vec()] {
            let mut bundle = b"#bundle\0".to_vec();
            bundle.extend_from_slice(&[0u8; 8]);
            bundle.extend_from_slice(&tail);
            let packet = decode_packet(&bundle).unwrap();
            assert!(flatten(packet).is_empty());
        }
        for junk in [
            b"hello world, not osc......".to_vec(),
            vec![0xFFu8; 64],
            b",iii\x00\x00".to_vec(),
            b"/".to_vec(),
        ] {
            assert!(decode_packet(&junk).is_err(), "junk decoded: {junk:?}");
        }
    }

    /// Arg extractors reject wrong shapes with `Err`, never panic
    /// (venue packets with swapped typetags must not kill the recv loop).
    #[test]
    fn arg_extractors_reject_wrong_shapes() {
        let empty = OscMessage {
            addr: "/ch/01/mix/fader".into(),
            args: vec![],
        };
        assert!(fader_value(&empty).is_err());
        assert!(int_arg(&empty).is_err());
        let stringy = OscMessage {
            addr: "/ch/01/mix/fader".into(),
            args: vec![OscType::String("nope".into())],
        };
        assert!(fader_value(&stringy).is_err());
        assert!(int_arg(&stringy).is_err());
        let blobbed = OscMessage {
            addr: "/meters/1".into(),
            args: vec![OscType::Blob(vec![1, 2, 3])],
        };
        assert!(fader_value(&blobbed).is_err());
        // Compatible widenings still work (desk sends ints/doubles too).
        let inty = OscMessage {
            addr: "/ch/01/mix/fader".into(),
            args: vec![OscType::Int(1)],
        };
        assert!((fader_value(&inty).unwrap() - 1.0).abs() < f32::EPSILON);
        let floaty = OscMessage {
            addr: "/ch/01/mix/on".into(),
            args: vec![OscType::Float(1.0)],
        };
        assert!(is_on(&floaty).unwrap());
    }

    /// Slashless `meters/N` datagrams (the desk's `/batchsubscribe` reply
    /// form, rig-verified 2026-10-19) decode with the slash restored so
    /// they route into the meter path; slashless non-meter traffic still
    /// errors (no misrouting).
    #[test]
    fn slashless_meter_blob_decodes_with_slash_restored() {
        let blob = crate::meters::build_meter_blob(&[0.2, 0.8]);
        let bytes = encode_message("meters/1", vec![OscType::Blob(blob)]).unwrap();
        // Strict rosc decode rejects it (address must start with '/')...
        assert!(decoder::decode_udp(&bytes).is_err());
        // ...but the shared decoder repairs slashless `meters` addrs.
        let msgs = flatten(decode_packet(&bytes).unwrap());
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].addr, "/meters/1");
        let other = encode_message("ch/01/mix/fader", vec![OscType::Float(0.5)]).unwrap();
        assert!(decode_packet(&other).is_err());
        for len in 0..bytes.len() {
            let _ = decode_packet(&bytes[..len]);
        }
    }
}
