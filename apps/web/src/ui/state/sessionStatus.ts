import type { LivePhase } from './live.ts';
import type { LinkView } from '../world/types.ts';

/** Only a confirmed hardware session can be green. Transport readiness and
 * simulator capability snapshots are not evidence of a connected mixer. */
export function sessionStatus(phase: LivePhase, capsReady: boolean, driver: string, link: LinkView | null, offline = false) {
  const connected = !offline && phase === 'online' && capsReady && driver !== 'sim' && link?.state === 'connected';
  const demo = !offline && phase === 'online' && capsReady && driver === 'sim';
  const reconnecting = !offline && phase === 'online' && driver !== 'sim' && link?.state === 'reconnecting';
  const starting = !offline && phase === 'connecting';
  const name = connected || reconnecting ? link?.name?.trim() || link?.ip || 'Mixer' : '';
  const label = connected ? `Connected to ${name}` : offline ? 'Working offline' : reconnecting ? `Reconnecting to ${name}` : starting ? 'Starting connection service' : 'Not connected';
  const detail = connected ? [link?.ip, link?.fs ? `${link.fs / 1000} kHz` : ''].filter(Boolean).join(' · ')
    : offline ? 'Local preview. Changes stay on this device.'
    : demo ? 'Simulator preview · no hardware mixer connected.'
    : reconnecting ? 'Waiting for the mixer to respond. Confirmed values will return when the session recovers.'
    : phase === 'error' ? 'Connection service unavailable. Retry to continue.'
    : starting ? 'Preparing mixer discovery. You can cancel or work offline.'
    : 'Find your mixer on Wi-Fi or connect with its IP address.';
  return { connected, demo, reconnecting, starting, name, label, detail,
    color: connected ? '#5fe060' : offline || reconnecting || starting ? '#f0b44a' : '#ff7d70',
    aria: `Connection: ${label}, select device`,
    ready: !offline && phase === 'online' && capsReady,
  };
}
