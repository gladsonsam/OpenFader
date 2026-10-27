//! CHANNEL_VALUES body codecs. Sparse writes and pushes share the same
//! address/value layout; normalize them without inventing intervening words.

const SPARSE_FLAG: u16 = 0x2000;
const SPARSE_COUNT_MASK: u16 = SPARSE_FLAG - 1;
const METER_STREAM_FLAG: u16 = 0x1000;

/// A contiguous run of CHANNEL_VALUES words in one address space.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueBlock {
    pub start: u32,
    pub value_type: u8,
    pub raws: Vec<u32>,
}

/// Decode an entire value payload before exposing any values. Sparse bodies
/// use count flag `0x2000`: `[address0, meta, value0, address1, value1, ...]`.
/// Runs preserve wire order (including repeated or descending addresses).
/// Ordinary count zero consumes all remaining words; an explicit ordinary
/// count ignores surplus words, but never accepts a truncated range.
/// Captured meter streams (type 1, count field exactly `0x1000`) also consume
/// all remaining words: this field is a stream flag, not a 4096-word count.
pub fn parse_value_blocks(body: &[u8]) -> Option<Vec<ValueBlock>> {
    if body.len() < 8 || !body.len().is_multiple_of(4) {
        return None;
    }
    let word = |offset| u32::from_be_bytes(body[offset..offset + 4].try_into().unwrap());
    let start = word(0);
    let meta = word(4);
    let count_field = (meta >> 16) as u16;
    let value_type = (meta >> 8) as u8;
    if count_field & SPARSE_FLAG != 0 {
        // Bits above the flag are not part of the supported sparse count.
        if count_field & !(SPARSE_FLAG | SPARSE_COUNT_MASK) != 0 {
            return None;
        }
        let count = usize::from(count_field & SPARSE_COUNT_MASK);
        if count == 0 || body.len() != count.checked_mul(8)?.checked_add(4)? {
            return None;
        }
        let mut blocks: Vec<ValueBlock> = Vec::new();
        for i in 0..count {
            let address = if i == 0 { start } else { word(4 + i * 8) };
            let raw = word(8 + i * 8);
            if let Some(last) = blocks.last_mut() {
                if last.start.checked_add(u32::try_from(last.raws.len()).ok()?) == Some(address) {
                    last.raws.push(raw);
                    continue;
                }
            }
            blocks.push(ValueBlock {
                start: address,
                value_type,
                raws: vec![raw],
            });
        }
        Some(blocks)
    } else {
        let available = body.len() / 4 - 2;
        // Only the observed meter stream marker has implied length. Do not
        // mask this bit out of explicit counts or other address spaces.
        let count = if value_type == super::VALUE_TYPE_METERS && count_field == METER_STREAM_FLAG {
            0
        } else {
            usize::from(count_field)
        };
        let count = if count == 0 { available } else { count };
        if count > available {
            return None;
        }
        if count > 0 {
            start.checked_add(u32::try_from(count - 1).ok()?)?;
        }
        let raws = (0..count).map(|i| word(8 + i * 4)).collect();
        Some(vec![ValueBlock {
            start,
            value_type,
            raws,
        }])
    }
}

/// Compatibility view for callers that require one contiguous range.
/// Noncontiguous sparse payloads are withheld rather than flattened.
pub fn parse_value_block(body: &[u8]) -> Option<ValueBlock> {
    let mut blocks = parse_value_blocks(body)?;
    if blocks.len() != 1 {
        return None;
    }
    blocks.pop()
}

/// Raw wire value for a single-address write: integers/bools travel as u32,
/// faders and other continuous parameters as big-endian IEEE-754 floats (dB).
#[derive(Clone, Copy, Debug)]
pub enum RawValue {
    Int(u32),
    Float(f32),
}

impl RawValue {
    fn to_be_bytes(self) -> [u8; 4] {
        match self {
            Self::Int(v) => v.to_be_bytes(),
            Self::Float(v) => v.to_be_bytes(),
        }
    }
}

/// Decoded parameter word.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DecodedWord {
    F(f32),
    I(u32),
}

pub fn decode_word(raw: u32, is_float: bool) -> DecodedWord {
    if is_float {
        DecodedWord::F(f32::from_bits(raw))
    } else {
        DecodedWord::I(raw)
    }
}

/// Build a single-address CHANNEL_VALUES write body:
/// `[address, count(1)|type|0, value]`.
pub fn build_single_set(address: u32, value: RawValue, value_type: u8) -> Vec<u8> {
    let meta = (1u32 << 16) | ((value_type as u32) << 8);
    let mut out = Vec::with_capacity(12);
    out.extend_from_slice(&address.to_be_bytes());
    out.extend_from_slice(&meta.to_be_bytes());
    out.extend_from_slice(&value.to_be_bytes());
    out
}

/// Build a multi-address CHANNEL_VALUES write the way the mixer app does:
/// `(address, value)` pairs with the meta word carrying a `0x2000` flag in
/// the count field, placed after the first address.
pub fn build_multi_set(pairs: &[(u32, u32)], value_type: u8) -> anyhow::Result<Vec<u8>> {
    if pairs.is_empty() {
        anyhow::bail!("multi-set needs at least one (address, value) pair");
    }
    if pairs.len() > usize::from(SPARSE_COUNT_MASK) {
        anyhow::bail!("multi-set count exceeds the sparse count field");
    }
    let count = pairs.len() as u32;
    let meta = ((count | u32::from(SPARSE_FLAG)) << 16) | ((value_type as u32) << 8);
    let mut out = Vec::with_capacity(pairs.len() * 8 + 4);
    let mut first = true;
    for (addr, raw) in pairs {
        out.extend_from_slice(&addr.to_be_bytes());
        if first {
            out.extend_from_slice(&meta.to_be_bytes());
            first = false;
        }
        out.extend_from_slice(&raw.to_be_bytes());
    }
    Ok(out)
}

/// The mixer answers read requests with an empty body: polling does not
/// work, only listening to pushes does, so this is used for probing only.
pub fn build_read_request(start: u32, count: u32, value_type: u8) -> Vec<u8> {
    let meta = ((count & 0xFFFF) << 16) | ((value_type as u32) << 8);
    let mut out = Vec::with_capacity(8);
    out.extend_from_slice(&start.to_be_bytes());
    out.extend_from_slice(&meta.to_be_bytes());
    out
}
