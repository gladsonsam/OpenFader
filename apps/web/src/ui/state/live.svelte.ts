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

import { EngineWorld } from '../world/engineWorld/engineWorld.ts';
import { WsTransport } from '../world/transport.ts';
import {
  CONNECT_TIMEOUT_MS,
  DEFAULT_RELAY_HOST,
  loadRelayAuto,
  loadRelayHost,
  relayHostFromSearch,
  saveRelayAuto,
  saveRelayHost,
  linearToDb,
  relayUrlFromHost,
  type LivePhase,
} from './live.ts';
import { bridgeStrip } from './engineBridge.ts';

export type { LivePhase };

class LiveRelay {
  phase: LivePhase = $state('idle');
  message = $state('Not connected');
  host: string = $state(loadRelayHost());
  online = $state(false);
  capsReady = $state(false);
  latencyMs: number | null = $state(null);
  /** Bumped on every engine ingest so screens re-read meters/version. */
  version = $state(0);

  private transport: WsTransport | null = null;
  private world: EngineWorld | null = null;
  private unsubs: Array<() => void> = [];
  private timer: ReturnType<typeof setTimeout> | null = null;
  private helloAt = 0;

  get url(): string {
    return relayUrlFromHost(this.host);
  }

  get engine(): EngineWorld | null {
    return this.world;
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

  /** Apply `?server=` once at boot (called by the shell mount). */
  initFromUrl(search: string): void {
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
      // No ?server=: when the page itself is served from the relay's
      // :8080 origin (sim doubles as the static host), default to it so a
      // phone on LAN needs no typing.
      try {
        if (window.location.port === '8080' && window.location.hostname) {
          const h = window.location.hostname + ':8080';
          if (h !== this.host) {
            this.disconnect();
            this.host = h;
            saveRelayHost(h);
          }
        }
      } catch {
        /* ignore */
      }
      // Native feel: reopen the app with the link up when the user left it
      // up. Fixture/diff pages never auto-link (deterministic screenshots).
      if (loadRelayAuto() && this.phase === 'idle') this.connect();
    }
  }

  setHost(host: string): void {
    const h = host.trim() || DEFAULT_RELAY_HOST;
    if (h === this.host) return;
    this.disconnect();
    this.host = h;
    saveRelayHost(h);
  }

  connect(host?: string, driver?: string, addr?: string): void {
    if (host !== undefined) {
      const h = host.trim() || DEFAULT_RELAY_HOST;
      this.host = h;
      saveRelayHost(h);
    }
    if (this.phase === 'online' || this.phase === 'connecting') return;
    this.teardown();
    this.phase = 'connecting';
    this.message = 'Connecting to ' + this.host + ' …';
    this.online = false;
    this.capsReady = false;
    this.latencyMs = null;
    this.helloAt = Date.now();
    // Remember the intent: reopening the app rejoins the link. A failed
    // handshake clears it again (fail) so a bad host doesn't nag on boot.
    saveRelayAuto(true);
    const transport = new WsTransport(relayUrlFromHost(this.host));
    const world = new EngineWorld(transport);
    this.transport = transport;
    this.world = world;
    this.unsubs.push(transport.onStatus((up) => this.onStatus(up)));
    this.unsubs.push(
      world.subscribe(() => {
        this.version++;
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
          'No response from relay at ' + this.host + ' — is the relay running and reachable from this device?',
        );
      }
    }, CONNECT_TIMEOUT_MS);
  }

  private onStatus(up: boolean): void {
    this.online = up;
    if (!up && this.phase === 'online') {
      this.phase = 'error';
      this.message = 'Connection lost — retrying ' + this.host + ' …';
    } else if (!up && this.phase === 'connecting') {
      this.message = 'No route to ' + this.host + ' — retrying …';
    } else if (up && this.phase === 'connecting' && !this.capsReady) {
      this.message = 'Handshake with ' + this.host + ' …';
    }
  }

  private fail(msg: string): void {
    this.clearTimer();
    this.teardown();
    this.phase = 'error';
    this.message = msg;
    saveRelayAuto(false);
  }

  disconnect(): void {
    this.clearTimer();
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
