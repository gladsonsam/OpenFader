//! Channel-name and show-snapshot payload codecs.

use super::word_swap;

// ---------------------------------------------------------------------------
// Names + snapshots
// ---------------------------------------------------------------------------

/// Parse a CHANNEL_NAMES payload into `(slot, name)` pairs.
///
/// The payload mirrors CHANNEL_VALUES framing (start slot + meta + a
/// word-swapped stream of NUL-terminated names). An unnamed slot is a bare
/// NUL and still consumes an index, so blanks are kept as empty strings to
/// keep later slots numbered correctly.
///
/// The meta count bounds how many slots the push covers when nonzero: a
/// single-slot rename then touches exactly its slot instead of also blanking
/// the neighbour with padding NULs. A zero count (bulk table dump) takes
/// every chunk, padding included, those land past the real 75-slot table
/// and never surface through name lookups.
pub fn parse_name_table(body: &[u8]) -> Option<Vec<(u32, String)>> {
    if body.len() < 8 || !body.len().is_multiple_of(4) {
        return None;
    }
    let start = u32::from_be_bytes(body[0..4].try_into().ok()?);
    let meta = u32::from_be_bytes(body[4..8].try_into().ok()?);
    let count = ((meta >> 16) & 0xFFFF) as usize;
    let swapped = word_swap(&body[8..]);
    let chunks: Vec<&[u8]> = swapped.split(|b| *b == 0).collect();
    let n = if count == 0 {
        chunks.len()
    } else {
        count.min(chunks.len())
    };
    let mut out = Vec::with_capacity(n);
    for (i, chunk) in chunks.iter().take(n).enumerate() {
        let name = String::from_utf8_lossy(chunk).trim().to_string();
        out.push((start + i as u32, name));
    }
    Some(out)
}

/// A snapshot event relayed by the mixer (`SHOW_SNAPSHOT` op 1/5 pushes).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SnapshotEvent {
    Recall(u32),
    Rename(u32, String),
}

/// Body `<op> <number> ...`: op 1 is a recall relay, op 5 carries
/// `<unix epoch> <word-swapped name>`.
pub fn parse_snapshot_push(body: &[u8]) -> Option<SnapshotEvent> {
    if body.len() < 8 {
        return None;
    }
    let op = u32::from_be_bytes(body[0..4].try_into().ok()?);
    let number = u32::from_be_bytes(body[4..8].try_into().ok()?);
    if number == 0 {
        return None;
    }
    match op {
        1 => Some(SnapshotEvent::Recall(number)),
        5 => {
            if body.len() < 12 {
                return None;
            }
            let raw = word_swap(&body[12..]);
            let name = String::from_utf8_lossy(raw.split(|b| *b == 0).next().unwrap_or(&[]))
                .trim()
                .to_string();
            if name.is_empty() {
                return None;
            }
            Some(SnapshotEvent::Rename(number, name))
        }
        _ => None,
    }
}

/// Recall body pair, the way the mixer app does it: REQUEST `[1, snapshot]`
/// followed by RESPONSE `[1]`.
pub fn build_snapshot_recall_request(snapshot: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(8);
    out.extend_from_slice(&1u32.to_be_bytes());
    out.extend_from_slice(&snapshot.to_be_bytes());
    out
}

pub fn build_snapshot_save(snapshot: u32, epoch_secs: u64, name: &str) -> Vec<u8> {
    let text = name.as_bytes();
    let clipped = &text[..text.len().min(63)];
    let mut padded = clipped.to_vec();
    padded.push(0);
    while !padded.len().is_multiple_of(4) {
        padded.push(0);
    }
    let mut out = Vec::with_capacity(12 + padded.len());
    out.extend_from_slice(&5u32.to_be_bytes());
    out.extend_from_slice(&snapshot.to_be_bytes());
    out.extend_from_slice(&(epoch_secs as u32).to_be_bytes());
    out.extend_from_slice(&word_swap(&padded));
    out
}
