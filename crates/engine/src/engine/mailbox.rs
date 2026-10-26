//! Client command handle and reliable/latest-only receiving mailbox.

use super::*;

/// A client's handle on the [`Engine`]: its id, its live event mailbox,
/// and typed/JSON command entry points.
///
/// `events` first yields the attach snapshot (`you`, `state`, `shows`,
/// `clients`, `locks`, `caps`, `devices`, `link`, `info`?, `scenes`,
/// `fx`, `routing`, then one `params` frame per cached channel,
/// `native_snapshots`, then `local_library_status` in device mode only)
/// in that order, then every live broadcast. Unicast frames (`you` replies,
/// `list_shows` replies, denials, `recalled` reports, command errors) arrive
/// only in the offending/sender's mailbox.
///
/// Dropping the handle detaches the client (best-effort synchronous;
/// prefer an explicit [`detach`](ClientHandle::detach) so the roster update
/// and lock auto-release are guaranteed to run).
/// A client's live event mailbox: the receiving half of the engine
/// fan-out with latest-only realtime coalescing.
///
/// `recv`/`try_recv` mirror [`mpsc::UnboundedReceiver`], so adapters drain
/// exactly as before. Yielding a realtime frame clears that mailbox's
/// in-flight flag and promotes every coalesced slot back into the channel
/// (meters first, then per-key families in key order), so an actively
/// draining client observes every stream's latest frame and loses nothing
/// but superseded same-key frames. `len` covers both the queued channel
/// backlog and the coalesced slots (what an edge fan-out still owes this
/// client); control frames are always counted, never coalesced.
pub struct ClientMailbox {
    pub(super) rx: mpsc::UnboundedReceiver<SharedEvent>,
    /// Promotes coalesced slots back into `rx` as the client drains (an
    /// extra sender on the same channel: unbounded sends never block).
    pub(super) tx: mpsc::UnboundedSender<SharedEvent>,
    pub(super) rt: Arc<std::sync::Mutex<RtSlots>>,
    pub(super) queued: Arc<AtomicUsize>,
}

impl ClientMailbox {
    pub(super) fn new(
        rx: mpsc::UnboundedReceiver<SharedEvent>,
        tx: mpsc::UnboundedSender<SharedEvent>,
        rt: Arc<std::sync::Mutex<RtSlots>>,
        queued: Arc<AtomicUsize>,
    ) -> Self {
        Self { rx, tx, rt, queued }
    }

    /// Next frame, waiting if empty (returns `None` once the engine has
    /// detached and the queue has drained).
    pub async fn recv(&mut self) -> Option<SharedEvent> {
        let frame = self.rx.recv().await?;
        self.after_yield(&frame);
        Some(frame)
    }

    /// Next frame without waiting.
    pub fn try_recv(&mut self) -> Result<SharedEvent, mpsc::error::TryRecvError> {
        let frame = self.rx.try_recv()?;
        self.after_yield(&frame);
        Ok(frame)
    }

    /// Frames this client is still owed: queued channel backlog plus
    /// coalesced realtime slots.
    pub fn len(&self) -> usize {
        self.queued.load(Ordering::Relaxed) + self.rt.lock().map(|rt| rt.occupancy()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Account for one yielded frame; on realtime frames, clear in-flight
    /// and promote the coalesced latest frames behind it.
    pub(super) fn after_yield(&mut self, frame: &SharedEvent) {
        self.queued.fetch_sub(1, Ordering::Relaxed);
        if rt_slot_key(frame).is_none() {
            return;
        }
        let pending = match self.rt.lock() {
            Ok(mut rt) => {
                rt.inflight = false;
                rt.take_all()
            }
            Err(_) => Vec::new(),
        };
        if pending.is_empty() {
            return;
        }
        let mut promoted = false;
        for frame in pending {
            if self.tx.send(frame).is_ok() {
                self.queued.fetch_add(1, Ordering::Relaxed);
                promoted = true;
            }
        }
        if promoted {
            if let Ok(mut rt) = self.rt.lock() {
                rt.inflight = true;
            }
        }
    }
}

pub struct ClientHandle {
    /// Server-side id (`client-N`); matches later `by` tags.
    pub id: String,
    pub(super) engine: Arc<Engine>,
    /// Live event mailbox: an unbounded channel plus per-mailbox
    /// latest-only realtime slots (see [`RtSlots`]). Slow clients never
    /// apply backpressure, but they no longer accumulate meter-class frames
    /// without bound: while a realtime frame sits queued, newer `meters` /
    /// `taps` / `spectrum` / `audio_spectrum` / `gr` frames supersede the
    /// older same-key frame in the slots instead of queueing, so a stalled
    /// client holds at most one queued realtime frame plus one latest frame
    /// per realtime stream key. Every other frame (state changes, params,
    /// roster, locks, errors, …) queues reliably in order and is never
    /// dropped or coalesced. Each frame carries its wire JSON serialized
    /// once at broadcast time and shared across clients via `Arc` (see
    /// [`SharedEvent`]); the sweeper prunes detached mailboxes.
    pub events: ClientMailbox,
}

impl ClientHandle {
    /// Send one typed command. Domain errors (bad args, denied writes,
    /// driver failures) arrive as unicast [`Event::Error`] in `events`;
    /// `Err` only means the client is already detached.
    ///
    /// Delivery order mirrors the relay socket: the unicast reply (or
    /// error) lands in the sender's mailbox *before* the broadcasts the
    /// command triggered, so a client waiting for its reply never consumes
    /// its own follow-up frames first.
    pub async fn send(&self, cmd: Cmd) -> anyhow::Result<()> {
        if !self.engine.has_client(&self.id) {
            anyhow::bail!("client {} is detached", self.id);
        }
        self.engine.apply_cmd(&self.id, cmd).await;
        Ok(())
    }

    /// Send one raw JSON command line (what adapters get off the socket).
    /// Never fails: parse failures also arrive as unicast `error` events,
    /// so bad input never closes the connection.
    pub async fn send_json(&self, text: &str) -> anyhow::Result<()> {
        if !self.engine.has_client(&self.id) {
            anyhow::bail!("client {} is detached", self.id);
        }
        self.engine.handle_json(&self.id, text).await;
        Ok(())
    }

    /// Explicit detach (roster update + lock auto-release).
    pub async fn detach(&self) {
        self.engine.remove_client(&self.id);
    }
}

impl Drop for ClientHandle {
    fn drop(&mut self) {
        self.engine.remove_client(&self.id);
    }
}

impl Engine {
    pub(super) fn has_client(&self, id: &str) -> bool {
        self.clients.read().unwrap().contains_key(id)
    }
}

/// Realtime stream key: one slot per meter-class family that a newer
/// frame supersedes. `Meters` is a single global frame; `Taps` fan out per
/// negotiated tap id; `Spectrum`/`AudioSpectrum`/`Gr` fan out per channel
/// (`AudioSpectrum` without a strip shares the `u16::MAX` key, mirroring
/// the serve-adapter outbox convention).
#[derive(Clone, PartialEq, Eq, Hash)]
pub(super) enum RtSlot {
    Meters,
    Tap(String),
    Spectrum(u16),
    Audio(u16),
    Gr(u16),
    Dynamics,
}

/// Classify a frame: `Some` key for supersede-by-newer meter-class frames
/// (`meters`, per-tap `taps`, envelope `spectrum`, provider
/// `audio_spectrum`, per-channel `gr`), `None` for everything else.
/// Control frames (state changes, params, roster, locks, errors, link,
/// scenes, …) are never coalescible: they queue reliably in order.
pub(super) fn rt_slot_key(event: &Event) -> Option<RtSlot> {
    match event {
        Event::Meters { .. } => Some(RtSlot::Meters),
        Event::Taps { tap, .. } => Some(RtSlot::Tap(tap.clone())),
        Event::Spectrum { ch, .. } => Some(RtSlot::Spectrum(*ch)),
        Event::AudioSpectrum { ch, .. } => Some(RtSlot::Audio(ch.unwrap_or(u16::MAX))),
        Event::Gr { ch, .. } => Some(RtSlot::Gr(*ch)),
        Event::DynamicsMeters { .. } => Some(RtSlot::Dynamics),
        _ => None,
    }
}

/// Per-mailbox latest-only realtime slots.
///
/// While [`inflight`](RtSlots::inflight) (a realtime frame sits queued in
/// the channel, unconsumed), incoming meter-class frames replace the slot
/// for their stream key instead of queueing: at most one latest frame per
/// key waits, however fast the ticker runs. Consuming a realtime frame
/// clears `inflight` and the [`ClientMailbox`] promotes every slot back
/// into the channel, so an active client loses nothing but superseded
/// (older same-key) frames, and a stalled client's realtime memory stays
/// flat: one queued frame plus one slot set.
#[derive(Default)]
pub(super) struct RtSlots {
    /// A realtime frame is currently queued in the channel.
    pub(super) inflight: bool,
    pub(super) meters: Option<SharedEvent>,
    pub(super) dynamics: Option<SharedEvent>,
    pub(super) taps: HashMap<String, SharedEvent>,
    pub(super) spectrum: HashMap<u16, SharedEvent>,
    pub(super) audio: HashMap<u16, SharedEvent>,
    pub(super) gr: HashMap<u16, SharedEvent>,
}

impl RtSlots {
    /// Replace the slot for `key` with a newer frame (the older same-key
    /// frame is superseded and dropped).
    pub(super) fn store(&mut self, key: RtSlot, frame: SharedEvent) {
        match key {
            RtSlot::Dynamics => {
                self.dynamics = Some(frame);
            }
            RtSlot::Meters => {
                self.meters = Some(frame);
            }
            RtSlot::Tap(tap) => {
                self.taps.insert(tap, frame);
            }
            RtSlot::Spectrum(ch) => {
                self.spectrum.insert(ch, frame);
            }
            RtSlot::Audio(key) => {
                self.audio.insert(key, frame);
            }
            RtSlot::Gr(ch) => {
                self.gr.insert(ch, frame);
            }
        }
    }

    /// Drain every slot, meters first then per-key families in key order
    /// (deterministic promotion order for tests and adapters).
    pub(super) fn take_all(&mut self) -> Vec<SharedEvent> {
        let mut out = Vec::new();
        if let Some(frame) = self.meters.take() {
            out.push(frame);
        }
        if let Some(frame) = self.dynamics.take() {
            out.push(frame);
        }
        let mut taps: Vec<_> = std::mem::take(&mut self.taps).into_iter().collect();
        taps.sort_by(|a, b| a.0.cmp(&b.0));
        out.extend(taps.into_iter().map(|(_, frame)| frame));
        let mut spectrum: Vec<_> = std::mem::take(&mut self.spectrum).into_iter().collect();
        spectrum.sort_by_key(|(ch, _)| *ch);
        out.extend(spectrum.into_iter().map(|(_, frame)| frame));
        let mut audio: Vec<_> = std::mem::take(&mut self.audio).into_iter().collect();
        audio.sort_by_key(|(ch, _)| *ch);
        out.extend(audio.into_iter().map(|(_, frame)| frame));
        let mut gr: Vec<_> = std::mem::take(&mut self.gr).into_iter().collect();
        gr.sort_by_key(|(ch, _)| *ch);
        out.extend(gr.into_iter().map(|(_, frame)| frame));
        out
    }

    /// Frames currently held outside the channel.
    pub(super) fn occupancy(&self) -> usize {
        usize::from(self.meters.is_some())
            + usize::from(self.dynamics.is_some())
            + self.taps.len()
            + self.spectrum.len()
            + self.audio.len()
            + self.gr.len()
    }
}

/// One connected client (presence tracking + mailbox + role).
///
/// The mailbox is a two-level fan-out: an unbounded channel carrying every
/// frame in order, plus latest-only realtime slots (see [`RtSlots`]) shared
/// with the client's [`ClientMailbox`]. Meter-class frames collapse per
/// stream key while a frame is in flight, so a stalled client holds at most
/// one queued realtime frame plus one latest frame per stream key; every
/// other frame queues reliably and is never dropped or coalesced.
pub(super) struct ClientEntry {
    pub(super) mailbox: mpsc::UnboundedSender<SharedEvent>,
    /// Latest-only realtime slots for this mailbox (sender side writes,
    /// the [`ClientMailbox`] promotes them back into the channel as the
    /// client drains). Shared, never held across an `.await`.
    pub(super) rt: Arc<std::sync::Mutex<RtSlots>>,
    /// Estimated channel backlog (every channel send increments, every
    /// yield decrements). Backs [`ClientMailbox::len`] for edge stats.
    pub(super) queued: Arc<AtomicUsize>,
    #[allow(dead_code)]
    pub(super) label: String,
    #[allow(dead_code)]
    pub(super) connected_at: Instant,
    pub(super) last_write: Option<Instant>,
    pub(super) policy: Policy,
    /// MR1 realtime preference: when true the adapter encodes `Meters`/`Gr`
    /// as WS BINARY frames (via [`Event::to_binary`](crate::protocol::Event::to_binary))
    /// instead of JSON text. Set by [`Cmd::Hello`](crate::protocol::Cmd::Hello);
    /// default off so old clients keep byte-identical JSON.
    pub(super) binary_meters: bool,
    /// R2 meter-tap preference: when true the engine fans per-strip tap
    /// frames (`in`/`post`/`gr_gate`/`gr_comp`) to this client (JSON, or
    /// WS BINARY type 3 in the adapter). Set by `hello {"taps":true}`;
    /// default off so old clients see no new traffic.
    pub(super) taps: bool,
    /// Whether this client already holds the full `caps` catalogue. Attach
    /// queues the slim caps (essentials lane only, see
    /// [`Engine::caps_slim_message`]); the first `hello` completes it with
    /// a unicast full `caps` and flips this. Later hellos are pure re-acks.
    pub(super) caps_full: bool,
}
