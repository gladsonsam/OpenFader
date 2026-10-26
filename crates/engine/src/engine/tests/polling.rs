use super::*;

#[tokio::test]
async fn focus_jumps_queue() {
    // Parked poller: exact broadcast-sequence assertions below race
    // the live 50 ms poller (it steals focus entries mid-test; the
    // tail of the manual block then falls through to round-robin).
    let engine = parked_engine();
    let mut client = engine.attach(None).await;
    // Drain the attach snapshot (no params frames: cache starts empty).
    for _ in 0..6 {
        client.events.recv().await.expect("snapshot");
    }
    settle_parked_poller(&engine, &mut client).await;
    // Advance partway into channel 0's block. The parked tick already
    // warmed ch0's essentials through the core lane (fader/mute/name),
    // so 2 of these 10 round-robin reads are cache hits and stay
    // quiet, 8 frames land.
    for _ in 0..10 {
        engine.poll_once().await;
    }
    // Focus channel 5: the next whole block of reads is channel 5.
    engine.request_focus("client-1", &[5]);
    // The focus block is ch5's SCOPED set (catalogue scopes decide;
    // e.g. bus/main-only EQ bands are not in it).
    let nparams = engine
        .poll_list("sim")
        .await
        .iter()
        .filter(|(c, _)| *c == 5)
        .count();
    assert!(nparams > 100, "focused strip carries the input set");
    for _ in 0..nparams {
        engine.poll_once().await;
    }
    // Diff frames land in the mailbox in poll order: 8 round-robin
    // reads (10 minus the 2 core-warmed dups), then the whole focus
    // block on ch5.
    let mut chs: Vec<u16> = Vec::new();
    while let Ok(event) = client.events.try_recv() {
        if let Event::Params { ch, .. } = &*event {
            chs.push(*ch);
        }
    }
    assert_eq!(
        chs.len(),
        8 + nparams,
        "8 round-robin + {nparams} focus reads"
    );
    assert!(
        chs[8..].iter().all(|&c| c == 5),
        "focus block is all ch5, got {chs:?}"
    );
    // Channel 5's cache is complete while channel 0's is still partial.
    let cache = engine.param_cache.read().unwrap();
    assert_eq!(
        cache.get(&5).map(|m| m.len()),
        Some(nparams),
        "focused ch5 complete"
    );
    assert!(
        cache.get(&0).map(|m| m.len()).unwrap_or(0) < nparams,
        "ch0 still partial"
    );
}

#[tokio::test]
async fn sim_full_cycle_fills_every_strip() {
    // Engine invariant: after one full poll cycle every strip has a
    // value for every supported scoped id, the poller never silently
    // skips a supported (strip, param) pair.
    let engine = Engine::new(EngineConfig::default());
    let list = engine.poll_list("sim").await;
    for _ in 0..list.len() {
        engine.poll_once().await;
    }
    let cache = engine.param_cache.read().unwrap();
    let topo = engine.driver_topology("sim");
    let sim_caps = engine.driver_capabilities("sim");
    for fam in &topo {
        let want: Vec<&&str> = sim_caps
            .iter()
            .filter(|id| {
                engine
                    .registry("sim")
                    .find(id)
                    .map(|def| openfader_core::params::supports_kind(def, fam.kind))
                    .unwrap_or(true)
            })
            .collect();
        for s in fam.first..fam.first + fam.count {
            let got = cache.get(&(s as u16)).map(|m| m.len()).unwrap_or(0);
            assert_eq!(
                got,
                want.len(),
                "strip {s} has every scoped param after one cycle ({got}/{})",
                want.len()
            );
            for id in &want {
                assert!(
                    cache.get(&(s as u16)).is_some_and(|m| m.contains_key(**id)),
                    "strip {s} misses {id}"
                );
            }
        }
    }
}

#[tokio::test]
async fn focus_flood_superseded_by_latest() {
    // CATALOGUE-V2 §5: flood the queue with 30 focus commands, then
    // focus strip N, N's full set drains first (most-recent-first),
    // ahead of the flood backlog. Parked poller + settle: the live
    // poller would otherwise steal focus entries mid-test (see
    // `focus_jumps_queue`).
    let engine = parked_engine();
    let mut client = engine.attach(None).await;
    for _ in 0..6 {
        client.events.recv().await.expect("snapshot");
    }
    settle_parked_poller(&engine, &mut client).await;
    for i in 0..30u16 {
        engine.request_focus("client-1", &[i % 32]);
    }
    // Strip 40 (first extended input, Ch33) was never flooded: its whole
    // scoped set must drain before any flooded strip's leftovers.
    engine.request_focus("client-1", &[40]);
    let scoped = engine
        .poll_list("sim")
        .await
        .iter()
        .filter(|(c, _)| *c == 40)
        .count();
    assert!(scoped > 100, "strip 40 has the full input set");
    for _ in 0..scoped {
        engine.poll_once().await;
    }
    let mut chs: Vec<u16> = Vec::new();
    while let Ok(event) = client.events.try_recv() {
        if let Event::Params { ch, .. } = &*event {
            chs.push(*ch);
        }
    }
    assert_eq!(chs.len(), scoped, "focus block is exactly strip 40's set");
    assert!(
        chs.iter().all(|&c| c == 40),
        "newest focus drains first: {chs:?}"
    );
    // No duplicates from the flood: the queue holds one entry per
    // (strip, param) at most.
    let q = engine.focus_queue.read().unwrap();
    let mut seen = std::collections::HashSet::new();
    for (_, ch, param, _) in q.iter() {
        assert!(
            seen.insert((*ch, param.clone())),
            "duplicate ({ch}, {param})"
        );
    }
}

#[tokio::test]
async fn focus_queue_capped_and_detach_drops() {
    let engine = Engine::new(EngineConfig::default());
    let alice = engine.attach(None).await;
    let bob = engine.attach(None).await;
    // Repeated focus on every strip grows the queue, but it stays capped
    // (~4 sweeps) instead of growing without bound.
    for _ in 0..10 {
        for ch in 0..engine.driver_strips("sim") as u16 {
            engine.request_focus(&alice.id, &[ch]);
        }
    }
    let nstrips = engine.driver_strips("sim");
    let nids = engine.driver_capabilities("sim").len();
    assert!(engine.focus_queue.read().unwrap().len() <= 4 * nstrips * nids);
    // Bob's focus is queued…
    engine.request_focus(&bob.id, &[7]);
    assert!(
        engine
            .focus_queue
            .read()
            .unwrap()
            .iter()
            .any(|(_, _, _, o)| o == &bob.id),
        "bob's focus queued"
    );
    let bob_id = bob.id.clone();
    // …and dropped with him when he detaches, while Alice's work stays.
    drop(bob);
    let q = engine.focus_queue.read().unwrap();
    assert!(
        q.iter().all(|(_, _, _, o)| o != &bob_id),
        "detached client's focus dropped"
    );
    assert!(
        q.iter().any(|(_, _, _, o)| o == &alice.id),
        "survivor's focus kept"
    );
}
