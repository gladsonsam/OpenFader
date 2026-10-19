// Libraries + desk state: shows/numbered scenes, FX rack, routing patches,
// devices/link/info and the offline diff. Owned by EngineWorld (one concern
// per file); the engine pushes frames in, screens read getters, commands go
// out over the injected send fn. All getters return copies.
//
// Run: node --experimental-strip-types apps/web/src/ui/world/engineWorld/engineWorld.test.ts

import {
  cmdBusFader,
  cmdBusMute,
  cmdCaptureScene,
  cmdConnectDevice,
  cmdClearSolo,
  cmdDeleteScene,
  cmdDisconnect,
  cmdFileDownload,
  cmdFileUpload,
  cmdFxBypass,
  cmdFxOn,
  cmdFxParam,
  cmdFxSetType,
  cmdInfo,
  cmdListShows,
  cmdMuteGroup,
  cmdMuteGroupAssign,
  cmdMainMute,
  cmdSolo,
  cmdOffline,
  cmdOfflineDiscard,
  cmdOfflinePush,
  cmdPatchClear,
  cmdPatchSet,
  cmdProbeDesk,
  cmdRecallScene,
  cmdScan,
  cmdSceneClear,
  cmdSceneDelete,
  cmdSceneRecall,
  cmdSceneRename,
  cmdSceneSave,
  cmdSubgroupAssign,
  cmdSubgroupFader,
  cmdSubgroupMute,
  type EngineEvent,
} from '../protocol.ts';
import { clamp } from '../mapping/params.ts';
import type {
  DeviceView,
  FxSlotView,
  InfoView,
  LinkView,
  OfflineChangeView,
  PatchView,
  SceneEntry,
  ShowLib,
} from '../types.ts';

export class LibraryStore {
  private _shows: ShowLib[] = [];
  private _scenes: SceneEntry[] = [];
  private _fxSlots: FxSlotView[] = [];
  private _fxCaps: { slots: number; types: Array<{ id: string; label: string; params: Array<{ id: string; label: string; unit?: string }> }> } | null = null;
  private _patches: PatchView[] = [];
  private _routingSupported = false;
  private _devices: DeviceView[] = [];
  private _scannedAt = 0;
  private _link: LinkView | null = null;
  private _info: InfoView | null = null;
  private _offlineDiff: OfflineChangeView[] = [];
  private _offlineCount = 0;
  /** Master-section counts from caps.groups (0 = unsupported on the driver). */
  private _groups = { buses: 0, subgroups: 0, muteGroups: 0, mgAssign: true, subAssign: true };
  private _filesSupported = false;
  /** Last-known master states from bus/subgroup/mute_group echoes. */
  private _busMasters: Record<number, { fader: number; muted: boolean }> = {};
  private _subMasters: Record<number, { fader: number; muted: boolean }> = {};
  private _muteMasters: Record<number, boolean> = {};
  private _subAssign: Record<string, boolean> = {};
  private _mgAssign: Record<string, boolean> = {};
  /** Main LR master state (attachunicast + main_mute echoes; null = unknown). */
  private _mainFader: number | null = null;
  private _mainMuted: boolean | null = null;
  /** Per-strip solo (sparse solo echoes; absent = not soloed). */
  private _solos: Record<number, boolean> = {};
  private _lastFile: { filename: string; size: number; fileType: number } | null = null;

  private readonly send: (msg: string) => void;

  constructor(send: (msg: string) => void) {
    this.send = send;
  }

  /** Store one engine frame; true when this store owns the kind. */
  apply(evt: EngineEvent): boolean {
    switch (evt.kind) {
      case 'shows':
        this._shows = evt.shows.map((s) => ({
          name: s.name,
          scenes: s.scenes.map((sc) => ({
            name: sc.name,
            strips: sc.strips.map((st) => ({ ch: st.ch, name: st.name, fader: st.fader, muted: st.muted })),
            main_fader: sc.main_fader,
          })),
        }));
        return true;
      case 'devices':
        this._devices = evt.list.map((d) => ({ ...d }));
        this._scannedAt = evt.scanned_at;
        return true;
      case 'link':
        this._link = { state: evt.state, name: evt.name, ip: evt.ip, fs: evt.fs, latencyMs: evt.latency_ms };
        return true;
      case 'info':
        this._info = {
          model: evt.model,
          firmware: evt.firmware,
          sampleRate: evt.sample_rate,
          consoleName: evt.console_name,
          driver: evt.driver,
        };
        return true;
      case 'scenes':
        this._scenes = evt.scenes.map((s) => ({ ...s }));
        return true;
      case 'fx':
        this._fxSlots = evt.slots.map((s) => ({ ...s, params: { ...s.params } }));
        return true;
      case 'routing':
        this._patches = evt.patches.map((p) => ({ ...p }));
        return true;
      case 'offline_diff':
        this._offlineDiff = evt.changes.map((c) => ({ ...c }));
        this._offlineCount = evt.count;
        return true;
      case 'bus':
        this._busMasters[evt.bus] = { fader: evt.fader, muted: evt.muted };
        return true;
      case 'subgroup':
        this._subMasters[evt.group] = { fader: evt.fader, muted: evt.muted };
        return true;
      case 'subgroup_assign':
        this._subAssign[`${evt.ch}:${evt.group}`] = evt.member;
        return true;
      case 'mute_group':
        this._muteMasters[evt.group] = evt.muted;
        return true;
      case 'mute_group_assign':
        this._mgAssign[`${evt.ch}:${evt.group}`] = evt.member;
        return true;
      case 'main':
        if (evt.fader !== null) this._mainFader = evt.fader;
        if (evt.muted !== null) this._mainMuted = evt.muted;
        return true;
      case 'solo':
        this._solos[evt.ch] = evt.soloed;
        return true;
      case 'file':
        this._lastFile = { filename: evt.filename, size: evt.size, fileType: evt.file_type };
        return true;
      default:
        return false;
    }
  }

  /** caps addendum: FX type tables + routing support (additive, R2). */
  setCaps(
    fx: { slots: number; types: Array<{ id: string; label: string; params: Array<{ id: string; label: string; unit?: string }> }> },
    routingSupported: boolean,
    groups?: { buses: number; subgroups: number; mute_groups: number; mg_assign?: boolean; sub_assign?: boolean },
    filesSupported?: boolean,
  ): void {
    this._fxCaps = { slots: fx.slots, types: fx.types.map((t) => ({ id: t.id, label: t.label, params: t.params.map((q) => ({ ...q })) })) };
    this._routingSupported = routingSupported;
    if (groups) {
      this._groups = {
        buses: Math.max(0, groups.buses),
        subgroups: Math.max(0, groups.subgroups),
        muteGroups: Math.max(0, groups.mute_groups),
        mgAssign: groups.mg_assign ?? true,
        subAssign: groups.sub_assign ?? true,
      };
    }
    if (filesSupported !== undefined) this._filesSupported = filesSupported;
  }

  // ---- shows / scenes ----

  get shows(): ShowLib[] {
    return this._shows.map((s) => ({
      name: s.name,
      scenes: s.scenes.map((sc) => ({
        ...sc,
        strips: sc.strips.map((st) => ({ ...st })),
      })),
    }));
  }
  get numberedScenes(): SceneEntry[] {
    return this._scenes.map((s) => ({ ...s }));
  }
  listShows(): void {
    this.send(cmdListShows());
  }
  captureScene(show: string, name: string): void {
    this.send(cmdCaptureScene(show, name));
  }
  recallScene(show: string, name: string, fadeMs?: number): void {
    this.send(cmdRecallScene(show, name, fadeMs ?? null));
  }
  deleteScene(show: string, name: string): void {
    this.send(cmdDeleteScene(show, name));
  }
  sceneSave(n: number, name: string): void {
    this.send(cmdSceneSave(n, name));
  }
  sceneRecall(n: number, fadeMs?: number, scope?: { strips?: number[]; groups?: string[]; globals?: string[] } | null): void {
    this.send(cmdSceneRecall(n, fadeMs, scope ?? null));
  }
  sceneRename(n: number, name: string): void {
    this.send(cmdSceneRename(n, name));
  }
  sceneDelete(n: number): void {
    this.send(cmdSceneDelete(n));
  }
  sceneClear(): void {
    this.send(cmdSceneClear());
  }

  // ---- fx ----

  get fxSlots(): FxSlotView[] {
    return this._fxSlots.map((s) => ({ ...s, params: { ...s.params } }));
  }
  get fxCaps(): { slots: number; types: Array<{ id: string; label: string; params: Array<{ id: string; label: string; unit?: string }> }> } | null {
    return this._fxCaps ? { slots: this._fxCaps.slots, types: this._fxCaps.types.map((t) => ({ id: t.id, label: t.label, params: t.params.map((q) => ({ ...q })) })) } : null;
  }
  fxSetType(slot: number, type: string): void {
    this.send(cmdFxSetType(slot, type));
  }
  fxParam(slot: number, id: string, v01: number): void {
    this.send(cmdFxParam(slot, id, clamp(v01, 0, 1)));
  }
  fxBypass(slot: number, bypass: boolean): void {
    this.send(cmdFxBypass(slot, bypass));
  }
  fxOn(slot: number, on: boolean): void {
    this.send(cmdFxOn(slot, on));
  }

  // ---- routing ----

  get patches(): PatchView[] {
    return this._patches.map((p) => ({ ...p }));
  }
  get routingSupported(): boolean {
    return this._routingSupported;
  }
  patchSet(strip: number, group: string, index: number): void {
    this.send(cmdPatchSet(strip, group, index));
  }
  patchClear(strip: number): void {
    this.send(cmdPatchClear(strip));
  }

  // ---- devices / link / info / offline ----

  get devices(): DeviceView[] {
    return this._devices.map((d) => ({ ...d }));
  }
  get scannedAt(): number {
    return this._scannedAt;
  }
  scan(): void {
    this.send(cmdScan());
  }
  probeDesk(addr: string): void {
    this.send(cmdProbeDesk(addr));
  }
  connectDevice(id: string): void {
    this.send(cmdConnectDevice(id));
  }
  disconnectDesk(): void {
    this.send(cmdDisconnect());
  }
  requestInfo(): void {
    this.send(cmdInfo());
  }
  get link(): LinkView | null {
    return this._link ? { ...this._link } : null;
  }
  get info(): InfoView | null {
    return this._info ? { ...this._info } : null;
  }
  get offlineDiff(): OfflineChangeView[] {
    return this._offlineDiff.map((c) => ({ ...c }));
  }
  get offlineCount(): number {
    return this._offlineCount;
  }
  offline(model: string): void {
    this.send(cmdOffline(model));
  }
  offlinePush(confirm: boolean): void {
    this.send(cmdOfflinePush(confirm));
  }
  offlineDiscard(): void {
    this.send(cmdOfflineDiscard());
  }

  // ---- masters / groups / files (R2) ----

  get groupCaps(): { buses: number; subgroups: number; muteGroups: number; mgAssign: boolean; subAssign: boolean; files: boolean } {
    return { ...this._groups, files: this._filesSupported };
  }
  busMaster(bus: number): { fader: number; muted: boolean } | null {
    const m = this._busMasters[bus];
    return m ? { ...m } : null;
  }
  subgroupMaster(group: number): { fader: number; muted: boolean } | null {
    const m = this._subMasters[group];
    return m ? { ...m } : null;
  }
  muteGroupMaster(group: number): boolean | null {
    const m = this._muteMasters[group];
    return typeof m === 'boolean' ? m : null;
  }
  subgroupAssigned(ch: number, group: number): boolean | null {
    const m = this._subAssign[`${ch}:${group}`];
    return typeof m === 'boolean' ? m : null;
  }
  muteGroupAssigned(ch: number, group: number): boolean | null {
    const m = this._mgAssign[`${ch}:${group}`];
    return typeof m === 'boolean' ? m : null;
  }
  /** Main LR master fader/mute (attach + echoes; null = unknown yet). */
  get mainMaster(): { fader: number | null; muted: boolean | null } {
    return { fader: this._mainFader, muted: this._mainMuted };
  }
  /** Per-strip solo (sparse echoes; false when never echoed). */
  soloOf(ch: number): boolean {
    return this._solos[ch] === true;
  }
  get lastFile(): { filename: string; size: number; fileType: number } | null {
    return this._lastFile ? { ...this._lastFile } : null;
  }
  busFader(bus: number, value: number): void {
    this.send(cmdBusFader(bus, value));
  }
  busMute(bus: number, muted: boolean): void {
    this.send(cmdBusMute(bus, muted));
  }
  subgroupFader(group: number, value: number): void {
    this.send(cmdSubgroupFader(group, value));
  }
  subgroupMute(group: number, muted: boolean): void {
    this.send(cmdSubgroupMute(group, muted));
  }
  subgroupAssign(ch: number, group: number, member: boolean): void {
    this.send(cmdSubgroupAssign(ch, group, member));
  }
  muteGroup(group: number, muted: boolean): void {
    this.send(cmdMuteGroup(group, muted));
  }
  muteGroupAssign(ch: number, group: number, member: boolean): void {
    this.send(cmdMuteGroupAssign(ch, group, member));
  }
  setMainMute(muted: boolean): void {
    this.send(cmdMainMute(muted));
  }
  setSolo(ch: number, soloed: boolean): void {
    this.send(cmdSolo(ch, soloed));
  }
  /** Fire-and-forget clear-solo (X-Air `/-action/clearsolo`; honest `unsupported` elsewhere). */
  clearSolo(): void {
    this.send(cmdClearSolo());
  }
  fileDownload(filename: string): void {
    this.send(cmdFileDownload(filename));
  }
  fileUpload(filename: string, data: string, fileType: number): void {
    this.send(cmdFileUpload(filename, data, fileType));
  }
}
