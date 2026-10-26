use super::*;

/// Mailbox coalescing harness: a detached entry + mailbox pair with no
/// ticker attached, so the bound assertions are exact.
fn stalled_pair() -> (ClientEntry, ClientMailbox) {
    let (tx, rx) = mpsc::unbounded_channel();
    let rt = Arc::new(std::sync::Mutex::new(RtSlots::default()));
    let queued = Arc::new(AtomicUsize::new(0));
    let entry = ClientEntry {
        mailbox: tx.clone(),
        rt: Arc::clone(&rt),
        queued: Arc::clone(&queued),
        label: "test".to_string(),
        connected_at: Instant::now(),
        last_write: None,
        policy: Policy::admin(),
        binary_meters: false,
        taps: false,
        caps_full: false,
    };
    (entry, ClientMailbox::new(rx, tx, rt, queued))
}

fn meters_frame(v: f32) -> SharedEvent {
    SharedEvent::new(Event::Meters { levels: vec![v] })
}

#[tokio::test]
async fn stalled_mailbox_coalesces_meter_flood_to_latest() {
    let (entry, mut mailbox) = stalled_pair();
    for i in 0..50u32 {
        Engine::push_to(&entry, meters_frame(i as f32));
    }
    // Bound: one queued frame plus one slot, however long the stall.
    assert_eq!(mailbox.len(), 2, "flood must collapse to queued + slot");
    // The first delivery is the frame that was already queued; draining
    // it promotes the latest behind it.
    let first = mailbox.try_recv().expect("queued meters frame");
    assert!(
        matches!(&*first.event, Event::Meters { levels } if levels == &vec![0.0]),
        "queued frame is delivered first"
    );
    let mut last = first;
    while let Ok(frame) = mailbox.try_recv() {
        last = frame;
    }
    assert!(
        matches!(&*last.event, Event::Meters { levels } if levels == &vec![49.0]),
        "latest frame must survive the flood"
    );
    assert!(mailbox.is_empty(), "slots must drain, not linger");
}

#[tokio::test]
async fn stalled_mailbox_never_drops_control_under_meter_flood() {
    let (entry, mut mailbox) = stalled_pair();
    for i in 0..20u16 {
        Engine::push_to(&entry, meters_frame(i as f32));
        Engine::push_to(
            &entry,
            SharedEvent::new(Event::Param {
                ch: i,
                field: "fader".to_string(),
                value: json!(0.5),
                key: None,
                by: None,
                authority: None,
            }),
        );
    }
    Engine::push_to(
        &entry,
        SharedEvent::new(Event::Error {
            message: "boom".to_string(),
            locked_by: None,
            ch: None,
        }),
    );
    let mut params = Vec::new();
    let mut errors = 0u32;
    let mut meters = Vec::new();
    while let Ok(frame) = mailbox.try_recv() {
        match &*frame.event {
            Event::Param { ch, .. } => params.push(*ch),
            Event::Error { .. } => errors += 1,
            Event::Meters { levels } => meters.push(levels.clone()),
            other => panic!("unexpected frame in drain: {}", other.tag()),
        }
    }
    assert_eq!(
        params,
        (0..20u16).collect::<Vec<_>>(),
        "control order must survive the flood"
    );
    assert_eq!(errors, 1, "errors are never coalesced");
    assert!(
        meters.len() <= 2,
        "meters collapse to queued + latest, got {}",
        meters.len()
    );
    assert_eq!(
        meters.last().unwrap(),
        &vec![19.0],
        "latest meters frame must survive"
    );
    assert!(mailbox.is_empty());
}

#[tokio::test]
async fn stalled_mailbox_keeps_latest_per_realtime_key() {
    let (entry, mut mailbox) = stalled_pair();
    let gr = |ch: u16, gate: f32| {
        SharedEvent::new(Event::Gr {
            ch,
            gate,
            comp: 0.0,
        })
    };
    // First frame queues; the rest collapse per channel key.
    Engine::push_to(&entry, gr(0, 1.0));
    Engine::push_to(&entry, gr(1, 1.0));
    Engine::push_to(&entry, gr(0, 2.0));
    Engine::push_to(
        &entry,
        SharedEvent::new(Event::Spectrum {
            ch: 3,
            bins: vec![0.5],
        }),
    );
    Engine::push_to(
        &entry,
        SharedEvent::new(Event::Taps {
            tap: "in".to_string(),
            levels: vec![0.25],
        }),
    );
    // 1 queued + 4 slots (gr/0, gr/1, spectrum/3, taps/in).
    assert_eq!(mailbox.len(), 5);
    let mut gates = Vec::new();
    let mut taps = 0u32;
    let mut spectra = 0u32;
    while let Ok(frame) = mailbox.try_recv() {
        match &*frame.event {
            Event::Gr { gate, .. } => gates.push(*gate),
            Event::Taps { .. } => taps += 1,
            Event::Spectrum { .. } => spectra += 1,
            other => panic!("unexpected frame in drain: {}", other.tag()),
        }
    }
    assert_eq!(
        gates,
        vec![1.0, 2.0, 1.0],
        "per-channel latest, queued first"
    );
    assert_eq!((taps, spectra), (1, 1));
    assert!(mailbox.is_empty());
}
