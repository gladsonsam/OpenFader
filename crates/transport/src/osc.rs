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

/// Encode a no-arg message (queries, keepalive, subscribes).
pub fn query_message(addr: &str) -> anyhow::Result<Vec<u8>> {
    encode_message(addr, vec![])
}

/// Decode one UDP datagram into a packet.
pub fn decode_packet(buf: &[u8]) -> anyhow::Result<OscPacket> {
    decoder::decode_udp(buf)
        .map(|(_, p)| p)
        .map_err(|e| anyhow::anyhow!("osc decode: {e:?}"))
}

/// Flatten a packet into its messages (bundles expand to N messages).
pub fn flatten(packet: OscPacket) -> Vec<OscMessage> {
    match packet {
        OscPacket::Message(m) => vec![m],
        OscPacket::Bundle(b) => b.content.into_iter().flat_map(flatten).collect(),
    }
}

/// Clamp a fader to the 0.0..1.0 wire range.
pub fn clamp01(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

/// Build a fader-set message value.
pub fn fader_arg(value: f32) -> OscType {
    OscType::Float(clamp01(value))
}

/// Build a mute message value for X-Air `/mix/on` semantics: 1 = on (unmuted).
pub fn on_arg(muted: bool) -> OscType {
    OscType::Int(if muted { 0 } else { 1 })
}

/// Extract the first float arg (fader replies).
pub fn fader_value(msg: &OscMessage) -> anyhow::Result<f32> {
    match msg.args.first() {
        Some(OscType::Float(f)) => Ok(*f),
        Some(OscType::Double(d)) => Ok(*d as f32),
        Some(OscType::Int(i)) => Ok(*i as f32),
        other => anyhow::bail!("{}: expected float arg, got {:?}", msg.addr, other),
    }
}

/// Extract the first int arg.
pub fn int_arg(msg: &OscMessage) -> anyhow::Result<i32> {
    match msg.args.first() {
        Some(OscType::Int(i)) => Ok(*i),
        Some(OscType::Float(f)) => Ok(*f as i32),
        other => anyhow::bail!("{}: expected int arg, got {:?}", msg.addr, other),
    }
}

/// True when a `/mix/on`-style message means ON (unmuted).
pub fn is_on(msg: &OscMessage) -> anyhow::Result<bool> {
    Ok(int_arg(msg)? != 0)
}

/// True when muted.
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
}
