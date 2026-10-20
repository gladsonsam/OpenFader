// Live relay singleton: the real socket behind the connection screen.
//
// The fixture store (uiStore) owns the design's display model; this module
// owns the real-time link to the relay (default ws://<host>/ws, sim on
// :8080). `connect(host?, driver?, addr?)` opens a WsTransport +
// EngineWorld and runs the hello handshake, flipping `phase` to 'online'
// once caps arrive. With no driver it attaches WITHOUT switching the
// relay; a `connect` command goes out only for an explicitly chosen
// driver. Failures (refused host, timeout, drop) surface as `phase` +
// `message` so the connection screen shows them instead of silently
// flipping a fixture flag.
// The relay host persists in localStorage; `?server=` overrides it.
//
// Mixer/channel screens read live meters through `liveLevelDb(fixtureId)`
// (dB, like MeterSim.levelOf) and prefer them when online; fader/mute writes
// still edit the fixture model (engine write-through is wave 3).

import type { EngineWorld } from '../world/engineWorld/engineWorld.ts';
import type { Transport } from '../world/transport.ts';
import { isTauriRuntime } from '../world/tauriEnv.ts';
import {
  CONNECT_TIMEOUT_MS,
  DEFAULT_RELAY_HOST,
  SCAN_TIMEOUT_MS,
  isLoopbackHost,
  loadRecentDesks,
  loadRelayHost,
  pageHostSuggestion,
  relayHostFromSearch,
  relayHostPart,
  relayPortPart,
  saveRecentDesk,
  saveRelayAuto,
  saveRelayHost,
  linearToDb,
  relayUrlFromHost,
  type LivePhase,
} from './live.ts';
import { bridgeStrip } from './engineBridge.ts';

export type { LivePhase };

/** Explicit offline mode persists across reloads (set by the connection
 * screen's Work-offline action). While set, the app never auto-links the
 * relay: offline means zero network. An explicit user connect (relay card
 * or `?server=`) clears it — that intent wins over the stored flag. */
const OFFLINE_LS_KEY = 'openmix-offline';

function loadOfflineMode(): boolean {
  try {
    return typeof localStorage !== 'undefined' && localStorage.getItem(OFFLINE_LS_KEY) === '1';
  } catch {
    return false;
  }
}

function clearOfflineMode(): void {
  try {
    localStorage?.removeItem(OFFLINE_LS_KEY);
  } catch {
    /* ignore */
  }
}

class LiveRelay {
  phase: LivePhase = $state('idle');
  message = $state('Not connected');
  host: string = $state(loadRelayHost());
  online = $state(false);
  capsReady = $state(false);
  latencyMs: number | null = $state(null);
  /** Client-side scan-busy flag: the engine emits no scan-started event
   *  (a `devices` frame only lands when the ~3 s sweep finishes), so the
   *  connection screen tracks the press itself. Set by noteScanRequested,
   *  cleared on the next devices frame (scannedAt moves) or after
   *  SCAN_TIMEOUT_MS — never a fake device, just the button + list state. */
  scanning = $state(false);
  /** Honest scan failure hint (scan pressed with no link, or the sweep
   *  timed out). Null = no failure to report. */
  scanError: string | null = $state(null);
  /** Bumped on every engine ingest so screens re-read meters/version. */
  version = $state(0);

  private transport: Transport | null = null;
  private world: EngineWorld | null = null;
  private unsubs: Array<() => void> = [];
  private timer: ReturnType<typeof setTimeout> | null = null;
  private scanTimer: ReturnType<typeof setTimeout> | null = null;
  private scanBaseline = 0;
  private helloAt = 0;

  /** True inside the Tauri shell (desktop/mobile): the engine is embedded
   * in the app process and reached over IPC — no relay host involved. */
  get embedded(): boolean {
    return isTauriRuntime();
  }

  get url(): string {
    return this.embedded ? 'tauri://engine' : relayUrlFromHost(this.host);
  }

  get engine(): EngineWorld | null {
    return this.world;
  }

  get deskConnected(): boolean {
    void this.version;
    const w = this.world;
    return this.phase === 'online' && !!w?.capsReady && w.driver !== 'sim' && w.link?.state === 'connected';
  }

  get demo(): boolean {
    void this.version;
    return this.phase === 'online' && !!this.world?.capsReady && this.world.driver === 'sim';
  }

  /** EngineWorld observers (the fixture sync): called with the world on
   * every world bump, plus immediately when the world already exists.
   * Survives reconnects — each new world fans out to the same set. */
  private worldSubs = new Set<(w: EngineWorld) => void>();

  observeWorld(cb: (w: EngineWorld) => void): () => void {
    this.worldSubs.add(cb);
    const w = this.world;
    if (w) {
      try {
        cb(w);
      } catch {
        /* ignore: observer-local failure */
      }
    }
    return () => {
      this.worldSubs.delete(cb);
    };
  }

  /** Apply `?server=` once at boot (browser only — ignored when embedded:
   * the Tauri shell always links the in-process engine, never a relay). */
  initFromUrl(search: string): void {
    if (this.embedded) {
      if (typeof window !== 'undefined' && !loadOfflineMode() && this.phase === 'idle') this.connect();
      return;
    }
    const q = new URLSearchParams(search.startsWith('?') ? search : '?' + search);
    const isFixture = q.get('fixture') === '1' || q.get('fixture') === 'true';
    const { host, fromUrl } = relayHostFromSearch(search, this.host);
    if (fromUrl) {
      this.disconnect();
      this.host = host;
      saveRelayHost(host);
      // An explicit ?server= is an instruction to link up, not just to
      // remember the host (world-debug and phone-on-LAN flows).
      if (!isFixture) this.connect();
      return;
    }
    if (typeof window !== 'undefined' && !isFixture) {
      // No ?server=: when the page itself is served over LAN, default the
      // relay to the page's host (same machine usually serves both): a
      // phone on LAN needs no typing. Loopback pages keep the localhost
      // default — 127.0.0.1 is only correct on the relay's own machine.
      try {
        const hn = window.location.hostname;
        if (hn && !isLoopbackHost(hn) && isLoopbackHost(relayHostPart(this.host))) {
          const h = hn + ':' + relayPortPart(this.host);
          if (h !== this.host) {
            this.disconnect();
            this.host = h;
            saveRelayHost(h);
          }
        }
      } catch {
        /* ignore */
      }
      // Native feel: the frontend<->backend link is automatic — the app
      // always boots attached with zero user action. Fixture/diff pages
      // never auto-link (deterministic screenshots). Explicit offline mode
      // suppresses the link (zero network).
      if (!loadOfflineMode() && this.phase === 'idle') this.connect();
    }
  }

  setHost(host: string): void {
    const h = host.trim() || DEFAULT_RELAY_HOST;
    if (h === this.host) return;
    this.disconnect();
    this.host = h;
    saveRelayHost(h);
  }

  /** This page's hostname ('' outside a browser). */
  pageHostname(): string {
    try {
      return typeof window === 'undefined' ? '' : window.location.hostname || '';
    } catch {
      return '';
    }
  }

  /**
   * One-tap relay fix for remote clients: `<page-host>:<relay-port>` when
   * the page is LAN-served and the configured host isn't already it.
   * Null on localhost pages (the localhost default stays) — and never
   * 127.0.0.1 for a remote client.
   */
  suggestedHost(): string | null {
    return pageHostSuggestion(this.host, this.pageHostname());
  }

  /** Apply the one-tap fix: point at this page's host and link up. */
  usePageHost(): boolean {
    const s = this.suggestedHost();
    if (!s) return false;
    this.connect(s);
    return true;
  }

  /** Recent manual-desk hosts (retained for the engine probe path). */
  recentDesks(): string[] {
    return loadRecentDesks();
  }

  rememberDesk(host: string): string[] {
    return saveRecentDesk(host);
  }

  /**
   * Track one Scan press client-side (`sent` = whether the command actually
   * reached an engine — false when there is no link to send on). The busy
   * flag clears on the next devices frame (see the world subscriber above)
   * or after SCAN_TIMEOUT_MS, which reports an honest timeout hint.
   */
  noteScanRequested(sent: boolean): void {
    this.clearScanTimer();
    if (!sent) {
      this.scanning = false;
      this.scanError = 'Scan needs the relay link — retry once online.';
      return;
    }
    this.scanError = null;
    this.scanning = true;
    this.scanBaseline = this.world?.scannedAt ?? 0;
    this.scanTimer = setTimeout(() => {
      this.scanTimer = null;
      if (!this.scanning) return;
      this.scanning = false;
      this.scanError = 'Scan timed out — no response. Retry Scan, or work offline below.';
    }, SCAN_TIMEOUT_MS);
  }

  private clearScanTimer(): void {
    if (this.scanTimer !== null) {
      clearTimeout(this.scanTimer);
      this.scanTimer = null;
    }
  }

  async connect(host?: string, driver?: string, addr?: string): Promise<void> {
    if (!this.embedded && host !== undefined) {
      const h = host.trim() || DEFAULT_RELAY_HOST;
      this.host = h;
      saveRelayHost(h);
    }
    if (this.phase === 'online' || this.phase === 'connecting') return;
    // An explicit link-up is a real-desk intent: leave offline mode.
    clearOfflineMode();
    this.teardown();
    this.phase = 'connecting';
    this.message = this.embedded ? 'Connecting to embedded engine …' : 'Connecting to ' + this.host + ' …';
    this.online = false;
    this.capsReady = false;
    this.latencyMs = null;
    this.helloAt = Date.now();
    // Remember the intent: reopening the app rejoins the link. A failed
    // handshake clears it again (fail) so a bad host doesn't nag on boot.
    saveRelayAuto(true);
    // The engine modules (EngineWorld + transports + protocol) stay out of
    // the boot chunk: they load here, on the first link-up, while first
    // paint has already happened. Fixture mode never pays for them.
    let engine: typeof import('../world/engineWorld/engineWorld.ts');
    let transports: typeof import('../world/transport.ts');
    try {
      [engine, transports] = await Promise.all([
        import('../world/engineWorld/engineWorld.ts'),
        import('../world/transport.ts'),
      ]);
    } catch (e) {
      this.fail(e instanceof Error ? e.message : String(e));
      return;
    }
    // Embedded (Tauri shell): the engine runs in-process — no relay host.
    // Browser: the relay at the configured host (the relay host field is
    // only used on this path).
    const transport: Transport = this.embedded
      ? new transports.TauriTransport()
      : new transports.WsTransport(relayUrlFromHost(this.host));
    const world = new engine.EngineWorld(transport);
    if (this.phase !== 'connecting') {
      // A disconnect() landed while the engine modules loaded: drop the
      // fresh link instead of resurrecting it.
      try {
        world.destroy();
      } catch {
        /* ignore */
      }
      try {
        transport.close();
      } catch {
        /* ignore */
      }
      return;
    }
    this.transport = transport;
    this.world = world;
    this.unsubs.push(transport.onStatus((up) => this.onStatus(up)));
    this.unsubs.push(
      world.subscribe(() => {
        this.version++;
        // The sweep finished: a devices frame moved scannedAt past the
        // press-time baseline — drop the busy state (the timeout below is
        // only the fallback for a sweep that never reports back).
        if (this.scanning && world.scannedAt !== this.scanBaseline) {
          this.scanning = false;
          this.clearScanTimer();
        }
        if (!this.capsReady && world.capsReady) {
          this.capsReady = true;
          this.latencyMs = Date.now() - this.helloAt;
          this.phase = 'online';
          this.message = 'Live · ' + world.driver + ' · ' + world.stripCount + ' strips';
        }
        for (const cb of [...this.worldSubs]) {
          try {
            cb(world);
          } catch {
            /* ignore: observer-local failure */
          }
        }
      }),
    );
    try {
      // No driver here on purpose: auto-attach (boot/?server=) and plain
      // host connects open the socket + hello and accept the relay's
      // current driver. A driver-switching Connect is sent only when the
      // caller explicitly chose a driver (connection screen).
      world.connect(driver, addr);
    } catch (e) {
      this.fail(e instanceof Error ? e.message : String(e));
      return;
    }
    this.clearTimer();
    this.timer = setTimeout(() => {
      this.timer = null;
      if (this.phase === 'connecting' && !this.capsReady) {
        this.fail(
          this.embedded
            ? 'No response from the embedded engine — restart the app and retry.'
            : this.unreachableText(),
        );
      }
    }, CONNECT_TIMEOUT_MS);
  }

  /** Unreachable-relay error that says WHAT to use instead. A stored
   * 127.0.0.1 is only correct on the relay's own machine: from any other
   * device it points at itself. When this page is LAN-served, name the
   * page's host (one tap away via the relay card's use-page-host button);
   * localhost pages keep the short default text. */
  private unreachableText(): string {
    const base =
      'No response from relay at ' + this.host + ' — is the relay running and reachable from this device?';
    const s = this.suggestedHost();
    if (!s) return base;
    if (isLoopbackHost(relayHostPart(this.host))) {
      return (
        base +
        ` ${this.host} points at this device itself — the relay is usually on the machine serving this page. Try ${s} instead.`
      );
    }
    return base + ` If the relay runs on the machine serving this page, try ${s} instead.`;
  }

  private onStatus(up: boolean): void {
    this.online = up;
    if (!up && this.phase === 'online') {
      this.phase = 'error';
      this.message = this.embedded
        ? 'Embedded engine link lost — retrying …'
        : 'Connection lost — retrying ' + this.host + ' …';
    } else if (!up && this.phase === 'connecting') {
      this.message = this.embedded ? 'Starting embedded engine — retrying …' : 'No route to ' + this.host + ' — retrying …';
    } else if (up && this.phase === 'connecting' && !this.capsReady) {
      this.message = this.embedded ? 'Handshake with embedded engine …' : 'Handshake with ' + this.host + ' …';
    }
  }

  private fail(msg: string): void {
    this.clearTimer();
    this.clearScanTimer();
    this.scanning = false;
    this.teardown();
    this.phase = 'error';
    this.message = msg;
    saveRelayAuto(false);
  }

  disconnect(): void {
    this.clearTimer();
    this.clearScanTimer();
    this.scanning = false;
    this.scanError = null;
    this.teardown();
    this.phase = 'idle';
    this.message = 'Not connected';
    this.online = false;
    this.capsReady = false;
    this.latencyMs = null;
    saveRelayAuto(false);
  }

  private clearTimer(): void {
    if (this.timer !== null) {
      clearTimeout(this.timer);
      this.timer = null;
    }
  }

  private teardown(): void {
    for (const u of this.unsubs.splice(0)) {
      try {
        u();
      } catch {
        /* ignore */
      }
    }
    try {
      this.world?.destroy();
    } catch {
      /* ignore */
    }
    try {
      this.transport?.close();
    } catch {
      /* ignore */
    }
    this.world = null;
    this.transport = null;
  }

  /** Live meter level in dB for a fixture strip id.
   *
   * Topology-driven (the Nth strip of the fixture id's family kind on the
   * live driver — never the old sim-topology index map). Returns null
   * ONLY while offline (callers fall back to the fixture sim); while
   * online an unmapped or silent strip reads -90 (true silence), never
   * the sim's animation. On X-Air only channels have a rig-verified blob
   * index (0 = ch 1); bus/main/FX families read silence until a loud-rig
   * session pins their blob positions — never a neighbor's level.
   */
  liveLevelDb(fixtureId: string): number | null {
    const w = this.world;
    if (this.phase !== 'online' || !w || !w.capsReady) return null;
    if (w.driver !== 'sim' && !this.deskConnected) return -90;
    const ch = bridgeStrip(w, fixtureId);
    if (ch === null || ch >= w.meters.size) return -90;
    if (w.driver === 'xair' && w.kindOf(ch) !== 'in') return -90;
    return linearToDb(w.meters.level[ch] ?? 0);
  }
}

export const live = new LiveRelay();

// Diagnostics hook (same pattern as WorldDebug's `window.__world`): lets
// verification scripts and tests observe the link without touching the UI.
try {
  (window as unknown as { __live?: LiveRelay }).__live = live;
} catch {
  /* non-browser (node tests) */
}
