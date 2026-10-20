// Channel assign view-model: the desk's assign plane for the selected
// channel — LR assign, pan, DCA groups, mute groups (membership +
// masters) and automix (enable + weight + live gain-reduction), plus
// stereo-link state.
//
// Pure over an injected live source (reads only): null source or null
// engine strip hides the card (fixture mode renders nothing, so the
// fixture baselines are untouched). Membership/master/param values are
// echo-confirmed only — null ("not confirmed yet") until the first
// attach snapshot or assign/param echo; the UI shows that honestly
// instead of faking a no. Where the desk has no per-channel assign leaf
// (subgroups on X-Air: masters only — the desk has no subgroup leaf at
// all), the assign toggle collapses to a master-only row
// (assignSupported = false).

export interface DcaRowVm {
  /** 0-based engine DCA group index (the command arg). */
  group: number;
  /** Display label (`DCA 1`; 1-based like the desk). */
  label: string;
  /** Confirmed membership (null = no echo yet, unknown). */
  member: boolean | null;
  memberLabel: string;
  memberAria: string;
  memberBg: string;
  memberFg: string;
}

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
  /** DCA groups (channel → DCA membership, caps-gated per group). */
  dca: DcaRowVm[];
  /** False where the desk has no DCA-assign leaf: the section collapses. */
  dcaSupported: boolean;
  /** LR assign leaf state. */
  lrSupported: boolean;
  lrAssigned: boolean | null;
  lrLabel: string;
  lrAria: string;
  lrBg: string;
  lrFg: string;
  /** Pan leaf state (-100..100, null = unknown). */
  panSupported: boolean;
  pan: number | null;
  panText: string;
  panAria: string;
  /** Automix enable + weight leaves. */
  automixSupported: boolean;
  automixOn: boolean | null;
  automixOnLabel: string;
  automixOnAria: string;
  automixOnBg: string;
  automixOnFg: string;
  automixWeight: number | null;
  automixWeightText: string;
  automixWeightAria: string;
  /** Live comp gain-reduction in dB (>= 0; null = no meter data yet). */
  grComp: number | null;
  grText: string;
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
  /** DCA groups on the desk (4 on X-Air; 0 = section collapses). */
  dcaGroups: number;
  /** Per-DCA assign leaves exist (false = no DCA-assign leaf). */
  assignDca: boolean;
  /** Confirmed DCA membership (null = unknown). */
  dcaAssigned(group: number): boolean | null;
  /** False where the desk has no LR-assign leaf: the row collapses. */
  lrSupported: boolean;
  /** Confirmed LR assign (null = unknown). */
  lrAssign(): boolean | null;
  /** False where the desk has no pan leaf: the row collapses. */
  panSupported: boolean;
  /** Confirmed pan -100..100 (null = unknown). */
  pan(): number | null;
  /** False where the desk has no automix leaves: the section collapses. */
  automixSupported: boolean;
  /** Confirmed automix enable (null = unknown). */
  automixOn(): boolean | null;
  /** Confirmed automix weight 0..1 (null = unknown). */
  automixWeight(): number | null;
  /** Live comp gain-reduction in dB (null = no meter data yet). */
  grComp(): number | null;
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

function dcaRow(group: number, member: boolean | null): DcaRowVm {
  const what = `DCA ${group + 1}`;
  return {
    group,
    label: what,
    member,
    memberLabel: member === null ? '?' : member ? 'In' : 'Out',
    memberAria: `${what} membership${member === null ? ', not confirmed yet' : member ? ', member' : ', not a member'}`,
    memberBg: member === true ? '#e8edf5' : '#2b3038',
    memberFg: member === true ? '#0b0f14' : '#c9d1dc',
  };
}

/** Build the assign card vm (null = hide the card: offline or unmapped). */
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
  const dca: DcaRowVm[] = [];
  for (let g = 0; g < src.dcaGroups; g++) {
    dca.push(dcaRow(g, src.assignDca ? src.dcaAssigned(g) : null));
  }
  const lr = src.lrSupported ? src.lrAssign() : null;
  const pan = src.panSupported ? src.pan() : null;
  const axOn = src.automixSupported ? src.automixOn() : null;
  const axW = src.automixSupported ? src.automixWeight() : null;
  const gr = src.automixSupported ? src.grComp() : null;
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
  if (src.dcaGroups === 0 || !src.assignDca) notes.push('no DCA assign leaf');
  else if (dca.some(r => r.member === null)) notes.push('DCA membership not confirmed yet');
  if (!src.lrSupported) notes.push('no LR assign leaf');
  else if (lr === null) notes.push('LR assign not confirmed yet');
  if (!src.panSupported) notes.push('no pan leaf');
  else if (pan === null) notes.push('pan not confirmed yet');
  if (!src.automixSupported) notes.push('no automix leaves');
  else {
    if (axOn === null) notes.push('automix enable not confirmed yet');
    if (axW === null) notes.push('automix weight not confirmed yet');
  }
  if (link === 'unsupported') notes.push('stereo link is a global desk switch (no per-channel leaf)');
  if ((src.assignMg && mg.some(r => r.member === null)) || (src.assignSg && sg.some(r => r.member === null))) {
    notes.push('group membership not confirmed yet');
  }
  return {
    title: 'Assign',
    mg,
    sg,
    dca,
    dcaSupported: src.dcaGroups > 0 && src.assignDca,
    lrSupported: src.lrSupported,
    lrAssigned: lr,
    lrLabel: lr === null ? 'LR ?' : lr ? 'LR IN' : 'LR OUT',
    lrAria: `LR assign${lr === null ? ', not confirmed yet' : lr ? ', assigned' : ', unassigned'}`,
    lrBg: lr === true ? '#e8edf5' : '#2b3038',
    lrFg: lr === true ? '#0b0f14' : '#c9d1dc',
    panSupported: src.panSupported,
    pan,
    panText: pan === null ? '?' : `${pan >= 0 ? 'R' : 'L'}${Math.abs(Math.round(pan))}`,
    panAria: `Pan${pan === null ? ', not confirmed yet' : `, ${Math.round(pan)}`}`,
    automixSupported: src.automixSupported,
    automixOn: axOn,
    automixOnLabel: axOn === null ? 'X ?' : axOn ? 'X ON' : 'X OFF',
    automixOnAria: `Automix enable${axOn === null ? ', not confirmed yet' : axOn ? ', on' : ', off'}`,
    automixOnBg: axOn === true ? '#e8edf5' : '#2b3038',
    automixOnFg: axOn === true ? '#0b0f14' : '#c9d1dc',
    automixWeight: axW,
    automixWeightText: axW === null ? '?' : `${Math.round(axW * 100)}%`,
    automixWeightAria: `Automix weight${axW === null ? ', not confirmed yet' : `, ${Math.round(axW * 100)} percent`}`,
    grComp: gr,
    grText: gr === null ? '—' : `${gr.toFixed(1)} dB`,
    link,
    note: notes.length ? `${src.driver}: ${notes.join('; ')}.` : '',
  };
}
