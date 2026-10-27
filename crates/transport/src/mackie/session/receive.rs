//! Receive loop, correlated completion, and atomic desk-push ingestion.

use super::*;
use tokio::io::AsyncReadExt;
use tokio::net::tcp::OwnedReadHalf;

/// Read frames until the peer closes, dispatching each to [`dispatch`].
/// Garbage bytes (failed decodes) are skipped one at a time; short reads
/// wait for more data.
pub(super) async fn rx_loop(
    mut read: OwnedReadHalf,
    write: Arc<Mutex<OwnedWriteHalf>>,
    pending: Arc<StdMutex<HashMap<u8, oneshot::Sender<PendingResult>>>>,
    caches: Arc<Caches>,
) {
    let mut buf = Vec::with_capacity(65536);
    let mut tmp = [0u8; 4096];
    loop {
        loop {
            match decode_one(&buf) {
                Ok(Some((msg, used))) => {
                    buf.drain(..used);
                    dispatch(msg, &write, &pending, &caches).await;
                }
                Ok(None) => break, // truncated: read more
                Err(_) => {
                    if buf.is_empty() {
                        break;
                    }
                    buf.drain(..1); // garbage: resync one byte at a time
                }
            }
        }
        match read.read(&mut tmp).await {
            Ok(0) => break, // peer closed
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
            Err(_) => break,
        }
    }
    let senders: Vec<oneshot::Sender<PendingResult>> = pending
        .lock()
        .map(|mut p| p.drain().map(|(_, tx)| tx).collect())
        .unwrap_or_default();
    for tx in senders {
        let _ = tx.send(Err(RequestFailed("connection lost".into())));
    }
}

pub(super) async fn send_frame(
    write: &Arc<Mutex<OwnedWriteHalf>>,
    seq: u8,
    msg_type: MessageType,
    command: Command,
    body: Vec<u8>,
) -> anyhow::Result<()> {
    let msg = Message::new(seq, msg_type, command, body).map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut guard = write.lock().await;
    guard.write_all(&msg.encode()).await?;
    guard.flush().await?;
    Ok(())
}

fn ingest_value_blocks(store: &StdMutex<ValueStore>, blocks: &[ValueBlock], value_type: u8) {
    // Validate the entire payload first, then apply all of its runs under one
    // lock: values and pending provenance change together in wire order.
    if let Ok(mut s) = store.lock() {
        for block in blocks.iter().filter(|block| block.value_type == value_type) {
            for (i, raw) in block.raws.iter().enumerate() {
                let addr = block.start + i as u32;
                s.sequence = s.sequence.wrapping_add(1);
                let sequence = s.sequence;
                let observed = s.observed.entry(addr).or_insert((*raw, 0, 0));
                if observed.0 != *raw {
                    observed.1 = observed.1.wrapping_add(1);
                    observed.0 = *raw;
                }
                observed.2 = sequence;
                s.values.insert(addr, *raw);
                s.pending.remove(&addr);
            }
        }
    }
}

fn dispatch_values(caches: &Caches, body: &[u8]) {
    let Some(blocks) = parse_value_blocks(body) else {
        return;
    };
    ingest_value_blocks(&caches.values, &blocks, VALUE_TYPE_PARAM);
    ingest_value_blocks(&caches.type2_values, &blocks, VALUE_TYPE_MUTE_GROUP);
    for block in blocks {
        let _ = caches.push_tx.send(PushEvent::Values {
            value_type: block.value_type,
            start: block.start,
            raws: block.raws,
        });
    }
}

fn ingest_names(store: &StdMutex<NameStore>, pairs: &[(u32, String)]) {
    // One lock: desk insert + pending clear are atomic (same contract).
    if let Ok(mut s) = store.lock() {
        for (slot, name) in pairs {
            s.names.insert(*slot, name.clone());
            s.pending.remove(slot);
        }
    }
}

fn complete_pending(
    pending: &Arc<StdMutex<HashMap<u8, oneshot::Sender<PendingResult>>>>,
    seq: u8,
    result: PendingResult,
) {
    let tx = pending.lock().map(|mut p| p.remove(&seq)).unwrap_or(None);
    if let Some(tx) = tx {
        let _ = tx.send(result);
    }
}

async fn dispatch(
    msg: Message,
    write: &Arc<Mutex<OwnedWriteHalf>>,
    pending: &Arc<StdMutex<HashMap<u8, oneshot::Sender<PendingResult>>>>,
    caches: &Caches,
) {
    if msg.command == Command::ChannelValues {
        dispatch_values(caches, &msg.body);
    }
    let names = &caches.names;
    let snapshot_names = &caches.snapshot_names;
    let current_snapshot = &caches.current_snapshot;

    match msg.msg_type {
        MessageType::Request => {
            let reply_body: Vec<u8> = match msg.command {
                Command::ClientHandshake => HANDSHAKE_REPLY.to_vec(),
                Command::GeneralInfo => GENERAL_INFO_REPLY.to_vec(),
                Command::ChannelInfoControl => msg.body.get(..4).unwrap_or(&[]).to_vec(),
                Command::ShowSnapshot => {
                    match parse_snapshot_push(&msg.body) {
                        Some(SnapshotEvent::Recall(n)) => {
                            if let Ok(mut c) = current_snapshot.lock() {
                                *c = Some(n);
                            }
                            // Push-only verified record (see the
                            // `snapshot_names_pushed` store docs): desk
                            // evidence, never own-write optimism.
                            if let Ok(mut c) = caches.current_snapshot_pushed.lock() {
                                *c = Some(n);
                            }
                            let _ = caches.push_tx.send(PushEvent::SnapshotRecall(n));
                        }
                        Some(SnapshotEvent::Rename(n, name)) => {
                            if let Ok(mut c) = snapshot_names.lock() {
                                c.insert(n, name.clone());
                            }
                            // Push-only verified record (see above).
                            if let Ok(mut c) = caches.snapshot_names_pushed.lock() {
                                c.insert(n, name.clone());
                            }
                            let _ = caches
                                .push_tx
                                .send(PushEvent::SnapshotRename { slot: n, name });
                        }
                        None => {}
                    }
                    msg.body.get(..4).unwrap_or(&[]).to_vec()
                }
                Command::ChannelValues => Vec::new(),
                Command::ChannelNames => {
                    if let Some(pairs) = parse_name_table(&msg.body) {
                        ingest_names(names, &pairs);
                        let _ = caches.push_tx.send(PushEvent::Names { pairs });
                    }
                    Vec::new()
                }
                _ => Vec::new(),
            };
            let _ = send_frame(
                write,
                msg.seq,
                MessageType::Response,
                msg.command,
                reply_body,
            )
            .await;
        }
        MessageType::Broadcast => {}
        MessageType::Response | MessageType::Error => {
            if msg.msg_type == MessageType::Error {
                complete_pending(
                    pending,
                    msg.seq,
                    Err(RequestFailed("mixer returned ERROR".into())),
                );
            } else {
                complete_pending(pending, msg.seq, Ok(msg));
            }
        }
    }
}
