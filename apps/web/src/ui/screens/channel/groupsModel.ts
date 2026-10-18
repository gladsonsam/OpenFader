// Channel groups view-model: the desk's mute-group / subgroup assignments
// for the selected channel, plus stereo-link state and master mutes.
//
// Pure over an injected live source (reads only): null source or null
// engine strip hides the card (fixture mode renders nothing, so the
// fixture baselines are untouched). Membership and master values are
// echo-confirmed only — null ("not confirmed yet") until the first
// attach snapshot or assign/mute echo; the UI shows that honestly
// instead of faking a no. Where the desk has no per-channel assign leaf
// (X-Air: masters only), the assign toggle collapses to a master-only
// row (assignSupported = false).

export interface GroupRowVm {
  /** 0-based engine group index (the command arg). */
  group: number;
  /** Display label (`MG 1`, `SUB 2`; 1-based like the desk). */
  label: string;
  /** Confirmed membership (null = no echo yet, unknown). */
  member: boolean | null;
  /** Member toggle visuals (follows the Pre/Post toggle language). */
  memberLabel: string;
  memberAria: string;
  memberBg: string;
  memberFg: string;
  /** Master mute (null = no echo yet, unknown). */
  masterMuted: boolean | null;
  /** False where the desk has no per-channel assign leaf: the assign
   *  toggle collapses (master-only row). */
  assign: boolean;
  masterAria: string;
  masterBg: string;
  masterFg: string;
}

export interface GroupsVm {
  /** Card title (`Desk groups · <name>`). */
  title: string;
  mg: GroupRowVm[];
  sg: GroupRowVm[];
  /** Stereo-link leaf state for the strip. */
  link: 'on' | 'off' | 'unsupported' | 'unknown';
  /** Honest-UI line: counts, unknown states, or why nothing is shown. */
  note: string;
}

/** Live reads the builder needs (EngineWorld shaped, fixture-free). */
export interface LiveGroupsSource {
  /** Engine strip index for the selected fixture channel (null = unmapped). */
  engineCh: number | null;
  /** Desk name for honest notes (`xair`, `sim`, …). */
  driver: string;
  muteGroups: number;
  subgroups: number;
  /** Per-channel assign leaves exist (false = masters only: collapse). */
  assignMg: boolean;
  assignSg: boolean;
  muteGroupAssigned(group: number): boolean | null;
  subgroupAssigned(group: number): boolean | null;
  muteGroupMaster(group: number): boolean | null;
  subgroupMasterMuted(group: number): boolean | null;
  /** False where the desk has no per-channel leaf (xair global chlink). */
  linkSupported: boolean;
  /** Confirmed link leaf (null = unknown). */
  link(): boolean | null;
}

function row(
  kind: 'MG' | 'SUB',
  group: number,
  member: boolean | null,
  masterMuted: boolean | null,
  assign: boolean,
): GroupRowVm {
  const label = kind === 'MG' ? `MG ${group + 1}` : `SUB ${group + 1}`;
  const what = kind === 'MG' ? `Mute group ${group + 1}` : `Subgroup ${group + 1}`;
  return {
    group,
    label,
    member,
    memberLabel: member === null ? '?' : member ? 'In' : 'Out',
    memberAria: `${what} membership${member === null ? ', not confirmed yet' : member ? ', member' : ', not a member'}`,
    memberBg: member === true ? '#e8edf5' : '#2b3038',
    memberFg: member === true ? '#0b0f14' : '#c9d1dc',
    masterMuted,
    masterAria: `${what} master mute${masterMuted === null ? ', state unknown' : masterMuted ? ', muted' : ', open'}`,
    masterBg: masterMuted === true ? '#d92d22' : '#2b3038',
    masterFg: masterMuted === true ? '#ffffff' : '#c9d1dc',
    assign,
  };
}

/** Build the groups card vm (null = hide the card: offline or unmapped). */
export function groupsVmFor(src: LiveGroupsSource | null): GroupsVm | null {
  if (!src || src.engineCh === null) return null;
  const mg: GroupRowVm[] = [];
  for (let g = 0; g < src.muteGroups; g++) {
    mg.push(row('MG', g, src.muteGroupAssigned(g), src.muteGroupMaster(g), src.assignMg));
  }
  const sg: GroupRowVm[] = [];
  for (let g = 0; g < src.subgroups; g++) {
    sg.push(row('SUB', g, src.subgroupAssigned(g), src.subgroupMasterMuted(g), src.assignSg));
  }
  const link: GroupsVm['link'] = !src.linkSupported
    ? 'unsupported'
    : src.link() === true
      ? 'on'
      : src.link() === false
        ? 'off'
        : 'unknown';
  const notes: string[] = [];
  if (src.muteGroups === 0) notes.push('no desk mute groups');
  if (src.subgroups === 0) notes.push('no desk subgroups');
  if (!src.assignMg && src.muteGroups > 0) notes.push('desk has mute-group masters only (no assign leaf)');
  if (link === 'unsupported') notes.push('stereo link is a global desk switch (no per-channel leaf)');
  if ((src.assignMg && mg.some(r => r.member === null)) || (src.assignSg && sg.some(r => r.member === null))) {
    notes.push('membership not confirmed yet');
  }
  return {
    title: 'Desk groups',
    mg,
    sg,
    link,
    note: notes.length ? `${src.driver}: ${notes.join('; ')}.` : '',
  };
}
