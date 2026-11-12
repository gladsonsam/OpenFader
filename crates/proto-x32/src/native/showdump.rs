//! Native showdump parsing and bounded collection.

use super::archive::decode_node_text;
use super::*;

/// Split off one leading `"quoted"` token; returns (token, rest).
fn take_quoted(s: &str) -> Option<(String, String)> {
    let s = s.trim_start();
    let inner = s.strip_prefix('"')?;
    let end = inner.find('"')?;
    Some((
        inner[..end].to_string(),
        inner[end + 1..].trim_start().to_string(),
    ))
}

fn take_int(s: &str) -> Option<(i64, String)> {
    let s = s.trim_start();
    let mut len = 0;
    for (i, ch) in s.char_indices() {
        if ch == '-' && i == 0 {
            len += 1;
        } else if ch.is_ascii_digit() {
            len += ch.len_utf8();
        } else {
            break;
        }
    }
    if len == 0 || (len == 1 && s.starts_with('-')) {
        return None;
    }
    s[..len]
        .parse::<i64>()
        .ok()
        .map(|v| (v, s[len..].trim_start().to_string()))
}

/// `%110000000` bitmask or plain int.
fn take_mask(s: &str) -> Option<(u32, String)> {
    let s = s.trim_start();
    if let Some(bits) = s.strip_prefix('%') {
        let mut value: u32 = 0;
        let mut rest = bits;
        while let Some(ch) = rest.chars().next() {
            if ch == '0' || ch == '1' {
                value = value
                    .checked_shl(1)
                    .and_then(|v| v.checked_add(u32::from(ch == '1')))?;
                rest = &rest[ch.len_utf8()..];
            } else {
                break;
            }
        }
        Some((value, rest.trim_start().to_string()))
    } else {
        take_int(s).map(|(v, r)| (v as u32, r))
    }
}

fn parse_scene_dump(slot: u8, rest: &str) -> Option<NativeSceneSlot> {
    let (name, rest) = take_quoted(rest)?;
    let (notes, rest) = take_quoted(&rest)?;
    let (safes, rest) = take_mask(&rest)?;
    let (hasdata, _) = take_int(&rest)?;
    if safes > 0xFF || !(0..=1).contains(&hasdata) {
        return None;
    }
    Some(NativeSceneSlot {
        slot,
        name,
        notes,
        safes,
        hasdata: hasdata != 0,
    })
}

fn parse_snippet_dump(slot: u8, rest: &str) -> Option<NativeSnippetSlot> {
    let (name, rest) = take_quoted(rest)?;
    let (eventtyp, rest) = take_mask(&rest)?;
    let (channels, rest) = take_int(&rest)?;
    let (auxbuses, rest) = take_int(&rest)?;
    let (maingrps, rest) = take_int(&rest)?;
    let (hasdata, _) = take_int(&rest)?;
    if eventtyp > 0x1FF_FFFF || maingrps > 0xFFFF || !(0..=1).contains(&hasdata) {
        return None;
    }
    Some(NativeSnippetSlot {
        slot,
        name,
        eventtyp,
        channels: channels as u32,
        auxbuses: auxbuses as u32,
        maingrps: maingrps as u32,
        hasdata: hasdata != 0,
    })
}

fn parse_cue_dump(position: u16, rest: &str) -> Option<NativeCueSlot> {
    let (numb, rest) = take_int(rest)?;
    let (name, rest) = take_quoted(&rest)?;
    let (skip, rest) = take_int(&rest)?;
    let (scene, rest) = take_int(&rest)?;
    let (snippet, rest) = take_int(&rest)?;
    let (miditype, rest) = take_int(&rest)?;
    let (midichan, rest) = take_int(&rest)?;
    let (midipara1, rest) = take_int(&rest)?;
    let (midipara2, _) = take_int(&rest)?;
    if !(0..=1).contains(&skip) || !(-1..100).contains(&scene) || !(-1..100).contains(&snippet) {
        return None;
    }
    if !(0..=3).contains(&miditype) {
        return None;
    }
    Some(NativeCueSlot {
        position,
        numb: numb as i32,
        name,
        skip: skip != 0,
        scene: scene as i32,
        snippet: snippet as i32,
        miditype: miditype as i32,
        midichan: midichan as i32,
        midipara1: midipara1 as i32,
        midipara2: midipara2 as i32,
    })
}

#[derive(Default)]
pub(super) struct ShowDump {
    pub(super) scenes: Vec<NativeSceneSlot>,
    pub(super) snippets: Vec<NativeSnippetSlot>,
    pub(super) cues: Vec<NativeCueSlot>,
}

pub(super) fn parse_showdump_line(dump: &mut ShowDump, line: &str) {
    let line = line.trim().trim_end_matches(['\n', '\r']).trim();
    let Some((path, rest)) = line.split_once(' ') else {
        return;
    };
    let rest = rest.trim();
    if let Some(slot) = path.strip_prefix("/-show/showfile/scene/") {
        if slot.len() == 3 {
            if let Ok(n) = slot.parse::<u8>() {
                if let Some(entry) = parse_scene_dump(n, rest) {
                    dump.scenes.push(entry);
                }
            }
        }
        return;
    }
    if let Some(slot) = path.strip_prefix("/-show/showfile/snippet/") {
        if slot.len() == 3 {
            if let Ok(n) = slot.parse::<u8>() {
                if let Some(entry) = parse_snippet_dump(n, rest) {
                    dump.snippets.push(entry);
                }
            }
        }
        return;
    }
    if let Some(slot) = path.strip_prefix("/-show/showfile/cue/") {
        if slot.len() == 3 {
            if let Ok(n) = slot.parse::<u16>() {
                if let Some(entry) = parse_cue_dump(n, rest) {
                    dump.cues.push(entry);
                }
            }
        }
    }
}

/// Collect `/showdump` node lines within a bounded window. Same
/// slashless-`node` decode reason as `read_node_text`: a raw socket.
pub(super) async fn collect_showdump(session: &Arc<OscSession>) -> Vec<String> {
    let Ok(peer) = session.socket().peer_addr() else {
        return Vec::new();
    };
    let Ok(socket) = tokio::net::UdpSocket::bind("0.0.0.0:0").await else {
        return Vec::new();
    };
    let Ok(out) = openfader_transport::osc::encode_message("/showdump", vec![]) else {
        return Vec::new();
    };
    if socket.send_to(&out, peer).await.is_err() {
        return Vec::new();
    }
    let mut lines = Vec::new();
    let deadline = tokio::time::Instant::now() + SHOWDUMP_WINDOW;
    let mut buf = vec![0u8; 4096];
    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            break;
        }
        match tokio::time::timeout(deadline - now, socket.recv_from(&mut buf)).await {
            Ok(Ok((n, _))) => {
                if let Some(text) = decode_node_text(&buf[..n]) {
                    lines.push(text);
                }
            }
            _ => break,
        }
    }
    lines
}
