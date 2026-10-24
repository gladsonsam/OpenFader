//! Length-prefixed TCP framing helpers (for closed TCP protocols like DL32S).
//! UDP/OSC drivers don't use this; it lives here so a future TCP driver reuses
//! it instead of duplicating framing code.

pub fn encode_frame(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + payload.len());
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
    out
}

/// Incomplete tail stays in `leftover`.
pub fn split_frames(buf: &[u8]) -> (Vec<Vec<u8>>, Vec<u8>) {
    let mut frames = Vec::new();
    let mut i = 0;
    while i + 4 <= buf.len() {
        let len = u32::from_be_bytes([buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]) as usize;
        if i + 4 + len > buf.len() {
            break;
        }
        frames.push(buf[i + 4..i + 4 + len].to_vec());
        i += 4 + len;
    }
    (frames, buf[i..].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_roundtrip_with_split_tail() {
        let mut stream = encode_frame(b"hello");
        stream.extend_from_slice(&encode_frame(b" MeterFrame"));
        let partial = &stream[..stream.len() - 1];
        let (frames, leftover) = split_frames(partial);
        assert_eq!(frames, vec![b"hello".to_vec()]);
        assert!(!leftover.is_empty());
        let mut rejoined = leftover;
        rejoined.push(stream[stream.len() - 1]);
        let (frames2, leftover2) = split_frames(&rejoined);
        assert_eq!(frames2, vec![b" MeterFrame".to_vec()]);
        assert!(leftover2.is_empty());
    }
}
