//! Shared loopback desk and connected-driver fixture.
use super::*;

/// In-process fake XR18: UDP loopback socket speaking real OSC via transport codecs.
/// Fader stores quantize writes to 10-bit steps like the real desk
/// (`(v*1023).floor()/1023`, so 0.5 stores as 0.49951124 — baseline T8),
/// which is why fader asserts use `FADER_READBACK_EPSILON`, not exact
/// equality. Meter streaming only starts after the documented
/// `/meters` + stream-id-string subscribe (T4).
pub(super) struct FakeMixer {
    pub(super) socket: Arc<UdpSocket>,
    pub(super) faders: StdMutex<HashMap<String, f32>>,
    pub(super) ons: StdMutex<HashMap<String, i32>>,
    pub(super) names: StdMutex<HashMap<String, String>>,
    /// FX params answer bit-exact (no quantization, baseline §6 row 5).
    /// Stored verbatim (`Float` or `Int`) so enum/switch pars keep
    /// their hardware INT wire shape; untouched pars read back 0.0.
    pub(super) fx: StdMutex<HashMap<String, OscType>>,
    /// Loaded effect per slot (`/fx/N/type` int `0..=60`); unset slots
    /// read back 0 (HALL) like a freshly-booted desk.
    pub(super) fx_types: StdMutex<HashMap<u8, i32>>,
    /// Headamp gains store the verbatim normalized wire float
    /// (hardware quantization uncharacterized — see `crate::headamp`).
    pub(super) gains: StdMutex<HashMap<String, f32>>,
    /// Headamp phantom switches store the verbatim int (`1`/`0`).
    pub(super) phantoms: StdMutex<HashMap<String, i32>>,
    /// Input-routing int leaves (`/config/rtnsrc`, `/preamp/invert`,
    /// `/preamp/hpon`): verbatim ints. `insrc` / channel `rtnsw` have
    /// NO arms — the fake stays silent there like the live desk.
    pub(super) input_ints: StdMutex<HashMap<String, i32>>,
    /// Input-routing float leaves (`/preamp/rtntrim`, `/preamp/hpf`):
    /// verbatim wire floats.
    pub(super) input_floats: StdMutex<HashMap<String, f32>>,
    /// Param-plane int leaves (`/eq/on`, `/gate/on`, `/dyn/on`,
    /// `/eq/N/type`): verbatim ints, defaulting to 0 (off / first
    /// filter type) like an untouched desk strip.
    pub(super) plane_ints: StdMutex<HashMap<String, i32>>,
    /// Param-plane float leaves (`/eq/N/{g,f,q}`, `/mix/pan`,
    /// `/mix/MM/level`): verbatim wire floats. Untouched defaults:
    /// eq `g`/`f`/`q` 0.5, `pan` 0.5 (centre), send `level` 0.0
    /// (−90 dB floor).
    pub(super) plane_floats: StdMutex<HashMap<String, f32>>,
    pub(super) meter_subscribe: StdMutex<Option<rosc::OscMessage>>,
    pub(super) dyn_subscribe: StdMutex<Option<rosc::OscMessage>>,
    /// Resubscribe tolerance counters: the renewal loop re-sends
    /// `/meters` subscribes (and `/xremote`) on schedule, so the fake
    /// counts every hit instead of asserting exactly-one.
    pub(super) meter_count: StdMutex<u64>,
    pub(super) dyn_count: StdMutex<u64>,
    pub(super) xremote_count: StdMutex<u64>,
    /// Most recently seen peer (any incoming datagram refreshes it):
    /// `broadcast` fires unsolicited datagrams there, simulating a
    /// desk-side change broadcast to `/xremote` subscribers.
    pub(super) last_peer: StdMutex<Option<SocketAddr>>,
}

impl FakeMixer {
    pub(super) async fn spawn() -> (SocketAddr, Arc<Self>) {
        let socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let addr = socket.local_addr().unwrap();
        let fake = Arc::new(Self {
            socket: socket.clone(),
            faders: StdMutex::new(HashMap::new()),
            ons: StdMutex::new(HashMap::from([("/ch/01/mix/on".to_string(), 1)])),
            names: StdMutex::new(HashMap::new()),
            fx: StdMutex::new(HashMap::new()),
            fx_types: StdMutex::new(HashMap::new()),
            gains: StdMutex::new(HashMap::new()),
            phantoms: StdMutex::new(HashMap::new()),
            input_ints: StdMutex::new(HashMap::new()),
            input_floats: StdMutex::new(HashMap::new()),
            plane_ints: StdMutex::new(HashMap::new()),
            plane_floats: StdMutex::new(HashMap::new()),
            meter_subscribe: StdMutex::new(None),
            dyn_subscribe: StdMutex::new(None),
            meter_count: StdMutex::new(0),
            dyn_count: StdMutex::new(0),
            xremote_count: StdMutex::new(0),
            last_peer: StdMutex::new(None),
        });
        let worker = fake.clone();
        tokio::spawn(async move {
            let mut buf = vec![0u8; 65535];
            // Preload defaults. Bus keys are single-digit: the real desk
            // is silent on `/bus/01/…` (T2), and so is this fake.
            worker
                .faders
                .lock()
                .unwrap()
                .insert("/ch/01/mix/fader".into(), 0.75);
            worker
                .faders
                .lock()
                .unwrap()
                .insert("/bus/1/mix/fader".into(), 0.7);
            worker
                .faders
                .lock()
                .unwrap()
                .insert(addrs::MAIN_FADER.into(), 0.8);
            loop {
                let Ok((n, peer)) = worker.socket.recv_from(&mut buf).await else {
                    break;
                };
                *worker.last_peer.lock().unwrap() = Some(peer);
                let Ok(packet) = osc::decode_packet(&buf[..n]) else {
                    continue;
                };
                for msg in osc::flatten(packet) {
                    let reply: Option<Vec<u8>> = match (msg.addr.as_str(), msg.args.first()) {
                        ("/xremote", _) => {
                            *worker.xremote_count.lock().unwrap() += 1;
                            None
                        }
                        ("/meters", Some(OscType::String(id))) if id == addrs::METER_SUBSCRIBE => {
                            // Documented subscribe form (T4): record it,
                            // then stream blobs at ~50ms like the desk.
                            // Resubscribes (renewal loop) just restart the
                            // burst — the desk tolerates them, so does this fake.
                            *worker.meter_subscribe.lock().unwrap() = Some(msg.clone());
                            *worker.meter_count.lock().unwrap() += 1;
                            let sock = worker.socket.clone();
                            tokio::spawn(async move {
                                for _ in 0..4 {
                                    let blob = build_meter_blob(&[0.1, 0.5, 0.9]);
                                    let bytes = osc::encode_message(
                                        addrs::METER_SUBSCRIBE,
                                        vec![OscType::Blob(blob)],
                                    )
                                    .unwrap();
                                    let _ = sock.send_to(&bytes, peer).await;
                                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                                }
                            });
                            None
                        }
                        ("/meters", Some(OscType::String(id)))
                            if id == addrs::METER_DYN_SUBSCRIBE =>
                        {
                            // Documented ALL DYN subscribe (dyn_gr): record
                            // it, then stream the documented blob shape
                            // (ch1 gate 6 dB, ch1 comp 12 dB) like a desk
                            // with dynamics engaged would. Resubscribes
                            // restart the burst, like the levels stream.
                            *worker.dyn_subscribe.lock().unwrap() = Some(msg.clone());
                            *worker.dyn_count.lock().unwrap() += 1;
                            let sock = worker.socket.clone();
                            tokio::spawn(async move {
                                let mut m = dyn_gr::DynMeters {
                                    gate: [0.0; dyn_gr::DYN_CHANNELS],
                                    comp: [0.0; dyn_gr::DYN_CHANNELS],
                                    bus: [0.0; dyn_gr::DYN_BUSES],
                                    lr: 0.0,
                                };
                                m.gate[0] = 6.0;
                                m.comp[0] = 12.0;
                                for _ in 0..2 {
                                    let blob = dyn_gr::build_dyn_blob(&m);
                                    let bytes = osc::encode_message(
                                        addrs::METER_DYN_SUBSCRIBE,
                                        vec![OscType::Blob(blob)],
                                    )
                                    .unwrap();
                                    let _ = sock.send_to(&bytes, peer).await;
                                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                                }
                            });
                            None
                        }
                        ("/info", _) if msg.args.is_empty() => Some(
                            osc::encode_message(
                                "/info",
                                vec![
                                    OscType::String("V0.04".into()),
                                    OscType::String("FakeXR".into()),
                                    OscType::String("XR18".into()),
                                    OscType::String("1.22".into()),
                                ],
                            )
                            .unwrap(),
                        ),
                        // Faders quantize like hardware (T8); on/name are exact.
                        // DCA faders ride the same 1023-step law on the
                        // desk (uncharacterized — same treatment here).
                        (a, Some(OscType::Float(f)))
                            if a.ends_with("/mix/fader")
                                || a.starts_with("/dca/") && a.ends_with("/fader") =>
                        {
                            let q = (*f * 1023.0).floor() / 1023.0;
                            worker.faders.lock().unwrap().insert(a.to_string(), q);
                            None
                        }
                        // FX type selects store verbatim (int form).
                        (a, Some(OscType::Int(i)))
                            if a.starts_with("/fx/") && a.ends_with("/type") =>
                        {
                            if let Some(slot) =
                                a.split('/').nth(2).and_then(|s| s.parse::<u8>().ok())
                            {
                                worker.fx_types.lock().unwrap().insert(slot, *i);
                            }
                            None
                        }
                        // FX params are bit-exact on hardware (baseline
                        // §6 row 5): store verbatim (int or float), no
                        // quantization — enum/switch pars keep INT shape.
                        (a, Some(arg @ (OscType::Float(_) | OscType::Int(_))))
                            if a.starts_with("/fx/") && a.contains("/par/") =>
                        {
                            worker.fx.lock().unwrap().insert(a.to_string(), arg.clone());
                            None
                        }
                        // Headamp gain stores the verbatim normalized
                        // wire float (quantization uncharacterized).
                        (a, Some(OscType::Float(f)))
                            if a.starts_with("/headamp/") && a.ends_with("/gain") =>
                        {
                            worker.gains.lock().unwrap().insert(a.to_string(), *f);
                            None
                        }
                        (a, Some(OscType::Int(i)))
                            if a.starts_with("/headamp/") && a.ends_with("/phantom") =>
                        {
                            worker.phantoms.lock().unwrap().insert(a.to_string(), *i);
                            None
                        }
                        // Input-routing int leaves store verbatim (`rtnsrc`
                        // U01..U18 index, `invert`/`hpon` 1/0). Deliberately
                        // NO arms for `insrc` or channel `rtnsw`: the fake
                        // stays silent there exactly like the live desk.
                        (a, Some(OscType::Int(i)))
                            if a.ends_with("/config/rtnsrc")
                                || a.ends_with("/config/insrc")
                                || a.ends_with("/preamp/rtnsw")
                                || a.ends_with("/preamp/invert")
                                || a.ends_with("/preamp/hpon") =>
                        {
                            worker.input_ints.lock().unwrap().insert(a.to_string(), *i);
                            None
                        }
                        // Input-routing float leaves store the verbatim
                        // wire float (desk quantization uncharacterized;
                        // live ch16 reads rtntrim 0.0 / hpf 0.0).
                        (a, Some(OscType::Float(f)))
                            if a.ends_with("/preamp/rtntrim") || a.ends_with("/preamp/hpf") =>
                        {
                            worker
                                .input_floats
                                .lock()
                                .unwrap()
                                .insert(a.to_string(), *f);
                            None
                        }
                        // Param-plane int leaves store verbatim
                        // (`/eq/on`, `/gate/on`, `/dyn/on`, `/eq/N/type`
                        // all int 1/0 or 0..5). Deliberately NO arms for
                        // send mutes (`/ch/NN/mix/MM/on`): the fake stays
                        // silent there exactly like the live desk (T7).
                        (a, Some(OscType::Int(i)))
                            if a.ends_with("/eq/on")
                                || a.ends_with("/gate/on")
                                || a.ends_with("/dyn/on")
                                || (a.contains("/eq/") && a.ends_with("/type")) =>
                        {
                            worker.plane_ints.lock().unwrap().insert(a.to_string(), *i);
                            None
                        }
                        // Param-plane float leaves store the verbatim
                        // wire float (`/eq/N/{g,f,q}`, `/mix/pan`,
                        // `/mix/MM/level`; desk quantization of these is
                        // uncharacterized — no fader-style rounding here).
                        (a, Some(OscType::Float(f)))
                            if (a.contains("/eq/")
                                && (a.ends_with("/g")
                                    || a.ends_with("/f")
                                    || a.ends_with("/q")))
                                || a.ends_with("/mix/pan")
                                || (a.contains("/mix/") && a.ends_with("/level")) =>
                        {
                            worker
                                .plane_floats
                                .lock()
                                .unwrap()
                                .insert(a.to_string(), *f);
                            None
                        }
                        (a, Some(OscType::Int(i)))
                            if a.ends_with("/mix/on")
                                || a.starts_with("/dca/") && a.ends_with("/on") =>
                        {
                            worker.ons.lock().unwrap().insert(a.to_string(), *i);
                            None
                        }
                        (a, Some(OscType::String(s))) if a.ends_with("/config/name") => {
                            worker
                                .names
                                .lock()
                                .unwrap()
                                .insert(a.to_string(), s.clone());
                            None
                        }
                        // Group-assignment bitmasks (`{root}/grp/dca`,
                        // `{root}/grp/mute`): the live desk keeps only
                        // the low 4 bits (writes of 32/63 read back 15),
                        // so the fake masks on store — an unmasked store
                        // would pass a test the desk would fail.
                        (a, Some(OscType::Int(i)))
                            if a.ends_with("/grp/dca") || a.ends_with("/grp/mute") =>
                        {
                            worker
                                .plane_ints
                                .lock()
                                .unwrap()
                                .insert(a.to_string(), *i & 0x0F);
                            None
                        }
                        (a, None) if a.ends_with("/grp/dca") || a.ends_with("/grp/mute") => {
                            let v = worker
                                .plane_ints
                                .lock()
                                .unwrap()
                                .get(a)
                                .copied()
                                .unwrap_or(0);
                            Some(osc::encode_message(a, vec![OscType::Int(v)]).unwrap())
                        }
                        // LR assign (`{root}/mix/lr`, int 1/0): store
                        // verbatim; untouched strips read back the live
                        // rig defaults (channels/FX returns assigned,
                        // buses not).
                        (a, Some(OscType::Int(i))) if a.ends_with("/mix/lr") => {
                            worker.plane_ints.lock().unwrap().insert(a.to_string(), *i);
                            None
                        }
                        (a, None) if a.ends_with("/mix/lr") => {
                            let v = worker
                                .plane_ints
                                .lock()
                                .unwrap()
                                .get(a)
                                .copied()
                                .unwrap_or_else(|| if a.starts_with("/bus/") { 0 } else { 1 });
                            Some(osc::encode_message(a, vec![OscType::Int(v)]).unwrap())
                        }
                        // Automix container (`{root}/automix`, `(int,
                        // float)` = enable + weight): store both
                        // verbatim; untouched strips read back (0, 0.5)
                        // like the live rig.
                        (a, Some(_)) if a.ends_with("/automix") => {
                            if let [OscType::Int(on), OscType::Float(w)] = msg.args.as_slice() {
                                worker.plane_ints.lock().unwrap().insert(a.to_string(), *on);
                                worker
                                    .plane_floats
                                    .lock()
                                    .unwrap()
                                    .insert(a.to_string(), *w);
                            }
                            None
                        }
                        (a, None) if a.ends_with("/automix") => {
                            let on = worker
                                .plane_ints
                                .lock()
                                .unwrap()
                                .get(a)
                                .copied()
                                .unwrap_or(0);
                            let w = worker
                                .plane_floats
                                .lock()
                                .unwrap()
                                .get(a)
                                .copied()
                                .unwrap_or(0.5);
                            Some(
                                osc::encode_message(a, vec![OscType::Int(on), OscType::Float(w)])
                                    .unwrap(),
                            )
                        }
                        (a, None)
                            if a.ends_with("/mix/fader")
                                || a.starts_with("/dca/") && a.ends_with("/fader") =>
                        {
                            let v = worker
                                .faders
                                .lock()
                                .unwrap()
                                .get(a)
                                .copied()
                                .unwrap_or(0.75);
                            Some(osc::encode_message(a, vec![OscType::Float(v)]).unwrap())
                        }
                        (a, None)
                            if a.ends_with("/mix/on")
                                || a.starts_with("/dca/") && a.ends_with("/on") =>
                        {
                            let v = worker.ons.lock().unwrap().get(a).copied().unwrap_or(1);
                            Some(osc::encode_message(a, vec![OscType::Int(v)]).unwrap())
                        }
                        (a, None) if a.ends_with("/config/name") => {
                            let v = worker
                                .names
                                .lock()
                                .unwrap()
                                .get(a)
                                .cloned()
                                .unwrap_or_default();
                            Some(osc::encode_message(a, vec![OscType::String(v)]).unwrap())
                        }
                        // `/fx/N/type` reads back the stored effect (unset → 0).
                        (a, None) if a.starts_with("/fx/") && a.ends_with("/type") => {
                            let slot = a
                                .split('/')
                                .nth(2)
                                .and_then(|s| s.parse::<u8>().ok())
                                .unwrap_or(0);
                            let v = worker
                                .fx_types
                                .lock()
                                .unwrap()
                                .get(&slot)
                                .copied()
                                .unwrap_or(0);
                            Some(osc::encode_message(a, vec![OscType::Int(v)]).unwrap())
                        }
                        // Untouched FX params read back 0.0 float;
                        // stored pars answer verbatim (int stays int).
                        (a, None) if a.starts_with("/fx/") && a.contains("/par/") => {
                            let v = worker
                                .fx
                                .lock()
                                .unwrap()
                                .get(a)
                                .cloned()
                                .unwrap_or(OscType::Float(0.0));
                            Some(osc::encode_message(a, vec![v]).unwrap())
                        }
                        // Untouched headamp gains read back 0.5 wire
                        // (= 24 dB); untouched phantom reads back off.
                        (a, None) if a.starts_with("/headamp/") && a.ends_with("/gain") => {
                            let v = worker.gains.lock().unwrap().get(a).copied().unwrap_or(0.5);
                            Some(osc::encode_message(a, vec![OscType::Float(v)]).unwrap())
                        }
                        (a, None) if a.starts_with("/headamp/") && a.ends_with("/phantom") => {
                            let v = worker.phantoms.lock().unwrap().get(a).copied().unwrap_or(0);
                            Some(osc::encode_message(a, vec![OscType::Int(v)]).unwrap())
                        }
                        // Untouched input int leaves read back 0 (rtnsrc
                        // U01, invert/hpon off); untouched input floats
                        // read back 0.0 wire (rtntrim −18 dB, hpf 20 Hz).
                        (a, None)
                            if a.ends_with("/config/rtnsrc")
                                || a.ends_with("/config/insrc")
                                || a.ends_with("/preamp/rtnsw")
                                || a.ends_with("/preamp/invert")
                                || a.ends_with("/preamp/hpon") =>
                        {
                            let v = worker
                                .input_ints
                                .lock()
                                .unwrap()
                                .get(a)
                                .copied()
                                .unwrap_or_else(|| {
                                    if a.ends_with("/config/insrc") {
                                        a.strip_prefix("/ch/")
                                            .and_then(|s| s.split('/').next())
                                            .and_then(|s| s.parse::<i32>().ok())
                                            .unwrap_or(1)
                                            - 1
                                    } else {
                                        0
                                    }
                                });
                            Some(osc::encode_message(a, vec![OscType::Int(v)]).unwrap())
                        }
                        (a, None)
                            if a.ends_with("/preamp/rtntrim") || a.ends_with("/preamp/hpf") =>
                        {
                            let v = worker
                                .input_floats
                                .lock()
                                .unwrap()
                                .get(a)
                                .copied()
                                .unwrap_or(0.0);
                            Some(osc::encode_message(a, vec![OscType::Float(v)]).unwrap())
                        }
                        // Untouched param-plane switches read back 0
                        // (eq/gate/dyn off, first filter type).
                        (a, None)
                            if a.ends_with("/eq/on")
                                || a.ends_with("/gate/on")
                                || a.ends_with("/dyn/on")
                                || (a.contains("/eq/") && a.ends_with("/type")) =>
                        {
                            let v = worker
                                .plane_ints
                                .lock()
                                .unwrap()
                                .get(a)
                                .copied()
                                .unwrap_or(0);
                            Some(osc::encode_message(a, vec![OscType::Int(v)]).unwrap())
                        }
                        // Untouched param-plane floats: eq `g`/`f`/`q`
                        // read back mid-wire 0.5, pan centre 0.5, send
                        // levels the −90 dB floor (wire 0.0).
                        (a, None)
                            if (a.contains("/eq/")
                                && (a.ends_with("/g")
                                    || a.ends_with("/f")
                                    || a.ends_with("/q")))
                                || a.ends_with("/mix/pan")
                                || (a.contains("/mix/") && a.ends_with("/level")) =>
                        {
                            let fallback = if a.ends_with("/level") { 0.0 } else { 0.5 };
                            let v = worker
                                .plane_floats
                                .lock()
                                .unwrap()
                                .get(a)
                                .copied()
                                .unwrap_or(fallback);
                            Some(osc::encode_message(a, vec![OscType::Float(v)]).unwrap())
                        }
                        // v2 int leaves (firmware-truth 2026-10-18): strip look
                        // (`/config/color`; `/config/icon` and
                        // `/config/link` have NO desk leaf — silent —
                        // but the fake answers them so the retained
                        // driver arms stay loopback-tested), gate/dyn
                        // int leaves (incl. the `filter/` sub-leaves and
                        // `dyn/ratio` step index), send tap + grpon and
                        // per-send on/off. NOTE the send-on conflict:
                        // this fake answers `/ch/NN/mix/MM/on` while the
                        // live desk is silent (T7); `sendN_on` is out of
                        // caps. Guards list int leaves EXACTLY: a broad
                        // `contains("/gate/")` here would shadow the
                        // float leaves' queries below (same `None`
                        // shape). Untouched reads answer 0.
                        (a, Some(OscType::Int(i)))
                            if a.ends_with("/config/color")
                                || a.ends_with("/config/icon")
                                || a.ends_with("/config/link")
                                || a.starts_with("/config/chlink/")
                                || a.ends_with("/gate/mode")
                                || a.ends_with("/gate/keysrc")
                                || a.ends_with("/gate/filter/on")
                                || a.ends_with("/gate/filter/type")
                                || a.ends_with("/dyn/ratio")
                                || a.ends_with("/dyn/keysrc")
                                || a.ends_with("/dyn/filter/on")
                                || a.ends_with("/dyn/filter/type")
                                || a.ends_with("/dyn/mode")
                                || a.ends_with("/dyn/det")
                                || a.ends_with("/dyn/env")
                                || a.ends_with("/dyn/auto")
                                || (a.contains("/mix/") && a.ends_with("/tap"))
                                || (a.contains("/mix/") && a.ends_with("/grpon"))
                                || (a.contains("/mix/")
                                    && a.ends_with("/on")
                                    && !a.ends_with("/mix/on")) =>
                        {
                            worker.plane_ints.lock().unwrap().insert(a.to_string(), *i);
                            None
                        }
                        // v2b: LR assign (`{root}/mix/lr`, ch/bus/rtn —
                        // the `/lr` root has no leaf and the driver
                        // never asks): verbatim ints, untouched reads
                        // default per rig (ch/rtn 1, bus 0).
                        (a, Some(OscType::Int(i))) if a.ends_with("/mix/lr") => {
                            worker.plane_ints.lock().unwrap().insert(a.to_string(), *i);
                            None
                        }
                        // Strip insert (`{root}/insert/on|sel`, ch/bus/lr
                        // — rig-verified 2026-10-20; the driver never
                        // asks on rtn/fxsend/dca roots, which stay
                        // silent like the live desk): verbatim ints.
                        (a, Some(OscType::Int(i)))
                            if a.ends_with("/insert/on") || a.ends_with("/insert/sel") =>
                        {
                            worker.plane_ints.lock().unwrap().insert(a.to_string(), *i);
                            None
                        }
                        // v2b: solo monitoring globals (`/config/solo/*`):
                        // verbatim ints/float, untouched reads default
                        // per rig (dim 0, mute 0, level 0.75, source 3).
                        (a, Some(OscType::Int(i)))
                            if a == crate::solo::SOLO_DIM
                                || a == crate::solo::SOLO_MUTE
                                || a == crate::solo::SOLO_SOURCE =>
                        {
                            worker.plane_ints.lock().unwrap().insert(a.to_string(), *i);
                            None
                        }
                        (a, Some(OscType::Float(f))) if a == crate::solo::SOLO_LEVEL => {
                            worker
                                .plane_floats
                                .lock()
                                .unwrap()
                                .insert(a.to_string(), *f);
                            None
                        }
                        // v2b: FX slot insert (`/fx/N/insert`): verbatim
                        // ints, untouched reads default 0 (bypassed).
                        // (Strip insert *containers* like `/ch/01/insert`
                        // stay silent — the driver addresses only the
                        // `/insert/on|sel` leaves.)
                        (a, Some(OscType::Int(i)))
                            if a.starts_with("/fx/") && a.ends_with("/insert") =>
                        {
                            worker.plane_ints.lock().unwrap().insert(a.to_string(), *i);
                            None
                        }
                        // v2 float leaves: gate/dyn block (minus the int
                        // leaves above, which match first), key-filter
                        // corners (`filter/f`, log wire), per-send pan
                        // (`/mix/MM/pan`; channel pan keeps its own arm)
                        // and automix weight (`/automix/weight`, ch
                        // only). Untouched gate/dyn reads answer 0.0
                        // wire, send pan centre 0.5, automix 0.5.
                        (a, Some(OscType::Float(f)))
                            if (a.contains("/gate/")
                                && !a.ends_with("/gate/mode")
                                && !a.ends_with("/gate/keysrc")
                                && !a.ends_with("/gate/filter/on")
                                && !a.ends_with("/gate/filter/type"))
                                || (a.contains("/dyn/")
                                    && !a.ends_with("/dyn/mode")
                                    && !a.ends_with("/dyn/det")
                                    && !a.ends_with("/dyn/env")
                                    && !a.ends_with("/dyn/auto")
                                    && !a.ends_with("/dyn/ratio")
                                    && !a.ends_with("/dyn/keysrc")
                                    && !a.ends_with("/dyn/filter/on")
                                    && !a.ends_with("/dyn/filter/type"))
                                || (a.contains("/mix/")
                                    && a.ends_with("/pan")
                                    && !a.ends_with("/mix/pan"))
                                || a.ends_with("/automix/weight") =>
                        {
                            worker
                                .plane_floats
                                .lock()
                                .unwrap()
                                .insert(a.to_string(), *f);
                            None
                        }
                        (a, None)
                            if a.ends_with("/config/color")
                                || a.ends_with("/config/icon")
                                || a.ends_with("/config/link")
                                || a.starts_with("/config/chlink/")
                                || a.ends_with("/gate/mode")
                                || a.ends_with("/gate/keysrc")
                                || a.ends_with("/gate/filter/on")
                                || a.ends_with("/gate/filter/type")
                                || a.ends_with("/dyn/ratio")
                                || a.ends_with("/dyn/keysrc")
                                || a.ends_with("/dyn/filter/on")
                                || a.ends_with("/dyn/filter/type")
                                || a.ends_with("/dyn/mode")
                                || a.ends_with("/dyn/det")
                                || a.ends_with("/dyn/env")
                                || a.ends_with("/dyn/auto")
                                || (a.contains("/mix/") && a.ends_with("/tap"))
                                || (a.contains("/mix/") && a.ends_with("/grpon"))
                                || (a.contains("/mix/")
                                    && a.ends_with("/on")
                                    && !a.ends_with("/mix/on")) =>
                        {
                            let v = worker
                                .plane_ints
                                .lock()
                                .unwrap()
                                .get(a)
                                .copied()
                                .unwrap_or(0);
                            Some(osc::encode_message(a, vec![OscType::Int(v)]).unwrap())
                        }
                        // v2b: LR assign reads back the stored int, else
                        // the rig default (ch/rtn 1, bus 0).
                        (a, None) if a.ends_with("/mix/lr") => {
                            let v = worker
                                .plane_ints
                                .lock()
                                .unwrap()
                                .get(a)
                                .copied()
                                .unwrap_or_else(|| if a.starts_with("/bus/") { 0 } else { 1 });
                            Some(osc::encode_message(a, vec![OscType::Int(v)]).unwrap())
                        }
                        // v2b: solo globals read back stored values, else
                        // rig defaults (dim 0, mute 0, source 3).
                        (a, None)
                            if a == crate::solo::SOLO_DIM
                                || a == crate::solo::SOLO_MUTE
                                || a == crate::solo::SOLO_SOURCE =>
                        {
                            let fallback = if a == crate::solo::SOLO_SOURCE { 3 } else { 0 };
                            let v = worker
                                .plane_ints
                                .lock()
                                .unwrap()
                                .get(a)
                                .copied()
                                .unwrap_or(fallback);
                            Some(osc::encode_message(a, vec![OscType::Int(v)]).unwrap())
                        }
                        // v2b: solo level reads back stored, else rig 0.75.
                        (a, None) if a == crate::solo::SOLO_LEVEL => {
                            let v = worker
                                .plane_floats
                                .lock()
                                .unwrap()
                                .get(a)
                                .copied()
                                .unwrap_or(0.75);
                            Some(osc::encode_message(a, vec![OscType::Float(v)]).unwrap())
                        }
                        // v2b: FX slot insert reads back stored, else 0.
                        (a, None) if a.starts_with("/fx/") && a.ends_with("/insert") => {
                            let v = worker
                                .plane_ints
                                .lock()
                                .unwrap()
                                .get(a)
                                .copied()
                                .unwrap_or(0);
                            Some(osc::encode_message(a, vec![OscType::Int(v)]).unwrap())
                        }
                        // Strip insert reads back stored, else 0
                        // (bypassed, nothing selected — matches the rig
                        // defaults on untouched strips).
                        (a, None) if a.ends_with("/insert/on") || a.ends_with("/insert/sel") => {
                            let v = worker
                                .plane_ints
                                .lock()
                                .unwrap()
                                .get(a)
                                .copied()
                                .unwrap_or(0);
                            Some(osc::encode_message(a, vec![OscType::Int(v)]).unwrap())
                        }
                        (a, None)
                            if (a.contains("/gate/")
                                && !a.ends_with("/gate/mode")
                                && !a.ends_with("/gate/keysrc")
                                && !a.ends_with("/gate/filter/on")
                                && !a.ends_with("/gate/filter/type"))
                                || (a.contains("/dyn/")
                                    && !a.ends_with("/dyn/mode")
                                    && !a.ends_with("/dyn/det")
                                    && !a.ends_with("/dyn/env")
                                    && !a.ends_with("/dyn/auto")
                                    && !a.ends_with("/dyn/ratio")
                                    && !a.ends_with("/dyn/keysrc")
                                    && !a.ends_with("/dyn/filter/on")
                                    && !a.ends_with("/dyn/filter/type"))
                                || (a.contains("/mix/")
                                    && a.ends_with("/pan")
                                    && !a.ends_with("/mix/pan")) =>
                        {
                            let fallback = if a.ends_with("/pan") { 0.5 } else { 0.0 };
                            let v = worker
                                .plane_floats
                                .lock()
                                .unwrap()
                                .get(a)
                                .copied()
                                .unwrap_or(fallback);
                            Some(osc::encode_message(a, vec![OscType::Float(v)]).unwrap())
                        }
                        // v2b: automix weight reads back stored, else 0.5.
                        (a, None) if a.ends_with("/automix/weight") => {
                            let v = worker
                                .plane_floats
                                .lock()
                                .unwrap()
                                .get(a)
                                .copied()
                                .unwrap_or(0.5);
                            Some(osc::encode_message(a, vec![OscType::Float(v)]).unwrap())
                        }
                        // Mute-group masters (`/config/mute/1..4`): verbatim
                        // ints, untouched reads answer 0. Groups 5+
                        // stay silent like the live desk (MUTE_GROUPS).
                        (a, Some(OscType::Int(i)))
                            if a.starts_with("/config/mute/")
                                && matches!(a["/config/mute/".len()..].parse::<u8>(), Ok(n) if (1..=addrs::MUTE_GROUPS).contains(&n)) =>
                        {
                            worker.plane_ints.lock().unwrap().insert(a.to_string(), *i);
                            None
                        }
                        (a, None)
                            if a.starts_with("/config/mute/")
                                && matches!(a["/config/mute/".len()..].parse::<u8>(), Ok(n) if (1..=addrs::MUTE_GROUPS).contains(&n)) =>
                        {
                            let v = worker
                                .plane_ints
                                .lock()
                                .unwrap()
                                .get(a)
                                .copied()
                                .unwrap_or(0);
                            Some(osc::encode_message(a, vec![OscType::Int(v)]).unwrap())
                        }
                        ("/info", _) => None,
                        // Solo switches (`/-stat/solosw/NN`, 1-based):
                        // fire-and-forget sets store verbatim; the bank
                        // query reads back 54 ints (like the live desk,
                        // all 0 when nothing is soloed).
                        (a, Some(OscType::Int(i)))
                            if a.starts_with("/-stat/solosw/")
                                && matches!(a["/-stat/solosw/".len()..].parse::<u8>(), Ok(n) if (1..=54).contains(&n)) =>
                        {
                            worker.plane_ints.lock().unwrap().insert(a.to_string(), *i);
                            None
                        }
                        (a, None) if a == "/-stat/solosw" => {
                            let stored = worker.plane_ints.lock().unwrap();
                            let args: Vec<OscType> = (1..=54u8)
                                .map(|n| {
                                    OscType::Int(
                                        stored
                                            .get(&format!("/-stat/solosw/{n:02}"))
                                            .copied()
                                            .unwrap_or(0),
                                    )
                                })
                                .collect();
                            Some(osc::encode_message(a, args).unwrap())
                        }
                        // Clear-solo action: fire-and-forget sets are
                        // ACKed by silence; queries read back int 0
                        // (nothing soloed), like the live desk.
                        (a, Some(OscType::Int(_))) if a == addrs::CLEAR_SOLO => None,
                        (a, None) if a == addrs::CLEAR_SOLO => {
                            Some(osc::encode_message(a, vec![OscType::Int(0)]).unwrap())
                        }
                        _ => None,
                    };
                    if let Some(bytes) = reply {
                        let _ = worker.socket.send_to(&bytes, peer).await;
                    }
                }
            }
        });
        (addr, fake)
    }

    /// Fire an unsolicited datagram at the most recently seen peer,
    /// simulating a desk-side change broadcast to `/xremote`
    /// subscribers (no query asked for it — the driver's ingest task
    /// must route it to the update channel via `parse_inbound`).
    /// No-op until any datagram has arrived (the peer is unknown).
    pub(super) async fn broadcast(&self, addr: &str, args: Vec<OscType>) {
        let peer = *self.last_peer.lock().unwrap();
        if let Some(peer) = peer {
            let bytes = osc::encode_message(addr, args).unwrap();
            let _ = self.socket.send_to(&bytes, peer).await;
        }
    }
}

pub(super) async fn connected_driver() -> (XAirDriver, Arc<FakeMixer>) {
    let (addr, fake) = FakeMixer::spawn().await;
    // Bypass connect()'s best-effort /info ping timing by building the
    // session directly against the fake, then installing it.
    let peer: SocketAddr = addr;
    let sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    sock.connect(peer).await.unwrap();
    let session = OscSession::from_connected_socket(sock, Some(KeepaliveConfig::xremote()));
    let driver = XAirDriver::new();
    *driver.session.lock().await = Some(std::sync::Arc::new(session));
    // Match the connected lifecycle state expected by checked dispatch.
    driver
        .connected
        .store(true, std::sync::atomic::Ordering::Release);
    // Drain the fake's potential early datagrams by a tiny settle.
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    (driver, fake)
}
