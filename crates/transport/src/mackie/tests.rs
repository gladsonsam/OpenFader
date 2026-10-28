use super::*;

fn keepalive_request(seq: u8) -> Vec<u8> {
    vec![HEADER_MARKER, seq, 0, 0, 0, 0x01, 0xFF, 0x52]
}

#[test]
fn keepalive_frame_vector() {
    // Hand-computed: header AB 01 00 00 00 01 sums to 0xAD,
    // checksum 0xFFFF-0xAD = 0xFF52, no body.
    let msg = Message::new(1, MessageType::Request, Command::KeepAlive, vec![]).unwrap();
    assert_eq!(msg.encode(), keepalive_request(1));
    let (back, used) = decode_one(&keepalive_request(1)).unwrap().unwrap();
    assert_eq!(used, FRAME_PREFIX_LEN);
    assert_eq!(back, msg);
}

#[test]
fn single_set_frame_vector() {
    // Mute input ch1 (address 8) to 1: body 00000008|00010500|00000001.
    // Body bytes sum to 15, checksum 0xFFFFFFF0. Header AB 01 00 03 00 13
    // sums to 0xC2, checksum 0xFF3D.
    let body = build_single_set(8, RawValue::Int(1), VALUE_TYPE_PARAM);
    assert_eq!(body, vec![0, 0, 0, 8, 0, 1, 5, 0, 0, 0, 0, 1]);
    let msg = Message::new(1, MessageType::Request, Command::ChannelValues, body).unwrap();
    let bytes = msg.encode();
    assert_eq!(
        &bytes[..FRAME_PREFIX_LEN],
        &[HEADER_MARKER, 1, 0, 3, 0, 0x13, 0xFF, 0x3D]
    );
    assert_eq!(&bytes[FRAME_PREFIX_LEN + 12..], &[0xFF, 0xFF, 0xFF, 0xF0]);
    let (back, used) = decode_one(&bytes).unwrap().unwrap();
    assert_eq!(used, bytes.len());
    assert_eq!(back, msg);
}

#[test]
fn header_checksum_rejects_corruption() {
    let mut bad = keepalive_request(1);
    bad[7] ^= 0xFF;
    match decode_one(&bad) {
        Err(FrameError::BadHeaderChecksum { .. }) => {}
        other => panic!("expected checksum error, got {other:?}"),
    }
}

#[test]
fn decode_rejects_garbage() {
    assert!(matches!(
        decode_one(&[0x00, 0x01, 0x02, 0x03, 0, 0, 0, 0]),
        Err(FrameError::BadMarker(0x00))
    ));
    assert!(matches!(
        decode_one(&[HEADER_MARKER, 1, 0, 0, 0x42, 0x01, 0, 0]),
        Err(FrameError::UnknownType(0x42))
    ));
    // Truncated body: header claims 1 chunk, only half present.
    let mut short = keepalive_request(1);
    short[2] = 0;
    short[3] = 1; // chunk count 1, no body bytes follow
    let recomputed = {
        let h: [u8; 6] = short[..6].try_into().unwrap();
        Message::header_checksum(&h).to_be_bytes()
    };
    short[6] = recomputed[0];
    short[7] = recomputed[1];
    assert_eq!(decode_one(&short).unwrap(), None);
}

#[test]
fn word_swap_vector() {
    // SPEC: "MGZU" travels as 55 5A 47 4D.
    assert_eq!(word_swap(&[0x55, 0x5A, 0x47, 0x4D]), b"MGZU");
    assert_eq!(word_swap(b"MGZU"), vec![0x55, 0x5A, 0x47, 0x4D]);
    assert!(word_swap(&[]).is_empty());
    // Trailing partial word is dropped.
    assert_eq!(word_swap(&[1, 2, 3, 4, 5]), vec![4, 3, 2, 1]);
}

#[test]
fn bulk_push_count_zero_means_all() {
    // Bulk push shape: meta 0x00000500 (type 5, count 0), 3 values.
    let mut body = vec![0, 0, 0, 8, 0, 0, 5, 0];
    for v in [1u32, 0xC1E00000, 0u32] {
        body.extend_from_slice(&v.to_be_bytes());
    }
    let block = parse_value_block(&body).unwrap();
    assert_eq!(block.start, 8);
    assert_eq!(block.value_type, VALUE_TYPE_PARAM);
    assert_eq!(block.raws, vec![1, 0xC1E00000, 0]);
}

#[test]
fn explicit_count_is_honoured() {
    let mut body = vec![0, 0, 0, 8, 0, 2, 5, 0];
    for v in [1u32, 2, 3] {
        body.extend_from_slice(&v.to_be_bytes());
    }
    let block = parse_value_block(&body).unwrap();
    assert_eq!(block.raws, vec![1, 2]);
}

#[test]
fn multi_set_flag_shape() {
    let body = build_multi_set(&[(8, 1), (9, 0xC1E00000)], VALUE_TYPE_PARAM).unwrap();
    // chunk0 = addr0, chunk1 = meta with 0x2000 flag, chunk2 = val0,
    // then (addrN, valN) pairs.
    let meta = u32::from_be_bytes(body[4..8].try_into().unwrap());
    assert_eq!(
        meta,
        ((2 | 0x2000) << 16) | ((VALUE_TYPE_PARAM as u32) << 8)
    );
    assert_eq!(&body[..4], &8u32.to_be_bytes());
    assert_eq!(&body[8..12], &1u32.to_be_bytes());
    assert_eq!(&body[12..16], &9u32.to_be_bytes());
    assert!(build_multi_set(&[], VALUE_TYPE_PARAM).is_err());
}

#[test]
fn mute_group_helpers() {
    assert_eq!(mute_group_master_address(1).unwrap(), 11);
    assert_eq!(mute_group_master_address(6).unwrap(), 16);
    assert!(mute_group_master_address(0).is_err());
    assert!(mute_group_master_address(7).is_err());
    assert_eq!(mute_group_name_slot(1).unwrap(), 64);
    assert_eq!(mute_group_name_slot(6).unwrap(), 69);
}

#[test]
fn meter_layout_shape() {
    let addrs = dl32_family_meter_addresses();
    assert_eq!(addrs.len(), 32 + 4 + 4 + 14 + 4 + 2);
    assert_eq!(addrs[0], 34);
    assert_eq!(addrs[31], 34 + 31 * 7);
    let body = build_meter_layout(&addrs);
    assert_eq!(body.len(), 4 + addrs.len() * 4);
    assert_eq!(&body[..4], &[0, 0, 0, 1]);
    assert_eq!(&body[4..8], &34i32.to_be_bytes());
    assert_eq!(BROADCAST_CONTROL_BODY.len(), 12);
}

#[test]
fn name_table_keeps_blank_slots() {
    // Slots 1-3: "Kick", "", "Snare", the blank still consumes index 2.
    let text = b"Kick\x00\x00Snare\x00\x00\x00";
    let mut padded = text.to_vec();
    while !padded.len().is_multiple_of(4) {
        padded.push(0);
    }
    let mut body = vec![];
    body.extend_from_slice(&1u32.to_be_bytes());
    body.extend_from_slice(&0x00030000u32.to_be_bytes()); // count = 3 slots
    body.extend_from_slice(&word_swap(&padded));
    let pairs = parse_name_table(&body).unwrap();
    assert_eq!(
        pairs,
        vec![
            (1, "Kick".to_string()),
            (2, String::new()),
            (3, "Snare".to_string()),
        ]
    );
}

#[test]
fn name_table_count_bounds_single_rename() {
    // A one-slot rename must not blank the neighbour with padding NULs.
    let mut padded = b"Vox\x00".to_vec();
    while !padded.len().is_multiple_of(4) {
        padded.push(0);
    }
    let mut body = vec![];
    body.extend_from_slice(&5u32.to_be_bytes());
    body.extend_from_slice(&0x00010000u32.to_be_bytes()); // count = 1 slot
    body.extend_from_slice(&word_swap(&padded));
    assert_eq!(
        parse_name_table(&body).unwrap(),
        vec![(5, "Vox".to_string())]
    );
}

#[test]
fn snapshot_push_events() {
    let mut recall = vec![];
    recall.extend_from_slice(&1u32.to_be_bytes());
    recall.extend_from_slice(&7u32.to_be_bytes());
    assert_eq!(parse_snapshot_push(&recall), Some(SnapshotEvent::Recall(7)));

    let mut save = vec![];
    save.extend_from_slice(&5u32.to_be_bytes());
    save.extend_from_slice(&2u32.to_be_bytes());
    save.extend_from_slice(&0u32.to_be_bytes()); // epoch placeholder
    let mut name = b"Main\x00".to_vec();
    while !name.len().is_multiple_of(4) {
        name.push(0);
    }
    save.extend_from_slice(&word_swap(&name));
    assert_eq!(
        parse_snapshot_push(&save),
        Some(SnapshotEvent::Rename(2, "Main".to_string()))
    );
    assert_eq!(parse_snapshot_push(&[0; 4]), None);
}

#[test]
fn snapshot_save_body_shape() {
    let body = build_snapshot_save(3, 1_700_000_000, "Main");
    assert_eq!(
        &body[..12],
        &[0, 0, 0, 5, 0, 0, 0, 3, 0x65, 0x53, 0xF1, 0x00][..]
    );
    // Name rides word-swapped after the 12-byte header.
    assert_eq!(word_swap(&body[12..])[..4], [b'M', b'a', b'i', b'n']);
}

// ---- loopback session test: fake mixer over TCP, no hardware ----

/// Minimal fake mixer: answers init requests with empty responses, then
/// after the second CHANNEL_INFO_CONTROL pushes a bulk value block, a
/// name table and a snapshot rename as REQUESTs (client must answer).
/// Records every CHANNEL_VALUES write it receives.
struct FakeMixer {
    writes: StdMutex<Vec<(u32, u32)>>,
}

async fn read_frame(stream: &mut TcpStream) -> anyhow::Result<Message> {
    let mut prefix = [0u8; FRAME_PREFIX_LEN];
    stream.read_exact(&mut prefix).await?;
    let chunks = u16::from_be_bytes([prefix[2], prefix[3]]) as usize;
    let mut rest = vec![0u8; chunks * 4 + if chunks == 0 { 0 } else { 4 }];
    if !rest.is_empty() {
        stream.read_exact(&mut rest).await?;
    }
    let mut full = prefix.to_vec();
    full.extend_from_slice(&rest);
    let (msg, _) = decode_one(&full)?.ok_or_else(|| anyhow::anyhow!("short frame"))?;
    Ok(msg)
}

async fn write_msg(stream: &mut TcpStream, msg: &Message) -> anyhow::Result<()> {
    stream.write_all(&msg.encode()).await?;
    Ok(())
}

fn response_to(req: &Message, body: Vec<u8>) -> Message {
    Message {
        seq: req.seq,
        msg_type: MessageType::Response,
        command: req.command,
        body,
    }
}

#[tokio::test]
async fn session_init_push_write_loopback() {
    let (listener, addr) = loopback_listener().await.unwrap();
    let fake = std::sync::Arc::new(FakeMixer {
        writes: StdMutex::new(Vec::new()),
    });
    let fake_task = {
        let fake = fake.clone();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut cic_count = 0;
            let mut seq: u8 = 40;
            let mut next_seq = move || {
                seq = seq.wrapping_add(1);
                if seq == 0 {
                    seq = 1;
                }
                seq
            };
            loop {
                let req = match read_frame(&mut stream).await {
                    Ok(m) => m,
                    Err(_) => break,
                };
                match (req.command, req.msg_type) {
                    (Command::KeepAlive, MessageType::Request) => {}
                    (Command::ChannelValues, MessageType::Request) if req.body.len() > 8 => {
                        if let Some(blocks) = parse_value_blocks(&req.body) {
                            let mut w = fake.writes.lock().unwrap();
                            for block in blocks {
                                for (i, raw) in block.raws.iter().enumerate() {
                                    w.push((block.start + i as u32, *raw));
                                }
                            }
                        }
                        let resp = response_to(&req, vec![]);
                        write_msg(&mut stream, &resp).await.unwrap();
                    }
                    _ => {
                        let resp = response_to(&req, vec![]);
                        write_msg(&mut stream, &resp).await.unwrap();
                        if req.command == Command::ChannelInfoControl {
                            cic_count += 1;
                            if cic_count == 2 {
                                let mut body = vec![];
                                body.extend_from_slice(&8u32.to_be_bytes());
                                body.extend_from_slice(&0x00000500u32.to_be_bytes());
                                body.extend_from_slice(&1u32.to_be_bytes());
                                body.extend_from_slice(&(-15.0f32).to_be_bytes());
                                let push = Message {
                                    seq: next_seq(),
                                    msg_type: MessageType::Request,
                                    command: Command::ChannelValues,
                                    body,
                                };
                                write_msg(&mut stream, &push).await.unwrap();
                                let text = b"Kick\x00\x00Snare\x00\x00\x00";
                                let mut padded = text.to_vec();
                                while !padded.len().is_multiple_of(4) {
                                    padded.push(0);
                                }
                                let mut nbody = vec![];
                                nbody.extend_from_slice(&1u32.to_be_bytes());
                                nbody.extend_from_slice(&0x00030000u32.to_be_bytes());
                                nbody.extend_from_slice(&word_swap(&padded));
                                let names = Message {
                                    seq: next_seq(),
                                    msg_type: MessageType::Request,
                                    command: Command::ChannelNames,
                                    body: nbody,
                                };
                                write_msg(&mut stream, &names).await.unwrap();
                                let mut sbody = vec![];
                                sbody.extend_from_slice(&5u32.to_be_bytes());
                                sbody.extend_from_slice(&2u32.to_be_bytes());
                                sbody.extend_from_slice(&0u32.to_be_bytes());
                                let mut nm = b"Main\x00".to_vec();
                                while !nm.len().is_multiple_of(4) {
                                    nm.push(0);
                                }
                                sbody.extend_from_slice(&word_swap(&nm));
                                let snap = Message {
                                    seq: next_seq(),
                                    msg_type: MessageType::Request,
                                    command: Command::ShowSnapshot,
                                    body: sbody,
                                };
                                write_msg(&mut stream, &snap).await.unwrap();
                            }
                        }
                    }
                }
            }
        })
    };

    let session = MackieSession::connect(&addr.to_string()).await.unwrap();

    // The snapshot is the final bootstrap frame. The first parameter push
    // can be cached before later names/snapshot frames have been processed.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    loop {
        if session.snapshot_names().contains_key(&2) {
            break;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("initial pushes never completed");
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(session.cached(8), Some(1));
    assert_eq!(session.cached(9), Some((-15.0f32).to_bits()));
    assert_eq!(session.cached(10), None);

    assert_eq!(session.name(1).as_deref(), Some("Kick"));
    assert_eq!(session.name(2), None); // blank slot
    assert_eq!(session.name(3).as_deref(), Some("Snare"));
    assert_eq!(
        session.snapshot_names().get(&2).map(String::as_str),
        Some("Main")
    );

    session.write_int(8, 0, VALUE_TYPE_PARAM).await.unwrap();
    session.write_float(9, -15.0).await.unwrap();
    session.set_mute_group(1, true).await.unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    loop {
        if fake.writes.lock().unwrap().len() >= 3 {
            break;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("writes never reached the fake");
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let writes = fake.writes.lock().unwrap().clone();
    assert!(writes.contains(&(8, 0)));
    assert!(writes.contains(&(9, (-15.0f32).to_bits())));
    assert!(writes.contains(&(11, 1))); // mute group 1 master
                                        // Own writes are cached locally without an echo.
    assert_eq!(session.cached(8), Some(0));
    assert_eq!(session.cached_type2(11), Some(1));

    session.close();
    fake_task.abort();
}

#[tokio::test]
async fn session_push_events_loopback() {
    // Push feed: every inbound value/name/snapshot frame is forwarded
    // to subscribers as well as into the caches. The fake answers init
    // like the passing bulk test, then fires one broadcast values
    // frame, one mixer-initiated names request and one snapshot recall
    // AFTER the final init step (2nd ChannelInfoControl) AND after the
    // test subscribes. The init response can share a TCP read with the
    // pushes, so acknowledging init alone does not guarantee a subscriber.
    let (listener, addr) = loopback_listener().await.unwrap();
    let (start_pushes, push_ready) = oneshot::channel::<()>();
    let fake_task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut seq: u8 = 80;
        let mut next_seq = move || {
            seq = seq.wrapping_add(1);
            if seq == 0 {
                seq = 1;
            }
            seq
        };
        let mut cic_count = 0;
        let mut fired = false;
        let mut push_ready = Some(push_ready);
        loop {
            let req = match read_frame(&mut stream).await {
                Ok(m) => m,
                Err(_) => break,
            };
            let resp = response_to(&req, vec![]);
            if write_msg(&mut stream, &resp).await.is_err() {
                break;
            }
            if req.command == Command::ChannelInfoControl {
                cic_count += 1;
            }
            if fired || cic_count < 2 {
                continue;
            }
            fired = true;
            push_ready.take().unwrap().await.expect("test subscribed");
            let mut body = vec![];
            body.extend_from_slice(&114u32.to_be_bytes());
            body.extend_from_slice(&0x00000500u32.to_be_bytes());
            body.extend_from_slice(&1u32.to_be_bytes());
            body.extend_from_slice(&(-15.0f32).to_be_bytes());
            let push = Message {
                seq: next_seq(),
                msg_type: MessageType::Broadcast,
                command: Command::ChannelValues,
                body,
            };
            write_msg(&mut stream, &push).await.unwrap();
            let padded = b"Keys".to_vec();
            let mut nbody = vec![];
            nbody.extend_from_slice(&4u32.to_be_bytes());
            nbody.extend_from_slice(&0x00010000u32.to_be_bytes());
            nbody.extend_from_slice(&word_swap(&padded));
            let names = Message {
                seq: next_seq(),
                msg_type: MessageType::Request,
                command: Command::ChannelNames,
                body: nbody,
            };
            write_msg(&mut stream, &names).await.unwrap();
            let sbody = vec![0, 0, 0, 1, 0, 0, 0, 7];
            let snap = Message {
                seq: next_seq(),
                msg_type: MessageType::Request,
                command: Command::ShowSnapshot,
                body: sbody,
            };
            write_msg(&mut stream, &snap).await.unwrap();
        }
    });

    let session = MackieSession::connect(&addr.to_string()).await.unwrap();
    let mut rx = session.subscribe_pushes();
    start_pushes
        .send(())
        .expect("fake mixer waiting for subscriber");
    let mut got_values = false;
    let mut got_names = false;
    let mut got_recall = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while !(got_values && got_names && got_recall) {
        if tokio::time::Instant::now() > deadline {
            panic!(
                "push events missing: values={got_values} names={got_names} recall={got_recall}"
            );
        }
        let ev = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("push timeout")
            .expect("push channel closed");
        match ev {
            PushEvent::Values {
                value_type,
                start,
                raws,
            } if value_type == 5 && start == 114 => {
                assert_eq!(raws, vec![1, (-15.0f32).to_bits()]);
                got_values = true;
            }
            PushEvent::Names { pairs } => {
                assert_eq!(pairs, vec![(4, "Keys".to_string())]);
                got_names = true;
            }
            PushEvent::SnapshotRecall(7) => got_recall = true,
            other => panic!("unexpected push event: {other:?}"),
        }
    }
    assert_eq!(session.cached(114), Some(1));
    assert_eq!(session.name(4).as_deref(), Some("Keys"));

    session.close();
    fake_task.abort();
}
