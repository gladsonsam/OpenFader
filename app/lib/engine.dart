// Cmd builders (client -> engine) and Event parsers (engine -> client).
// Same wire protocol as the WS relay (`openfader_engine::protocol`);
// unknown event types are ignored for interop.
import 'dart:convert';
import 'dart:typed_data';

import 'core/local_library.dart';
export 'core/local_library.dart';

/// Passive native desk reports, separate from OpenFader's local scene library.
/// `ready` means some push evidence arrived, never a complete slot inventory.
enum NativeSnapshotsStatus { unknown, unsupported, unavailable, loading, ready }

class NativeSnapshotSlotVm {
  final int slot;
  final String name;

  const NativeSnapshotSlotVm._(this.slot, this.name);

  @override
  bool operator ==(Object other) =>
      other is NativeSnapshotSlotVm && slot == other.slot && name == other.name;

  @override
  int get hashCode => Object.hash(slot, name);
}

class NativeSnapshotsVm {
  final String driver;

  /// Engine connection epoch, matching `sync_epoch`, not the RTA source epoch.
  final int? epoch;
  final bool supported;
  final bool readOnly;
  final NativeSnapshotsStatus status;
  final List<NativeSnapshotSlotVm> slots;

  /// Last reported native recall id, not confirmation of mixer audio state.
  final int? current;

  const NativeSnapshotsVm.unknown({this.driver = ''})
      : epoch = null,
        supported = false,
        readOnly = false,
        status = NativeSnapshotsStatus.unknown,
        slots = const [],
        current = null;

  const NativeSnapshotsVm.unsupported({this.driver = ''})
      : epoch = null,
        supported = false,
        readOnly = false,
        status = NativeSnapshotsStatus.unsupported,
        slots = const [],
        current = null;

  NativeSnapshotsVm._({
    required this.driver,
    this.epoch,
    required this.supported,
    required this.readOnly,
    required this.status,
    List<NativeSnapshotSlotVm> slots = const [],
    this.current,
  }) : slots = List.unmodifiable(slots);

  static NativeSnapshotsVm fromCaps(String driver, Object? raw) {
    if (raw is! Map || raw['supported'] is! bool || raw['read_only'] is! bool) {
      return NativeSnapshotsVm.unknown(driver: driver);
    }
    if (raw['supported'] == false) {
      return NativeSnapshotsVm.unsupported(driver: driver);
    }
    if (raw['read_only'] != true) {
      return NativeSnapshotsVm.unknown(driver: driver);
    }
    return NativeSnapshotsVm._(
        driver: driver,
        supported: true,
        readOnly: true,
        status: NativeSnapshotsStatus.loading);
  }

  NativeSnapshotsVm waiting({required bool connected}) => NativeSnapshotsVm._(
      driver: driver,
      supported: supported,
      readOnly: readOnly,
      status: connected
          ? NativeSnapshotsStatus.loading
          : NativeSnapshotsStatus.unavailable);

  static bool _isSlot(Object? value) =>
      value is int && value >= 0 && value <= 0xffffffff;

  /// Reject malformed or contradictory frames atomically. Native u32 ids
  /// (including zero) stay intact; no local library numbering or labels apply.
  static NativeSnapshotsVm? fromJson(Map<String, dynamic> e) {
    final driver = e['driver'];
    final epoch = e['epoch'];
    final supported = e['supported'], readOnly = e['read_only'];
    final rawSlots = e['slots'], current = e['current'];
    final status = switch (e['status']) {
      'unsupported' => NativeSnapshotsStatus.unsupported,
      'unavailable' => NativeSnapshotsStatus.unavailable,
      'loading' => NativeSnapshotsStatus.loading,
      'ready' => NativeSnapshotsStatus.ready,
      _ => null,
    };
    if (driver is! String ||
        driver.trim().isEmpty ||
        epoch is! int ||
        epoch < 0 ||
        supported is! bool ||
        readOnly is! bool ||
        status == null ||
        rawSlots is! List ||
        (current != null && !_isSlot(current)) ||
        (supported &&
            (!readOnly || status == NativeSnapshotsStatus.unsupported)) ||
        (!supported && status != NativeSnapshotsStatus.unsupported)) {
      return null;
    }
    final slots = <NativeSnapshotSlotVm>[];
    final ids = <int>{};
    for (final row in rawSlots) {
      if (row is! Map || !_isSlot(row['slot']) || row['name'] is! String) {
        return null;
      }
      final slot = row['slot'] as int;
      if (!ids.add(slot)) return null;
      slots.add(NativeSnapshotSlotVm._(slot, row['name'] as String));
    }
    if (status != NativeSnapshotsStatus.ready &&
        (slots.isNotEmpty || current != null)) {
      return null;
    }
    slots.sort((a, b) => a.slot.compareTo(b.slot));
    return NativeSnapshotsVm._(
        driver: driver,
        epoch: epoch,
        supported: supported,
        readOnly: readOnly,
        status: status,
        slots: slots,
        current: current as int?);
  }

  @override
  bool operator ==(Object other) =>
      other is NativeSnapshotsVm &&
      driver == other.driver &&
      epoch == other.epoch &&
      supported == other.supported &&
      readOnly == other.readOnly &&
      status == other.status &&
      current == other.current &&
      slots.length == other.slots.length &&
      Iterable<int>.generate(slots.length)
          .every((i) => slots[i] == other.slots[i]);

  @override
  int get hashCode => Object.hash(driver, epoch, supported, readOnly, status,
      current, Object.hashAll(slots));
}

/// Measured audio bands with explicit provenance. Never infer a channel from
/// the current selection: a null [ch] is a shared/global analyser feed.
class AudioSpectrumFrame {
  final int? ch;
  final List<double> binsDb;
  final List<double> bandEdgesHz;
  final String source;
  final String position;
  final int epoch;
  final DateTime receivedAt;

  AudioSpectrumFrame._(
      {required this.ch,
      required List<double> binsDb,
      required List<double> bandEdgesHz,
      required this.source,
      required this.position,
      required this.epoch,
      required this.receivedAt})
      : binsDb = List.unmodifiable(binsDb),
        bandEdgesHz = List.unmodifiable(bandEdgesHz);

  static const maxBins = 1024;
  static const expiry = Duration(milliseconds: 1500);
  bool get isShared => ch == null || position == 'shared';
  bool get isFresh => DateTime.now().difference(receivedAt) < expiry;

  /// Provider human text supplies the source name; no selector mapping here.
  String get label {
    final name = source
        .replaceFirst(RegExp(r'^DL32S\s+'), '')
        .replaceFirst(RegExp(r'\s*\(shared\)$'), '');
    return 'RTA · $name${isShared ? ' · shared' : ''}';
  }

  static AudioSpectrumFrame? fromJson(Map<String, dynamic> e) {
    final bins = e['bins_db'], edges = e['band_edges_hz'];
    final channel = e['ch'], epoch = e['epoch'];
    final source = e['source'], position = e['position'];
    if (bins is! List ||
        bins.isEmpty ||
        bins.length > maxBins ||
        edges is! List ||
        edges.length != bins.length + 1 ||
        epoch is! int ||
        epoch < 0 ||
        (channel != null && (channel is! int || channel < 0)) ||
        source is! String ||
        source.trim().isEmpty ||
        source == 'unavailable' ||
        position is! String ||
        bins.any((v) => v is! num || !v.isFinite) ||
        edges.any((v) => v is! num || !v.isFinite || v <= 0)) {
      return null;
    }
    final frequencies = [for (final v in edges) (v as num).toDouble()];
    for (var i = 1; i < frequencies.length; i++) {
      if (frequencies[i] <= frequencies[i - 1]) return null;
    }
    return AudioSpectrumFrame._(
        ch: channel as int?,
        binsDb: [for (final v in bins) (v as num).toDouble()],
        bandEdgesHz: frequencies,
        source: source.trim(),
        position: position,
        epoch: epoch,
        receivedAt: DateTime.now());
  }
}

/// One mixer strip as carried by the state event.
class Strip {
  final int ch;
  final String key;
  String name;
  double fader;
  bool muted;

  Strip(
      {required this.ch,
      required this.key,
      required this.name,
      required this.fader,
      required this.muted});

  factory Strip.fromJson(Map<String, dynamic> j) => Strip(
        ch: (j['ch'] as num).toInt(),
        key: (j['key'] as String?) ??
            'ch/${((j['ch'] as num).toInt() + 1).toString().padLeft(2, '0')}',
        name: (j['name'] as String?) ?? '',
        fader: ((j['fader'] as num?) ?? 0.0).toDouble(),
        muted: (j['muted'] as bool?) ?? false,
      );
}

/// Cmd JSON strings the engine accepts (see `Cmd::known_tags` in
/// `crates/engine/src/protocol.rs`).
class Cmds {
  static String connectSim() => '{"cmd":"connect","driver":"sim"}';
  static String connect([String driver = 'sim', String? addr]) => addr == null
      ? '{"cmd":"connect","driver":${_q(driver)}}'
      : '{"cmd":"connect","driver":${_q(driver)},"addr":${_q(addr)}}';
  static String scan() => '{"cmd":"scan"}';
  static String refreshDevices(String requestId, List<String> hosts) =>
      jsonEncode(
          {'cmd': 'refresh_devices', 'request_id': requestId, 'hosts': hosts});

  /// Local storage only; does not probe, connect, or disconnect a mixer.
  static String forgetSavedMixers(String requestId) {
    if (requestId.isEmpty) throw ArgumentError.value(requestId, 'requestId');
    return jsonEncode({'cmd': 'forget_saved_mixers', 'request_id': requestId});
  }

  static String discoverDevices(String requestId) =>
      jsonEncode({'cmd': 'discover_devices', 'request_id': requestId});
  static String probe(String addr) => '{"cmd":"probe","addr":${_q(addr)}}';
  static String probeDesk(String addr) => probe(addr);
  static String connectDevice(String id) =>
      '{"cmd":"connect_device","id":${_q(id)}}';
  static String disconnect() => '{"cmd":"disconnect"}';
  static String info() => '{"cmd":"info"}';
  static String stats() => '{"cmd":"stats"}';
  static String hello([bool binaryMeters = true, bool? taps]) => taps == null
      ? '{"cmd":"hello","binary_meters":$binaryMeters}'
      : '{"cmd":"hello","binary_meters":$binaryMeters,"taps":$taps}';
  static String fader(int ch, double value, [String? key]) =>
      '{"cmd":"fader","ch":$ch,"value":$value${key == null ? '' : ',"key":${_q(key)}'}}';
  static String mute(int ch, bool muted, [String? key]) =>
      '{"cmd":"mute","ch":$ch,"muted":$muted${key == null ? '' : ',"key":${_q(key)}'}}';
  static String mainFader(double value) =>
      '{"cmd":"main_fader","value":$value}';
  static String mainMute(bool muted) => '{"cmd":"main_mute","muted":$muted}';
  static String solo(int ch, bool soloed) =>
      '{"cmd":"solo","ch":$ch,"soloed":$soloed}';
  static String clearSolo() => '{"cmd":"clear_solo"}';
  static String param(int ch, String field, Object value) =>
      '{"cmd":"param","ch":$ch,"field":${_q(field)},"value":${jsonEncode(value)}}';
  static String spectrumTarget(int? ch) =>
      '{"cmd":"spectrum_target","ch":${ch ?? 'null'}}';
  static String focus(List<int> chs) => chs.length == 1
      ? '{"cmd":"focus","ch":${chs.first}}'
      : '{"cmd":"focus","chs":[${chs.join(',')}]}';
  static String lock(int ch) => '{"cmd":"lock","ch":$ch}';
  static String unlock(int ch) => '{"cmd":"unlock","ch":$ch}';
  static String listShows() => '{"cmd":"list_shows"}';
  static String localLibraryStatus() => '{"cmd":"local_library_status"}';
  static String localLibraryMutate(
      String requestId, int expectedRevision, LocalLibraryAction action) {
    if (requestId.isEmpty || expectedRevision < 0) {
      throw ArgumentError('A request id and current revision are required');
    }
    return jsonEncode({
      'cmd': 'local_library_mutate',
      'request_id': requestId,
      'expected_revision': expectedRevision,
      'action': action.toJson(),
    });
  }

  static String captureScene(String show, String name) =>
      '{"cmd":"capture_scene","show":${_q(show)},"name":${_q(name)}}';
  static String recallScene(String show, String name, [double? fadeMs]) =>
      '{"cmd":"recall_scene","show":${_q(show)},"name":${_q(name)},"fade_ms":${fadeMs ?? 'null'}}';
  static String deleteScene(String show, String name) =>
      '{"cmd":"delete_scene","show":${_q(show)},"name":${_q(name)}}';
  static String sceneSave(int n, String name) =>
      '{"cmd":"scene_save","n":$n,"name":${_q(name)}}';
  static String sceneRecall(int n, [double? fadeMs]) =>
      '{"cmd":"scene_recall","n":$n${fadeMs == null ? '' : ',"fade_ms":$fadeMs'}}';
  static String sceneRename(int n, String name) =>
      '{"cmd":"scene_rename","n":$n,"name":${_q(name)}}';
  static String sceneDelete(int n) => '{"cmd":"scene_delete","n":$n}';
  static String sceneClear() => '{"cmd":"scene_clear"}';
  static String fxSetType(int slot, String type) =>
      '{"cmd":"fx_set_type","slot":$slot,"type":${_q(type)}}';
  static String fxParam(int slot, String id, double value) =>
      '{"cmd":"fx_param","slot":$slot,"id":${_q(id)},"value":$value}';
  static String fxBypass(int slot, bool bypass) =>
      '{"cmd":"fx_bypass","slot":$slot,"bypass":$bypass}';
  static String fxOn(int slot, bool on) =>
      '{"cmd":"fx_on","slot":$slot,"on":$on}';
  static String patchSet(int strip, String group, int index) =>
      '{"cmd":"patch_set","strip":$strip,"group":${_q(group)},"index":$index}';
  static String patchClear(int strip) => '{"cmd":"patch_clear","strip":$strip}';
  static String offline(String model) =>
      '{"cmd":"offline","model":${_q(model)}}';
  static String offlinePush(bool confirm) =>
      '{"cmd":"offline_push","confirm":$confirm}';
  static String offlineDiscard() => '{"cmd":"offline_discard"}';
  static String busFader(int bus, double value) =>
      '{"cmd":"bus_fader","bus":$bus,"value":$value}';
  static String busMute(int bus, bool muted) =>
      '{"cmd":"bus_mute","bus":$bus,"muted":$muted}';
  static String subgroupFader(int group, double value) =>
      '{"cmd":"subgroup_fader","group":$group,"value":$value}';
  static String subgroupMute(int group, bool muted) =>
      '{"cmd":"subgroup_mute","group":$group,"muted":$muted}';
  static String subgroupAssign(int ch, int group, bool member) =>
      '{"cmd":"subgroup_assign","ch":$ch,"group":$group,"member":$member}';
  static String muteGroup(int group, bool muted) =>
      '{"cmd":"mute_group","group":$group,"muted":$muted}';
  static String muteGroupAssign(int ch, int group, bool member) =>
      '{"cmd":"mute_group_assign","ch":$ch,"group":$group,"member":$member}';
  static String fileDownload(String filename) =>
      '{"cmd":"file_download","filename":${_q(filename)}}';
  static String fileUpload(String filename, String data, int fileType) =>
      '{"cmd":"file_upload","filename":${_q(filename)},"data":${_q(data)},"file_type":$fileType}';

  static String _q(String s) =>
      '"${s.replaceAll('\\', '\\\\').replaceAll('"', '\\"')}"';
}

// ---------------------------------------------------------------------------
// EngineApi: transport behind the UI. `FfiEngine` drives the real embedded
// engine; the sim engine drives the same Cmd/Event JSON in memory.
// ---------------------------------------------------------------------------
abstract class EngineApi {
  Future<String> boot();
  Future<void> cmd(String clientId, String json);
  Future<List<String>> poll(String clientId, int cap);
  // Hot meter path: length-prefixed binary frames; non-meter events stay
  // on [poll] (JSON fallback, never lost).
  Future<Uint8List> pollBinary(String clientId, int cap);
  Future<void> resync(String clientId);
}

// ---------------------------------------------------------------------------
// Binary frames (SPEC 2a header + protocol.rs constants).
// ---------------------------------------------------------------------------
const int binTypeMeters = 1;
const int binTypeGr = 2;
const int binTypeTaps = 3;
const int binVersion = 1;
const int binHeaderLen = 12;

/// Decoded WS BINARY frame. `levels` for meters is linear 0..1; GR tap
/// levels are dB (24 dB full-scale); gate/comp are dB reduction.
class BinaryFrame {
  final String kind; // 'meters' | 'gr' | 'taps'
  final int seq;
  final int tMs;
  final int ch;
  final String tap;
  final List<double> levels;
  final double gate;
  final double comp;

  const BinaryFrame._({
    required this.kind,
    required this.seq,
    required this.tMs,
    this.ch = -1,
    this.tap = '',
    this.levels = const [],
    this.gate = double.nan,
    this.comp = double.nan,
  });

  factory BinaryFrame.meters(int seq, int tMs, List<double> levels) =>
      BinaryFrame._(kind: 'meters', seq: seq, tMs: tMs, levels: levels);
  factory BinaryFrame.gr(int seq, int tMs, int ch, double gate, double comp) =>
      BinaryFrame._(
          kind: 'gr', seq: seq, tMs: tMs, ch: ch, gate: gate, comp: comp);
  factory BinaryFrame.taps(int seq, int tMs, String tap, List<double> levels) =>
      BinaryFrame._(kind: 'taps', seq: seq, tMs: tMs, tap: tap, levels: levels);
}

const List<String> _tapNames = ['in', 'post', 'gr_gate', 'gr_comp'];

/// Decode one WS BINARY frame. Returns null for short or unknown frames.
BinaryFrame? parseBinary(Uint8List bytes) {
  if (bytes.length < binHeaderLen) return null;
  if (bytes[1] != binVersion) return null;
  final view = ByteData.sublistView(bytes);
  final type = bytes[0];
  final n = view.getUint16(2, Endian.little);
  final seq = view.getUint32(4, Endian.little);
  final tMs = view.getUint32(8, Endian.little);
  if (type == binTypeMeters) {
    if (bytes.length < binHeaderLen + n * 2) return null;
    return BinaryFrame.meters(seq, tMs, [
      for (var i = 0; i < n; i++)
        view.getUint16(binHeaderLen + i * 2, Endian.little) / 65535,
    ]);
  }
  if (type == binTypeGr) {
    if (n != 1 || bytes.length < binHeaderLen + 3) return null;
    return BinaryFrame.gr(seq, tMs, bytes[binHeaderLen],
        bytes[binHeaderLen + 1] / 4, bytes[binHeaderLen + 2] / 4);
  }
  if (type == binTypeTaps) {
    if (bytes.length < binHeaderLen + 1 + n * 2) return null;
    final tapId = bytes[binHeaderLen];
    final tap = tapId < _tapNames.length ? _tapNames[tapId] : 'unknown-$tapId';
    final gr = tap == 'gr_gate' || tap == 'gr_comp';
    return BinaryFrame.taps(seq, tMs, tap, [
      for (var i = 0; i < n; i++)
        view.getUint16(binHeaderLen + 1 + i * 2, Endian.little) /
            65535 *
            (gr ? 24 : 1),
    ]);
  }
  return null;
}

/// Inverse of the Rust `encode_binary_batch`
/// (`crates/flutter_bridge/src/api.rs`): repeated `[u32 LE len][frame]`.
/// A short trailing tail is dropped, so a torn read degrades gracefully.
List<Uint8List> splitBinaryBatch(Uint8List blob) {
  final out = <Uint8List>[];
  if (blob.isEmpty) return out;
  final view = ByteData.sublistView(blob);
  var offset = 0;
  while (offset + 4 <= blob.length) {
    final len = view.getUint32(offset, Endian.little);
    offset += 4;
    if (offset + len > blob.length) break;
    out.add(Uint8List.sublistView(blob, offset, offset + len));
    offset += len;
  }
  return out;
}

// ---------------------------------------------------------------------------
// EngineState: parsed snapshot of the engine frames the app renders.
// Unknown event types are ignored for interop.
// ---------------------------------------------------------------------------

class SceneEntry {
  final int number;
  final String name;
  final bool current;
  const SceneEntry(
      {required this.number, required this.name, this.current = false});
}

class DeviceEntry {
  final String id;
  final String name;
  final String state;
  final String ip;
  final String model;
  final String firmware;
  const DeviceEntry(
      {required this.id,
      required this.name,
      this.state = '',
      this.ip = '',
      this.model = '',
      this.firmware = ''});
}

class FxSlot {
  final int slot;
  final String type;
  final bool on;
  final double level;
  final bool bypass;
  final String returnLabel;
  final Map<String, double> params;
  FxSlot(
      {required this.slot,
      required this.type,
      this.on = true,
      this.level = 0,
      this.bypass = false,
      this.returnLabel = '',
      Map<String, double>? params})
      : params = params ?? {};
}

class OfflineChange {
  final int strip;
  final String id;
  final Object? from;
  final Object? to;
  const OfflineChange(
      {required this.strip, required this.id, this.from, this.to});
}

class EngineState {
  String clientId = '';
  String driver = 'sim';
  List<Strip> strips = [];
  List<double> meters = [];
  List<Map<String, dynamic>> topology = [];
  Map<String, dynamic> routingCaps = {};
  List<Map<String, dynamic>> fxTypes = [];
  String linkIp = '';
  String model = '';
  String firmware = '';
  int sampleRate = 0;
  int clientCount = 0;
  List<String> writers = [];

  // Extended R2 surface (all optional; absent until the engine sends them).
  double? mainFader;
  bool? mainMuted;
  Set<int> solos = {};
  Map<int, Map<String, Object>> params = {};
  Map<String, List<double>> taps = {}; // tap name -> levels
  Map<int, List<double>> spectrum = {}; // legacy demo magnitudes only
  AudioSpectrumFrame? audioSpectrum;
  int? _audioSpectrumEpoch;

  /// Clear presentation without forgetting the source epoch watermark.
  void clearAudioSpectrum() {
    audioSpectrum = null;
  }

  Map<int, double> grGate = {};
  Map<int, double> grComp = {};
  Map<int, double> busFaders = {};
  Map<int, bool> busMuted = {};
  Map<int, double> subgroupFaders = {};
  Map<int, bool> subgroupMuted = {};
  Map<int, bool> muteGroups = {};
  Map<String, bool> subgroupAssign = {};
  Map<String, bool> muteGroupAssign = {};
  Map<int, String> locks = {};
  LocalLibraryStatus? localLibrary;
  int _localLibraryRevision = 0;

  /// Called for engine attachment only, never for desk or channel changes.
  void resetLocalLibraryStatus() {
    localLibrary = null;
    _localLibraryRevision = 0;
  }

  void invalidateLocalLibraryStatus(int revision) {
    if (revision > _localLibraryRevision) _localLibraryRevision = revision;
    if (localLibrary != null && localLibrary!.revision < revision) {
      localLibrary = null;
    }
  }

  List<SceneEntry> scenes = [];
  Map<String, List<String>> shows = {};
  NativeSnapshotsVm? _nativeSnapshots;
  NativeSnapshotsVm? _nativeSnapshotsCaps;

  NativeSnapshotsVm get nativeSnapshots {
    if (driver == 'sim') {
      return const NativeSnapshotsVm.unsupported(driver: 'sim');
    }
    if (_nativeSnapshots != null) return _nativeSnapshots!;
    final caps = _nativeSnapshotsCaps;
    return caps?.supported == true
        ? caps!.waiting(connected: connected)
        : caps ?? NativeSnapshotsVm.unknown(driver: driver);
  }

  List<FxSlot> fx = [];
  List<Map<String, dynamic>> routing = [];
  List<DeviceEntry> devices = [];
  List<OfflineChange> offlineDiff = [];
  String linkState = '';
  String linkName = '';
  String error = '';
  String? errorLockedBy;
  int? errorCh;

  /// Latest explicitly correlated discovery/refresh completion. Parents match
  /// request_id; unrelated or old operations cannot complete their spinner.
  Map<String, dynamic>? deviceOperationComplete;

  // Incremented when a desk session ends, including same-driver reconnects.
  int session = 0;
  int? _syncEpoch;
  final Map<int, Set<String>> knownValues = {};

  bool valueKnown(int ch, String field) =>
      knownValues[ch]?.contains(field == 'muted' ? 'mute' : field) ?? false;

  void _noteKnown(int ch, Iterable<String> fields) {
    knownValues
        .putIfAbsent(ch, () => {})
        .addAll(fields.map((field) => field == 'muted' ? 'mute' : field));
  }

  bool capsReady = false;
  int muteGroupCount = 0;
  int subgroupCount = 0;
  Set<String> supportedParams = {};

  /// Full advertised catalogue rows; availability is separate from hydration.
  Map<String, Map<String, dynamic>> parameterMetadata = {};
  Set<String> meterTaps = {};
  int meterSeq = 0;

  bool get connected =>
      driver == 'sim' ? strips.isNotEmpty : linkState == 'connected';

  /// Discard values before a new connection, even to the same driver/IP.
  void resetSession() => _resetDesk();

  void _resetDesk() {
    session++;
    _nativeSnapshots = null;
    _nativeSnapshotsCaps = null;
    knownValues.clear();
    capsReady = false;
    supportedParams.clear();
    parameterMetadata = {};
    meterTaps.clear();
    strips = [];
    topology = [];
    routingCaps = {};
    fxTypes = [];
    meters = [];
    params.clear();
    taps.clear();
    spectrum.clear();
    audioSpectrum = null;
    _audioSpectrumEpoch = null;
    grGate.clear();
    grComp.clear();
    busFaders.clear();
    busMuted.clear();
    subgroupFaders.clear();
    subgroupMuted.clear();
    subgroupAssign.clear();
    muteGroupAssign.clear();
    muteGroups.clear();
    muteGroupCount = 0;
    subgroupCount = 0;
    solos.clear();
    locks.clear();
    fx = [];
    routing = [];
    mainFader = null;
    mainMuted = null;
    linkState = '';
    linkName = '';
    linkIp = '';
    model = '';
    firmware = '';
    error = '';
  }

  void _applyStripValues(int ch, Map<String, Object> values) {
    _noteKnown(ch, values.keys);
    params.putIfAbsent(ch, () => {}).addAll(values);
    final solo = values['solo'];
    if (solo is bool) {
      if (solo) {
        solos.add(ch);
      } else {
        solos.remove(ch);
      }
    }
    for (final strip in strips) {
      if (strip.ch != ch) continue;
      final fader = values['fader'];
      final mute = values['mute'] ?? values['muted'];
      final name = values['name'];
      if (fader is num) strip.fader = fader.toDouble();
      if (mute is bool) strip.muted = mute;
      if (name is String) strip.name = name;
      if (strip.key == 'main') {
        if (fader is num) mainFader = fader.toDouble();
        if (mute is bool) mainMuted = mute;
      }
    }
  }

  /// Apply one decoded Event frame. Returns true when the UI should rebuild.
  bool applyFrame(Map<String, dynamic> e) {
    switch (e['type']) {
      case 'sync_epoch':
        final epoch = (e['epoch'] as num?)?.toInt();
        if (epoch == null || epoch == _syncEpoch) return false;
        // A new client's first epoch terminates its initial state replay.
        // Only a later epoch invalidates state from an older connection.
        final hadEpoch = _syncEpoch != null;
        _syncEpoch = epoch;
        if (!hadEpoch) {
          // Initial attach publishes native observations before its epoch.
          // Preserve only observations belonging to that same replay.
          if (_nativeSnapshots != null && _nativeSnapshots!.epoch != epoch) {
            _nativeSnapshots = null;
            return true;
          }
          return false;
        }
        _resetDesk();
        return true;
      case 'you':
        clientId = (e['id'] as String?) ?? clientId;
        return false;
      case 'state':
        final nextDriver = e['driver'] as String? ?? driver;
        if (nextDriver != driver) _resetDesk();
        driver = nextDriver;
        final raw = (e['strips'] as List?) ?? const [];
        // Hardware drivers publish geometry in caps and live values in
        // params. An empty legacy snapshot must not erase that surface.
        if (raw.isNotEmpty || topology.isEmpty) {
          strips = [
            for (final row in raw)
              if (row is Map<String, dynamic>) Strip.fromJson(row)
          ];
          // Snapshot values supersede older cached echoes. Keep unrelated
          // param-plane fields while hydrating the values actually present.
          for (final row in raw) {
            if (row is! Map || row['ch'] is! num) continue;
            final ch = (row['ch'] as num).toInt();
            _applyStripValues(ch, {
              if (row['fader'] is num) 'fader': row['fader'] as num,
              if ((row['muted'] ?? row['mute']) is bool)
                'mute': (row['muted'] ?? row['mute']) as bool,
              if (row['name'] is String) 'name': row['name'] as String,
            });
          }
          for (final entry in params.entries.toList()) {
            _applyStripValues(entry.key, entry.value);
          }
        }
        final mf = e['main_fader'];
        if (mf is num) mainFader = mf.toDouble().clamp(0.0, 1.0);
        return true;
      case 'param':
        final ch = (e['ch'] as num?)?.toInt();
        final field = e['field'] as String?;
        if (ch == null || field == null) return false;
        final v = e['value'];
        if (v is! String && v is! bool && v is! num) return false;
        _applyStripValues(ch, {field: v as Object});
        return true;
      case 'params':
        final ch = (e['ch'] as num?)?.toInt();
        final values = e['values'];
        if (ch == null || values is! Map) return false;
        _applyStripValues(ch, {
          for (final entry in values.entries)
            if (entry.key is String && entry.value != null)
              entry.key as String: entry.value as Object,
        });
        return true;
      case 'device_operation_complete':
        deviceOperationComplete = Map<String, dynamic>.unmodifiable(e);
        return true;
      case 'caps':
        final nextDriver = e['driver'] as String? ?? driver;
        if (nextDriver != driver) _resetDesk();
        driver = nextDriver;
        capsReady = true;
        final nativeCaps =
            NativeSnapshotsVm.fromCaps(driver, e['native_snapshots']);
        if (!nativeCaps.supported) _nativeSnapshots = null;
        _nativeSnapshotsCaps = nativeCaps;
        meterTaps = {
          for (final tap in e['meter_taps'] as List? ?? const [])
            if (tap is String) tap,
        };
        final groups = e['groups'];
        muteGroupCount =
            groups is Map ? (groups['mute_groups'] as num?)?.toInt() ?? 0 : 0;
        subgroupCount = groups is Map
            ? ((groups['subgroups'] as num?)?.toInt() ?? 0).clamp(0, 255)
            : 0;
        routingCaps = e['routing'] is Map<String, dynamic>
            ? e['routing'] as Map<String, dynamic>
            : {};
        final types = e['fx'] is Map ? (e['fx'] as Map)['types'] : null;
        fxTypes = [
          for (final type in types is List ? types : const [])
            if (type is Map<String, dynamic>) type
        ];
        final families = e['topology'];
        if (families is List) {
          final previous = {for (final strip in strips) strip.key: strip};
          topology = [
            for (final f in families)
              if (f is Map<String, dynamic>) f
          ];
          strips = [
            for (final f in topology)
              for (var i = 0; i < ((f['count'] as num?)?.toInt() ?? 0); i++)
                (() {
                  final ch = ((f['first'] as num?)?.toInt() ?? 0) + i;
                  final keys = f['strip_keys'] as List?;
                  final key = keys != null && i < keys.length
                      ? keys[i] as String
                      : '${f['key']}/${(i + 1).toString().padLeft(2, '0')}';
                  return previous[key] ??
                      Strip(
                          ch: ch,
                          key: key,
                          name:
                              '${f['name_prefix'] ?? f['label'] ?? 'CH'}${i + 1}',
                          fader: 0,
                          muted: false);
                })(),
          ];
          for (final entry in params.entries.toList()) {
            _applyStripValues(entry.key, entry.value);
          }
        }
        final ps = e['params'];
        if (ps is List) {
          parameterMetadata = Map.unmodifiable({
            for (final p in ps)
              if (p is Map && p['id'] is String)
                p['id'] as String: Map<String, dynamic>.unmodifiable(
                    Map<String, dynamic>.from(p)),
          });
          supportedParams = parameterMetadata.keys.toSet();
        }
        return true;
      case 'meters':
        final raw = (e['levels'] as List?) ?? const [];
        meters = [for (final v in raw) ((v as num?) ?? 0.0).toDouble()];
        return false; // meter frames ride notifiers, never rebuild
      case 'audio_spectrum':
        final epoch = e['epoch'];
        if (epoch is! int ||
            epoch < 0 ||
            (_audioSpectrumEpoch != null && epoch < _audioSpectrumEpoch!)) {
          return false;
        }
        _audioSpectrumEpoch = epoch;
        audioSpectrum = AudioSpectrumFrame.fromJson(e);
        return false; // measured frames ride their own presentation notifier
      case 'spectrum':
        final ch = (e['ch'] as num?)?.toInt();
        final bins = e['bins'];
        if (ch == null || bins is! List) return false;
        spectrum[ch] = [for (final v in bins) ((v as num?) ?? 0.0).toDouble()];
        return false;
      case 'gr':
        final ch = (e['ch'] as num?)?.toInt();
        if (ch == null) return false;
        final g = e['gate'], c = e['comp'];
        if (g is num) grGate[ch] = g.toDouble();
        if (c is num) grComp[ch] = c.toDouble();
        return false;
      case 'taps':
        final tap = e['tap'] as String?;
        final raw = e['levels'];
        if (tap == null || raw is! List) return false;
        taps[tap] = [for (final v in raw) ((v as num?) ?? 0.0).toDouble()];
        return false;
      case 'bus':
        final b = (e['bus'] as num?)?.toInt();
        if (b == null) return false;
        final f = e['fader'], m = e['muted'];
        if (f is num) busFaders[b] = f.toDouble();
        if (m is bool) busMuted[b] = m;
        return true;
      case 'subgroup':
        final g = (e['group'] as num?)?.toInt();
        if (g == null) return false;
        final f = e['fader'], m = e['muted'];
        if (f is num) subgroupFaders[g] = f.toDouble();
        if (m is bool) subgroupMuted[g] = m;
        return true;
      case 'subgroup_assign':
        final ch = (e['ch'] as num?)?.toInt();
        final g = (e['group'] as num?)?.toInt();
        if (ch == null || g == null) return false;
        subgroupAssign['$ch:$g'] = (e['member'] as bool?) ?? false;
        return true;
      case 'mute_group':
        final g = (e['group'] as num?)?.toInt();
        if (g == null) return false;
        muteGroups[g] = (e['muted'] as bool?) ?? false;
        return true;
      case 'mute_group_assign':
        final ch = (e['ch'] as num?)?.toInt();
        final g = (e['group'] as num?)?.toInt();
        if (ch == null || g == null) return false;
        muteGroupAssign['$ch:$g'] = (e['member'] as bool?) ?? false;
        return true;
      case 'main':
        final f = e['fader'], m = e['muted'];
        if (f is num) mainFader = f.toDouble();
        if (m is bool) mainMuted = m;
        return true;
      case 'solo':
        final ch = (e['ch'] as num?)?.toInt();
        if (ch == null) return false;
        _noteKnown(ch, ['solo']);
        if ((e['soloed'] as bool?) ?? false) {
          solos.add(ch);
        } else {
          solos.remove(ch);
        }
        return true;
      case 'local_library_status':
        final library = LocalLibraryStatus.fromJson(e);
        if (library == null || library.revision < _localLibraryRevision) {
          return false;
        }
        final changed = localLibrary != library;
        _localLibraryRevision = library.revision;
        localLibrary = library;
        return changed;
      case 'shows':
        shows = {
          for (final show in e['shows'] as List? ?? const [])
            if (show is Map && show['name'] is String)
              show['name'] as String: [
                for (final scene in show['scenes'] as List? ?? const [])
                  if (scene is Map && scene['name'] is String)
                    scene['name'] as String,
              ],
        };
        return true;
      case 'native_snapshots':
        final native = NativeSnapshotsVm.fromJson(e);
        if (native == null ||
            native.driver != driver ||
            (_syncEpoch != null && native.epoch != _syncEpoch)) {
          return false;
        }
        // Attach/resume send link before native reports. Ignore late live
        // observations after disconnect; only a new connected session may
        // establish them again. Epoch equality also rejects a late frame
        // from an earlier connection to the same driver.
        if (!connected &&
            (native.status == NativeSnapshotsStatus.ready ||
                native.status == NativeSnapshotsStatus.loading)) {
          return false;
        }
        if (driver == 'sim' && native.supported) return false;
        final before = nativeSnapshots;
        _nativeSnapshots = native;
        _nativeSnapshotsCaps = native.supported
            ? native.waiting(connected: connected)
            : NativeSnapshotsVm.unsupported(driver: driver);
        return nativeSnapshots != before;
      case 'scenes':
        final raw = (e['scenes'] as List?) ?? const [];
        scenes = [
          for (final s in raw)
            if (s is Map)
              SceneEntry(
                number: (s['number'] as num?)?.toInt() ?? 0,
                name: (s['name'] as String?) ?? '',
                current: (s['current'] as bool?) ?? false,
              )
        ];
        return true;
      case 'fx':
        final raw = (e['slots'] as List?) ?? const [];
        fx = [
          for (final s in raw)
            if (s is Map)
              FxSlot(
                slot: (s['slot'] as num?)?.toInt() ?? 0,
                type: (s['type'] as String?) ?? '',
                on: (s['on'] as bool?) ?? true,
                bypass: s['bypass'] as bool? ?? false,
                returnLabel: s['ret_label'] as String? ?? '',
                level: ((s['level'] as num?) ?? 0).toDouble(),
                params: s['params'] is Map
                    ? {
                        for (final kv in (s['params'] as Map).entries)
                          if (kv.key is String && kv.value is num)
                            kv.key as String: (kv.value as num).toDouble()
                      }
                    : null,
              )
        ];
        return true;
      case 'routing':
        final raw = (e['patches'] as List?) ?? const [];
        routing = [
          for (final p in raw)
            if (p is Map)
              {for (final kv in p.entries) kv.key.toString(): kv.value}
        ];
        return true;
      case 'devices':
        final raw = (e['list'] as List?) ?? const [];
        devices = [
          for (final d in raw)
            if (d is Map && d['id'] is String)
              DeviceEntry(
                id: d['id'] as String,
                name: (d['name'] as String?) ?? d['id'] as String,
                state: (d['state'] as String?) ?? '',
                ip: d['ip'] as String? ?? '',
                model: d['model'] as String? ?? '',
                firmware: d['fw'] as String? ?? '',
              )
        ];
        return true;
      case 'info':
        model = e['model'] as String? ?? model;
        firmware = e['firmware'] as String? ?? firmware;
        sampleRate = (e['sample_rate'] as num?)?.toInt() ?? sampleRate;
        return true;
      case 'clients':
        clientCount = (e['count'] as num?)?.toInt() ?? 0;
        writers = [
          for (final w in e['writers'] as List? ?? const [])
            if (w is String) w
        ];
        return true;
      case 'link':
        linkIp = e['ip'] as String? ?? '';
        linkState = (e['state'] as String?) ?? linkState;
        if (linkState != 'connected') _nativeSnapshots = null;
        linkName = (e['name'] as String?) ?? linkName;
        return true;
      case 'locks':
        final holds = e['holds'];
        if (holds is! Map) return false;
        locks = {
          for (final kv in holds.entries)
            if (int.tryParse(kv.key.toString()) != null && kv.value is String)
              int.parse(kv.key.toString()): kv.value as String
        };
        return true;
      case 'offline_diff':
        final raw = (e['changes'] as List?) ?? const [];
        offlineDiff = [
          for (final c in raw)
            if (c is Map)
              OfflineChange(
                strip: (c['strip'] as num?)?.toInt() ?? 0,
                id: (c['id'] as String?) ?? '',
                from: c['from'] as Object?,
                to: c['to'] as Object?,
              )
        ];
        return true;
      case 'error':
        error = (e['message'] as String?) ?? error;
        errorLockedBy = e['locked_by'] as String?;
        errorCh = (e['ch'] as num?)?.toInt();
        return true;
      default:
        return false;
    }
  }

  /// Apply one decoded BINARY frame (meters/GR/taps). Never rebuilds.
  void applyBinary(BinaryFrame f) {
    meterSeq = f.seq;
    switch (f.kind) {
      case 'meters':
        meters = List.of(f.levels);
        break;
      case 'gr':
        if (f.ch >= 0) {
          grGate[f.ch] = f.gate;
          grComp[f.ch] = f.comp;
        }
        break;
      case 'taps':
        taps[f.tap] = List.of(f.levels);
        break;
    }
  }
}
