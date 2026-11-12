//! Readback-backed native inventory refresh.

use super::showdump::{collect_showdump, parse_showdump_line, ShowDump};
use super::*;

async fn read_slot_presence(session: &Arc<OscSession>, base: &str) -> anyhow::Result<bool> {
    let hasdata = read_int(session, &format!("{base}/hasdata")).await?;
    anyhow::ensure!(
        (0..=1).contains(&hasdata),
        "x32 native: {base}/hasdata outside 0..1"
    );
    let name = read_string(session, &format!("{base}/name")).await?;
    Ok(hasdata != 0 || !name.is_empty())
}

fn collect_inventory_reads<T>(
    expected: usize,
    values: Vec<anyhow::Result<T>>,
    label: &str,
    failures: &mut Vec<String>,
) -> Vec<T> {
    if values.len() != expected {
        failures.push(format!(
            "{label}: {} of {expected} workers returned",
            values.len()
        ));
    }
    values
        .into_iter()
        .filter_map(|value| match value {
            Ok(value) => Some(value),
            Err(error) => {
                failures.push(format!("{label}: {error:#}"));
                None
            }
        })
        .collect()
}

async fn read_scene_slot(session: Arc<OscSession>, slot: u8) -> anyhow::Result<NativeSceneSlot> {
    let base = format!("/-show/showfile/scene/{slot:03}");
    let hasdata = read_int(&session, &format!("{base}/hasdata")).await?;
    anyhow::ensure!(
        (0..=1).contains(&hasdata),
        "x32 native: scene hasdata out of range"
    );
    let name = read_string(&session, &format!("{base}/name")).await?;
    let notes = read_string(&session, &format!("{base}/notes")).await?;
    let safes = read_int(&session, &format!("{base}/safes")).await?;
    anyhow::ensure!(
        (0..=0xFF).contains(&safes),
        "x32 native: scene safes out of range"
    );
    Ok(NativeSceneSlot {
        slot,
        name,
        notes,
        safes: safes as u32,
        hasdata: hasdata != 0,
    })
}

async fn read_snippet_slot(
    session: Arc<OscSession>,
    slot: u8,
) -> anyhow::Result<NativeSnippetSlot> {
    let base = format!("/-show/showfile/snippet/{slot:03}");
    let hasdata = read_int(&session, &format!("{base}/hasdata")).await?;
    anyhow::ensure!(
        (0..=1).contains(&hasdata),
        "x32 native: snippet hasdata out of range"
    );
    let name = read_string(&session, &format!("{base}/name")).await?;
    let eventtyp = read_int(&session, &format!("{base}/eventtyp")).await?;
    let channels = read_int(&session, &format!("{base}/channels")).await?;
    let auxbuses = read_int(&session, &format!("{base}/auxbuses")).await?;
    let maingrps = read_int(&session, &format!("{base}/maingrps")).await?;
    anyhow::ensure!(
        (0..=0x1FF_FFFF).contains(&eventtyp),
        "x32 native: snippet eventtyp out of range"
    );
    anyhow::ensure!(
        (0..=0xFFFF).contains(&maingrps),
        "x32 native: snippet maingrps out of range"
    );
    Ok(NativeSnippetSlot {
        slot,
        name,
        eventtyp: eventtyp as u32,
        channels: channels as u32,
        auxbuses: auxbuses as u32,
        maingrps: maingrps as u32,
        hasdata: hasdata != 0,
    })
}

async fn read_lib_slot(
    session: Arc<OscSession>,
    bank: String,
    slot: u8,
) -> anyhow::Result<NativeLibSlot> {
    let base = format!("/-libs/{bank}/{slot:03}");
    let hasdata = read_int(&session, &format!("{base}/hasdata")).await?;
    anyhow::ensure!(
        (0..=1).contains(&hasdata),
        "x32 native: lib hasdata out of range"
    );
    let name = read_string(&session, &format!("{base}/name")).await?;
    let typ = read_int(&session, &format!("{base}/type")).await?;
    let flags = read_int(&session, &format!("{base}/flags")).await?;
    Ok(NativeLibSlot {
        slot,
        name,
        typ,
        flags: flags as u32,
        hasdata: hasdata != 0,
    })
}

async fn read_cue_numb(
    session: Arc<OscSession>,
    position: u16,
) -> anyhow::Result<Option<(u16, i32)>> {
    let numb = read_int(&session, &format!("/-show/showfile/cue/{position:03}/numb")).await?;
    Ok((numb != 0).then_some((position, numb)))
}

async fn read_cue_slot(session: Arc<OscSession>, position: u16) -> anyhow::Result<NativeCueSlot> {
    let base = format!("/-show/showfile/cue/{position:03}");
    let numb = read_int(&session, &format!("{base}/numb")).await?;
    anyhow::ensure!(numb != 0, "x32 native: cue position empty");
    let name = read_string(&session, &format!("{base}/name")).await?;
    let skip = read_int(&session, &format!("{base}/skip")).await?;
    let scene = read_int(&session, &format!("{base}/scene")).await?;
    let snippet = read_int(&session, &format!("{base}/bit")).await?;
    let miditype = read_int(&session, &format!("{base}/miditype")).await?;
    let midichan = read_int(&session, &format!("{base}/midichan")).await?;
    let midipara1 = read_int(&session, &format!("{base}/midipara1")).await?;
    let midipara2 = read_int(&session, &format!("{base}/midipara2")).await?;
    anyhow::ensure!((0..=1).contains(&skip), "x32 native: cue skip out of range");
    anyhow::ensure!(
        (-1..100).contains(&scene),
        "x32 native: cue scene ref out of range"
    );
    anyhow::ensure!(
        (-1..100).contains(&snippet),
        "x32 native: cue snippet ref out of range"
    );
    anyhow::ensure!(
        (0..=3).contains(&miditype),
        "x32 native: cue miditype out of range"
    );
    Ok(NativeCueSlot {
        position,
        numb,
        name,
        skip: skip != 0,
        scene,
        snippet,
        miditype,
        midichan,
        midipara1,
        midipara2,
    })
}

impl X32Driver {
    /// Refresh observed inventory; unreadable entries leave it incomplete.
    pub async fn native_refresh_into(
        &self,
        state: &Mutex<NativeState>,
        check: Check<'_>,
    ) -> anyhow::Result<Value> {
        let session = self.session()?;
        let _guard = self.writes.lock().await;
        self.ensure_current(&session)?;
        anyhow::ensure!(check(), "x32 native: session changed before refresh");

        // Require native show support before launching the full inventory scan.
        let (scene_present, snippet_present, _) = tokio::try_join!(
            read_int(&session, "/-show/showfile/scene/000/hasdata"),
            read_int(&session, "/-show/showfile/snippet/000/hasdata"),
            read_int(&session, "/-show/showfile/cue/000/numb"),
        )
        .map_err(|error| {
            anyhow::anyhow!("x32 native: required native show inventory unavailable: {error:#}")
        })?;
        anyhow::ensure!(
            (0..=1).contains(&scene_present) && (0..=1).contains(&snippet_present),
            "x32 native: required inventory hasdata must be 0 or 1"
        );
        anyhow::ensure!(
            check(),
            "x32 native: session changed during inventory preflight"
        );
        let show_name = read_string(&session, "/-show/showfile/show/name")
            .await
            .ok();
        let mut dump = ShowDump::default();
        for line in collect_showdump(&session).await {
            parse_showdump_line(&mut dump, &line);
        }
        anyhow::ensure!(check(), "x32 native: session changed during refresh");
        let mut failures = Vec::new();

        let mut scenes = dump.scenes;
        let known: Vec<u8> = scenes.iter().map(|slot| slot.slot).collect();
        let missing: Vec<u8> = (0..SCENE_SLOTS as u8)
            .filter(|slot| !known.contains(slot))
            .collect();
        let count = missing.len();
        let presence = fan_out_simple(missing, |slot| {
            let session = session.clone();
            async move {
                let occupied =
                    read_slot_presence(&session, &format!("/-show/showfile/scene/{slot:03}"))
                        .await?;
                Ok(occupied.then_some(slot))
            }
        })
        .await;
        let needed: Vec<u8> =
            collect_inventory_reads(count, presence, "Scene presence", &mut failures)
                .into_iter()
                .flatten()
                .collect();
        anyhow::ensure!(check(), "x32 native: session changed during refresh");
        let count = needed.len();
        let entries = fan_out_simple(needed, |slot| {
            let session = session.clone();
            async move { read_scene_slot(session, slot).await }
        })
        .await;
        scenes.extend(collect_inventory_reads(
            count,
            entries,
            "Scene details",
            &mut failures,
        ));
        scenes.sort_by_key(|slot| slot.slot);

        let mut snippets = dump.snippets;
        let known: Vec<u8> = snippets.iter().map(|slot| slot.slot).collect();
        let missing: Vec<u8> = (0..SNIPPET_SLOTS as u8)
            .filter(|slot| !known.contains(slot))
            .collect();
        let count = missing.len();
        let presence = fan_out_simple(missing, |slot| {
            let session = session.clone();
            async move {
                let occupied =
                    read_slot_presence(&session, &format!("/-show/showfile/snippet/{slot:03}"))
                        .await?;
                Ok(occupied.then_some(slot))
            }
        })
        .await;
        let needed: Vec<u8> =
            collect_inventory_reads(count, presence, "Snippet presence", &mut failures)
                .into_iter()
                .flatten()
                .collect();
        anyhow::ensure!(check(), "x32 native: session changed during refresh");
        let count = needed.len();
        let entries = fan_out_simple(needed, |slot| {
            let session = session.clone();
            async move { read_snippet_slot(session, slot).await }
        })
        .await;
        snippets.extend(collect_inventory_reads(
            count,
            entries,
            "Snippet details",
            &mut failures,
        ));
        snippets.sort_by_key(|slot| slot.slot);

        let mut cues = dump.cues;
        let known: Vec<u16> = cues.iter().map(|slot| slot.position).collect();
        let missing: Vec<u16> = (0..CUE_POSITIONS as u16)
            .filter(|slot| !known.contains(slot))
            .collect();
        let count = missing.len();
        let presence = fan_out_simple(missing, |position| {
            let session = session.clone();
            async move { read_cue_numb(session, position).await }
        })
        .await;
        let occupied: Vec<u16> =
            collect_inventory_reads(count, presence, "Cue presence", &mut failures)
                .into_iter()
                .flatten()
                .map(|(position, _)| position)
                .collect();
        anyhow::ensure!(check(), "x32 native: session changed during refresh");
        let count = occupied.len();
        let entries = fan_out_simple(occupied, |position| {
            let session = session.clone();
            async move { read_cue_slot(session, position).await }
        })
        .await;
        cues.extend(collect_inventory_reads(
            count,
            entries,
            "Cue details",
            &mut failures,
        ));
        cues.sort_by_key(|slot| slot.position);

        // Library support is optional. Once a bank answers, every subsequent
        // required read must succeed before its inventory can be complete.
        let libs_supported = matches!(
            tokio::time::timeout(
                Duration::from_millis(500),
                read_int(&session, "/-libs/ch/001/hasdata")
            )
            .await,
            Ok(Ok(0 | 1))
        );
        let mut lib_ch = Vec::new();
        let mut lib_fx = Vec::new();
        let mut lib_r = Vec::new();
        let mut lib_mon = Vec::new();
        if libs_supported {
            for bank in ["ch", "fx", "r", "mon"] {
                let presence = fan_out_simple((1..=LIB_SLOTS as u8).collect(), |slot| {
                    let session = session.clone();
                    async move {
                        let occupied =
                            read_slot_presence(&session, &format!("/-libs/{bank}/{slot:03}"))
                                .await?;
                        Ok(occupied.then_some(slot))
                    }
                })
                .await;
                let occupied: Vec<u8> =
                    collect_inventory_reads(LIB_SLOTS, presence, "Preset presence", &mut failures)
                        .into_iter()
                        .flatten()
                        .collect();
                anyhow::ensure!(check(), "x32 native: session changed during refresh");
                let count = occupied.len();
                let entries = fan_out_simple(occupied, |slot| {
                    let session = session.clone();
                    async move { read_lib_slot(session, bank.to_string(), slot).await }
                })
                .await;
                let entries =
                    collect_inventory_reads(count, entries, "Preset details", &mut failures);
                match bank {
                    "ch" => lib_ch = entries,
                    "fx" => lib_fx = entries,
                    "r" => lib_r = entries,
                    _ => lib_mon = entries,
                }
            }
        }
        anyhow::ensure!(check(), "x32 native: session changed during refresh");
        self.ensure_current(&session)?;
        let error = (!failures.is_empty()).then(|| format!(
            "Native inventory incomplete: {} failed reads ({}). Observed rows are partial; refresh before changing native entries.",
            failures.len(), failures.iter().take(8).cloned().collect::<Vec<_>>().join("; ")
        ));
        let mut cache = state
            .lock()
            .map_err(|_| anyhow::anyhow!("x32 native: state lock poisoned"))?;
        let revision = cache.revision + 1;
        *cache = NativeState {
            owner: Some(Arc::downgrade(&session)),
            revision,
            generation: revision,
            connected: true,
            complete: failures.is_empty(),
            error,
            show_name,
            scenes,
            snippets,
            cues,
            libs_supported: Some(libs_supported),
            lib_ch,
            lib_fx,
            lib_r,
            lib_mon,
        };
        Ok(cache.inventory_json(Some(&session)))
    }
}
