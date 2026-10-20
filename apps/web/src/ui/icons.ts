// Shared control artwork and the locally bundled channel icon catalogue.

import micUrl from './assets/icons8/microphone.png';
import choirUrl from './assets/icons8/choir.png';
import talkbackUrl from './assets/icons8/megaphone.png';
import headsetUrl from './assets/icons8/headset.png';
import guitarUrl from './assets/icons8/guitar.png';
import bassUrl from './assets/icons8/bass-guitar.png';
import banjoUrl from './assets/icons8/banjo.png';
import violinUrl from './assets/icons8/violin.png';
import celloUrl from './assets/icons8/cello.png';
import harpUrl from './assets/icons8/harp.png';
import drumsUrl from './assets/icons8/drum-set.png';
import kickUrl from './assets/icons8/bass-drum.png';
import snareUrl from './assets/icons8/snare-drum.png';
import cymbalsUrl from './assets/icons8/cymbals.png';
import maracasUrl from './assets/icons8/maracas.png';
import xylophoneUrl from './assets/icons8/xylophone.png';
import bellUrl from './assets/icons8/bell.png';
import keysUrl from './assets/icons8/piano.png';
import accordionUrl from './assets/icons8/accordion.png';
import saxophoneUrl from './assets/icons8/saxophone.png';
import trumpetUrl from './assets/icons8/trumpet.png';
import tromboneUrl from './assets/icons8/trombone.png';
import fluteUrl from './assets/icons8/flute.png';
import clarinetUrl from './assets/icons8/clarinet.png';
import harmonicaUrl from './assets/icons8/harmonica.png';
import vinylUrl from './assets/icons8/music-record.png';
import cdUrl from './assets/icons8/cd.png';
import laptopUrl from './assets/icons8/laptop.png';
import phoneUrl from './assets/icons8/smartphone.png';
import radioUrl from './assets/icons8/radio.png';
import usbUrl from './assets/icons8/usb-memory-stick.png';
import videoUrl from './assets/icons8/video.png';
import tvUrl from './assets/icons8/tv.png';
import webcamUrl from './assets/icons8/webcam.png';
import libraryUrl from './assets/icons8/music-library.png';
import headphonesUrl from './assets/icons8/headphones.png';
import speakerUrl from './assets/icons8/speaker.png';
import subwooferUrl from './assets/icons8/subwoofer.png';
import groupUrl from './assets/icons8/music-conductor.png';
import orchestraUrl from './assets/icons8/orchestra.png';
import fxUrl from './assets/icons8/audio-wave.png';
import musicUrl from './assets/icons8/musical-notes.png';
import cableUrl from './assets/icons8/audio-cable.png';

export interface IconDef {
  viewBox: string;
  /** Inner SVG markup, including image elements for bundled colour artwork. */
  body: string;
  /** Default stroke width; raster artwork uses zero. */
  strokeWidth: number;
  /** linecap used in the template (most are round) */
  linecap?: 'round' | 'butt' | 'square' | 'inherit';
  linejoin?: 'round' | 'miter' | 'bevel' | 'miter-clip' | 'arcs';
  /** true for the filled routing-grid dots */
  hasFillElements?: boolean;
}

const def = (
  viewBox: string,
  body: string,
  strokeWidth: number,
  extra: Partial<IconDef> = {},
): IconDef => ({ viewBox, body, strokeWidth, ...extra });

export const icons = {
  /** Back chevron (header Back button, 18x18, stroke 2.2) */
  back: def(
    '0 0 24 24',
    '<path d="M15 6l-6 6 6 6"></path>',
    2.2,
    { linecap: 'round', linejoin: 'round' },
  ),
  /** Lock (locked banner, 16x16, stroke 2.2) */
  lock: def(
    '0 0 24 24',
    '<rect x="5" y="11" width="14" height="10" rx="1.5"></rect><path d="M8 11V8a4 4 0 0 1 8 0v3"></path>',
    2.2,
    { linecap: 'round', linejoin: 'round' },
  ),
  /** Routing grid (header Routing button, 22x22, stroke 1.8, filled dots) */
  routing: def(
    '0 0 24 24',
    '<path d="M3 7h18M3 12h18M3 17h18M7 3v18M12 3v18M17 3v18"></path><circle cx="7" cy="12" r="1.8" fill="currentColor"></circle><circle cx="12" cy="17" r="1.8" fill="currentColor"></circle><circle cx="17" cy="7" r="1.8" fill="currentColor"></circle>',
    1.8,
    { linecap: 'round', hasFillElements: true },
  ),
  /** Connection plug (header Connect button, 22x22, stroke 2) */
  plug: def(
    '0 0 24 24',
    '<path d="M9 2v5M15 2v5M6 7h12v4a6 6 0 0 1-12 0zM12 17v5"></path>',
    2,
    { linecap: 'round', linejoin: 'round' },
  ),
  /** Settings gear (header Settings button, 22x22, stroke 1.8) */
  gear: def(
    '0 0 24 24',
    '<circle cx="12" cy="12" r="3"></circle><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"></path>',
    1.8,
    { linecap: 'round', linejoin: 'round' },
  ),
  /** Chevron down (scene dropdown button, 16x16, stroke var(--u4-text-5) 2.2) */
  chevronDown: def(
    '0 0 24 24',
    '<path d="M6 9l6 6 6-6"></path>',
    2.2,
    { linecap: 'round', linejoin: 'round' },
  ),
  /** Console device icon (Connection device cards, 24x24, stroke 1.8) */
  console: def(
    '0 0 24 24',
    '<path d="M3 5h18v14H3zM7 9v6M11 9v6M15 9v6M19 9v6"></path>',
    1.8,
    { linecap: 'round', linejoin: 'round' },
  ),
  /** I/O device icon (Connection stage-box card, 24x24, stroke 1.8) */
  io: def(
    '0 0 24 24',
    '<path d="M4 7h16v10H4zM7 12h.01M10 12h.01M13 12h.01M16 12h.01"></path>',
    1.8,
    { linecap: 'round', linejoin: 'round' },
  ),
  /** Meter bridge (lucide audio-lines artwork, 24x24, stroke 2) */
  meters: def(
    '0 0 24 24',
    '<path d="M2 10v3" /><path d="M6 6v11" /><path d="M10 3v18" /><path d="M14 8v8" /><path d="M18 5v13" /><path d="M22 10v3" />',
    2,
    { linecap: 'round', linejoin: 'round' },
  ),
  /** Previous channel (channel strip pager, 20x20, stroke 2.2) */
  chevronLeft: def(
    '0 0 24 24',
    '<path d="M15 6l-6 6 6 6"></path>',
    2.2,
    { linecap: 'round', linejoin: 'round' },
  ),
  /** Next channel (channel strip pager, 20x20, stroke 2.2) */
  chevronRight: def(
    '0 0 24 24',
    '<path d="M9 6l6 6-6 6"></path>',
    2.2,
    { linecap: 'round', linejoin: 'round' },
  ),
} satisfies Record<string, IconDef>;

export type IconName = keyof typeof icons;

/** Bundled Icons8 Color assets: shared by strips, tags, and the searchable picker. */
const channelIcon = (url: string, label: string, category: string, keywords: string) => ({
  ...def('0 0 100 100', `<image href="${url}" width="100" height="100" />`, 0),
  label,
  category,
  keywords,
});

export const chanIcons = {
  mic: channelIcon(micUrl, "Microphone", "Vocals", "vocal vox lead backing speech"),
  choir: channelIcon(choirUrl, "Choir", "Vocals", "vocal backing ensemble"),
  talkback: channelIcon(talkbackUrl, "Talkback", "Vocals", "announcement speech"),
  headset: channelIcon(headsetUrl, "Headset", "Vocals", "wireless speech"),
  guitar: channelIcon(guitarUrl, "Guitar", "Strings", "acoustic gtr"),
  bass: channelIcon(bassUrl, "Bass guitar", "Strings", "electric bass"),
  banjo: channelIcon(banjoUrl, "Banjo", "Strings", "folk"),
  violin: channelIcon(violinUrl, "Violin", "Strings", "fiddle strings"),
  cello: channelIcon(celloUrl, "Cello", "Strings", "strings"),
  harp: channelIcon(harpUrl, "Harp", "Strings", "strings"),
  drums: channelIcon(drumsUrl, "Drum kit", "Drums & percussion", "drums kit"),
  kick: channelIcon(kickUrl, "Kick", "Drums & percussion", "bass drum bd"),
  snare: channelIcon(snareUrl, "Snare", "Drums & percussion", "drum sd"),
  cymbals: channelIcon(cymbalsUrl, "Cymbals", "Drums & percussion", "hi hat overhead oh crash ride"),
  maracas: channelIcon(maracasUrl, "Maracas", "Drums & percussion", "shaker percussion"),
  xylophone: channelIcon(xylophoneUrl, "Xylophone", "Drums & percussion", "mallet marimba percussion"),
  bell: channelIcon(bellUrl, "Bell", "Drums & percussion", "chime percussion"),
  keys: channelIcon(keysUrl, "Keys", "Keys & winds", "keyboard piano synth organ"),
  accordion: channelIcon(accordionUrl, "Accordion", "Keys & winds", "keys"),
  saxophone: channelIcon(saxophoneUrl, "Saxophone", "Keys & winds", "sax brass woodwind"),
  trumpet: channelIcon(trumpetUrl, "Trumpet", "Keys & winds", "brass horn"),
  trombone: channelIcon(tromboneUrl, "Trombone", "Keys & winds", "brass horn"),
  flute: channelIcon(fluteUrl, "Flute", "Keys & winds", "woodwind"),
  clarinet: channelIcon(clarinetUrl, "Clarinet", "Keys & winds", "woodwind"),
  harmonica: channelIcon(harmonicaUrl, "Harmonica", "Keys & winds", "blues harp"),
  vinyl: channelIcon(vinylUrl, "Turntable", "Playback & media", "dj vinyl record tracks"),
  cd: channelIcon(cdUrl, "Disc", "Playback & media", "cd playback tracks"),
  laptop: channelIcon(laptopUrl, "Laptop", "Playback & media", "computer playback tracks"),
  phone: channelIcon(phoneUrl, "Phone", "Playback & media", "mobile playback aux"),
  radio: channelIcon(radioUrl, "Radio", "Playback & media", "broadcast tuner"),
  usb: channelIcon(usbUrl, "USB drive", "Playback & media", "playback tracks"),
  video: channelIcon(videoUrl, "Video", "Playback & media", "media film"),
  tv: channelIcon(tvUrl, "TV", "Playback & media", "screen television"),
  webcam: channelIcon(webcamUrl, "Camera", "Playback & media", "video livestream"),
  library: channelIcon(libraryUrl, "Playlist", "Playback & media", "music library tracks"),
  headphones: channelIcon(headphonesUrl, "Headphones", "Outputs & groups", "monitor iem ears"),
  speaker: channelIcon(speakerUrl, "Speaker", "Outputs & groups", "main output pa"),
  subwoofer: channelIcon(subwooferUrl, "Subwoofer", "Outputs & groups", "sub output bass"),
  group: channelIcon(groupUrl, "Group", "Outputs & groups", "dca bus master conductor"),
  orchestra: channelIcon(orchestraUrl, "Orchestra", "Outputs & groups", "ensemble band"),
  fx: channelIcon(fxUrl, "Effects", "Outputs & groups", "fx reverb delay processing"),
  music: channelIcon(musicUrl, "Music", "Outputs & groups", "notes generic"),
  cable: channelIcon(cableUrl, "Line input", "Outputs & groups", "di cable jack aux input"),
} satisfies Record<string, IconDef & { label: string; category: string; keywords: string }>;

export type ChanIconName = keyof typeof chanIcons;

/** Automatic channel artwork; an explicit icon or "No icon" overrides it. */
export function defaultChannelIcon(name: string, id: string): ChanIconName {
  if (/kick|bass drum/i.test(name)) return 'kick';
  if (/snare/i.test(name)) return 'snare';
  if (/hat|cymbal|overhead|\boh\b/i.test(name)) return 'cymbals';
  if (/drum|tom|perc/i.test(name)) return 'drums';
  if (/bass/i.test(name)) return 'bass';
  if (/guitar|\bgtr\b|\bac gt\b/i.test(name)) return 'guitar';
  if (/sax/i.test(name)) return 'saxophone';
  if (/trumpet/i.test(name)) return 'trumpet';
  if (/trombone/i.test(name)) return 'trombone';
  if (/violin|fiddle/i.test(name)) return 'violin';
  if (/cello/i.test(name)) return 'cello';
  if (/flute/i.test(name)) return 'flute';
  if (/clarinet/i.test(name)) return 'clarinet';
  if (/keys|piano|synth|organ/i.test(name)) return 'keys';
  if (/choir/i.test(name)) return 'choir';
  if (/talk/i.test(name)) return 'talkback';
  if (/vox|vocal|mic/i.test(name)) return 'mic';
  if (/laptop|computer/i.test(name)) return 'laptop';
  if (/playback|track|\bdj\b|usb|aux/i.test(name)) return 'vinyl';
  if (/^fx/.test(id)) return 'fx';
  if (/monitor|\biem\b|^mix\d/i.test(name) || /^mix\d/.test(id)) return 'headphones';
  if (/^main|^mtx/.test(id)) return 'speaker';
  if (/^dca|^grp/.test(id)) return 'group';
  return /^in\d/.test(id) ? 'mic' : 'music';
}

export const CHANNEL_ICON_CATEGORIES = ["Vocals", "Strings", "Drums & percussion", "Keys & winds", "Playback & media", "Outputs & groups"];

/** Stable IDs preserve saved channel selections as the catalogue grows. */
export const CHAN_ICON_NAMES = [
  'mic',
  'choir',
  'talkback',
  'headset',
  'guitar',
  'bass',
  'banjo',
  'violin',
  'cello',
  'harp',
  'drums',
  'kick',
  'snare',
  'cymbals',
  'maracas',
  'xylophone',
  'bell',
  'keys',
  'accordion',
  'saxophone',
  'trumpet',
  'trombone',
  'flute',
  'clarinet',
  'harmonica',
  'vinyl',
  'cd',
  'laptop',
  'phone',
  'radio',
  'usb',
  'video',
  'tv',
  'webcam',
  'library',
  'headphones',
  'speaker',
  'subwoofer',
  'group',
  'orchestra',
  'fx',
  'music',
  'cable',
] satisfies ChanIconName[];
